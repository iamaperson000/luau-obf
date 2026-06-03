use crate::opmap::OpMap;
use luau_lir::{BlockLabel, LirFunction, LirInstr, OpKind, Operand, UpvalSource};
use rand::seq::SliceRandom;
use std::collections::HashMap;

/// Compute the keystream byte for a single bytecode position.
/// pc is 1-based (matches the VM's `_sbyte(code, pc)` semantics).
pub(crate) fn keystream_byte(pc: u32, proto_id: u32, k0: u32, k1: u32) -> u8 {
    let mixed = pc
        .wrapping_mul(k1)
        .wrapping_add(k0)
        .wrapping_add(proto_id.wrapping_mul(2_654_435_761));
    (mixed & 0xFF) as u8
}

/// Generate a random permutation of opcode bytes 1..=n_ops for a single proto.
/// Returns (perm, inv) where:
///   perm[c] = the raw byte to emit for canonical opcode-index c (0-based slice
///             indexed by canonical_byte - 1).
///   inv[r-1] = the canonical opcode byte for raw byte r (1-based both).
pub(crate) fn make_opcode_permutation(
    n_ops: usize,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> (Vec<u8>, Vec<u8>) {
    let mut perm: Vec<u8> = (1..=(n_ops as u8)).collect();
    perm.shuffle(rng);
    let mut inv = vec![0u8; n_ops];
    for (canonical_zero_based, &raw) in perm.iter().enumerate() {
        // raw is the byte emitted for canonical (canonical_zero_based + 1).
        // inv[raw - 1] = canonical_zero_based + 1.
        inv[(raw - 1) as usize] = (canonical_zero_based + 1) as u8;
    }
    (perm, inv)
}

pub fn encode_function(
    f: &LirFunction,
    opmap: &OpMap,
    proto_id: u32,
    k0: u32,
    k1: u32,
    perm: &[u8],
    inv: &[u8],
) -> Vec<u8> {
    // Prologue: num_params:u8, num_regs:u16_le, num_upvals:u8, is_vararg:u8,
    // then 35 bytes of inverse opcode permutation (raw_byte → canonical_byte).
    // The instruction stream begins at byte offset 40 (pc=41 in 1-based terms).
    const PROLOGUE_LEN: u32 = 40;

    let mut instr_byte_offsets: Vec<u32> = Vec::with_capacity(f.instrs.len() + 1);
    let mut offset: u32 = PROLOGUE_LEN;
    let mut closure_counter: usize = 0;
    let mut build_results_counter: usize = 0;
    for instr in &f.instrs {
        instr_byte_offsets.push(offset);
        offset += instr_size_for(instr, f, &mut closure_counter, &mut build_results_counter);
    }
    instr_byte_offsets.push(offset);

    let mut label_byte: HashMap<BlockLabel, u32> = HashMap::new();
    for (label, instr_idx) in &f.label_positions {
        let byte_off = if (*instr_idx as usize) < instr_byte_offsets.len() {
            instr_byte_offsets[*instr_idx as usize]
        } else {
            *instr_byte_offsets.last().unwrap()
        };
        label_byte.insert(*label, byte_off);
    }

    let mut closure_emit: usize = 0;
    let mut br_emit: usize = 0;
    let mut out: Vec<u8> = Vec::with_capacity(offset as usize);
    // Emit the 5-byte META prologue + 35-byte inv table; the XOR pass at the
    // bottom will encrypt all 40 prologue bytes along with the instruction
    // stream.
    out.push(f.num_params as u8);
    out.push((f.num_regs & 0xFF) as u8);
    out.push(((f.num_regs >> 8) & 0xFF) as u8);
    out.push(f.num_upvals as u8);
    out.push(if f.is_vararg { 1 } else { 0 });
    out.extend_from_slice(inv);
    for (i, instr) in f.instrs.iter().enumerate() {
        let canonical = opmap.opcode_of(instr.op);
        let emitted = perm[(canonical - 1) as usize];
        out.push(emitted);
        let after_this = instr_byte_offsets[i] + instr_size_for_at(instr, f, closure_emit, br_emit);
        for operand in &instr.operands {
            match operand {
                Operand::Reg(r) => push_u16(&mut out, r.0),
                Operand::Const(c) => push_u16(&mut out, c.0),
                Operand::Proto(p) => push_u16(&mut out, p.0),
                Operand::SmallInt(n) => push_u16(&mut out, *n as u16),
                Operand::UpvalIdx(u) => push_u16(&mut out, *u),
                Operand::JmpTarget(label) => {
                    let target_byte = *label_byte.get(label).expect("label");
                    let delta = target_byte as i32 - after_this as i32;
                    push_u16(&mut out, delta as i16 as u16);
                }
            }
        }
        if instr.op == OpKind::Closure {
            let sources = &f.closure_upval_sources[closure_emit];
            closure_emit += 1;
            push_u16(&mut out, sources.len() as u16);
            for src in sources {
                match src {
                    UpvalSource::LocalReg(r) => {
                        out.push(0);
                        push_u16(&mut out, *r);
                    }
                    UpvalSource::ParentUpval(u) => {
                        out.push(1);
                        push_u16(&mut out, *u);
                    }
                }
            }
        } else if instr.op == OpKind::BuildResults {
            let values = &f.build_results_values[br_emit];
            br_emit += 1;
            push_u16(&mut out, values.len() as u16);
            for v in values {
                let Operand::Reg(r) = v else { panic!("BuildResults value must be a Reg") };
                push_u16(&mut out, r.0);
            }
        }
    }
    // XOR every byte by its keystream (1-based pc).
    for (i, byte) in out.iter_mut().enumerate() {
        let pc = (i + 1) as u32;
        *byte ^= keystream_byte(pc, proto_id, k0, k1);
    }
    out
}

fn instr_size_for(
    i: &LirInstr,
    f: &LirFunction,
    closure_counter: &mut usize,
    br_counter: &mut usize,
) -> u32 {
    let base = 1 + (i.operands.len() as u32) * 2;
    if i.op == OpKind::Closure {
        let sources = &f.closure_upval_sources[*closure_counter];
        *closure_counter += 1;
        return base + 2 + (sources.len() as u32) * 3;
    }
    if i.op == OpKind::BuildResults {
        let values = &f.build_results_values[*br_counter];
        *br_counter += 1;
        return base + 2 + (values.len() as u32) * 2;
    }
    base
}

fn instr_size_for_at(
    i: &LirInstr,
    f: &LirFunction,
    closure_idx: usize,
    br_idx: usize,
) -> u32 {
    let base = 1 + (i.operands.len() as u32) * 2;
    if i.op == OpKind::Closure {
        let sources = &f.closure_upval_sources[closure_idx];
        return base + 2 + (sources.len() as u32) * 3;
    }
    if i.op == OpKind::BuildResults {
        let values = &f.build_results_values[br_idx];
        return base + 2 + (values.len() as u32) * 2;
    }
    base
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.push((v & 0xFF) as u8);
    out.push(((v >> 8) & 0xFF) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opmap::OpMap;
    use luau_lir::{LirFunction, LirInstr, OpKind, Operand, ProtoId, Reg};

    #[test]
    fn encodes_single_return_instruction() {
        let f = LirFunction {
            id: ProtoId(0),
            num_params: 0,
            num_regs: 0,
            num_upvals: 0,
            is_vararg: false,
            consts: vec![],
            instrs: vec![LirInstr { op: OpKind::Return, operands: vec![Operand::Reg(Reg(0xFFFF))] }],
            label_positions: vec![],
            closure_upval_sources: vec![],
            build_results_values: vec![],
        };
        let opmap = OpMap::new(&[0u8; 32]);
        let n_ops = 35;
        let perm: Vec<u8> = (1..=n_ops as u8).collect();
        let inv = perm.clone(); // identity permutation is self-inverse
        let bytes = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv);
        // 5-byte META + 35-byte inv + 1 opcode byte + 2 operand bytes = 43.
        assert_eq!(bytes.len(), 43);
        // With k0=0 and k1=0 the keystream is all zero (pid=0), so prologue
        // bytes appear unmodified.
        assert_eq!(bytes[0], 0);       // num_params
        assert_eq!(bytes[1], 0);       // num_regs lo
        assert_eq!(bytes[2], 0);       // num_regs hi
        assert_eq!(bytes[3], 0);       // num_upvals
        assert_eq!(bytes[4], 0);       // is_vararg
        // bytes[5..40] = identity inv table = 1, 2, ..., 35.
        for i in 0..35 {
            assert_eq!(bytes[5 + i], (i + 1) as u8);
        }
        assert_eq!(bytes[40], opmap.opcode_of(OpKind::Return));
        assert_eq!(bytes[41], 0xFF);
        assert_eq!(bytes[42], 0xFF);
    }
}
