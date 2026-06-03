use crate::opmap::OpMap;
use luau_lir::{BlockLabel, LirFunction, LirInstr, OpKind, Operand, UpvalSource};
use rand::seq::SliceRandom;
use rand::Rng;
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
    rng: &mut rand_chacha::ChaCha20Rng,
) -> Vec<u8> {
    // Prologue: num_params:u8, num_regs:u16_le, num_upvals:u8, is_vararg:u8,
    // then N bytes of inverse opcode permutation (raw_byte → canonical_byte),
    // where N = inv.len() = ALL_OPS.len() (36 after Plan 30).
    // The instruction stream begins at byte offset (5 + N) (pc = 5 + N + 1).
    let prologue_len: u32 = 5 + inv.len() as u32;

    // --- Pre-pass: decide which adjacent LoadConst pairs to fuse. ---
    // Build set of instruction indices that are jump targets.
    let label_target_idx: std::collections::HashSet<u32> =
        f.label_positions.iter().map(|(_, i)| *i).collect();

    // fused[i] = true means instruction i is the START of a fused LCLC pair
    // (it and i+1 are consumed together as a single 9-byte LCLC instruction).
    let mut fused: Vec<bool> = vec![false; f.instrs.len()];
    {
        let mut i = 0usize;
        while i + 1 < f.instrs.len() {
            let a = &f.instrs[i];
            let b = &f.instrs[i + 1];
            let eligible =
                matches!(a.op, OpKind::LoadConst)
                && matches!(b.op, OpKind::LoadConst)
                && a.operands.len() == 2
                && b.operands.len() == 2
                && matches!(a.operands[0], Operand::Reg(_))
                && matches!(a.operands[1], Operand::Const(_))
                && matches!(b.operands[0], Operand::Reg(_))
                && matches!(b.operands[1], Operand::Const(_))
                && !label_target_idx.contains(&((i + 1) as u32));
            // Always draw the coin so rng schedule is independent of eligibility.
            let coin: u8 = rng.gen();
            if eligible && (coin & 1) == 0 {
                fused[i] = true;
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    // --- Size pass that respects fusion. ---
    let mut instr_byte_offsets: Vec<u32> = Vec::with_capacity(f.instrs.len() + 1);
    let mut offset: u32 = prologue_len;
    let mut closure_counter: usize = 0;
    let mut build_results_counter: usize = 0;
    {
        let mut i = 0usize;
        while i < f.instrs.len() {
            instr_byte_offsets.push(offset);
            if fused[i] {
                // LCLC: 1 opcode byte + 4 u16 operands = 9 bytes.
                offset += 9;
                // Record the same start offset for i+1 (it's consumed by LCLC);
                // label lookups for i+1 are safe because the jump-target guard
                // above ensures i+1 is never a label target when fused[i]=true.
                instr_byte_offsets.push(offset);
                i += 2;
            } else {
                offset += instr_size_for(
                    &f.instrs[i], f, &mut closure_counter, &mut build_results_counter,
                );
                i += 1;
            }
        }
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

    // --- Emit pass. ---
    let mut closure_emit: usize = 0;
    let mut br_emit: usize = 0;
    let mut out: Vec<u8> = Vec::with_capacity(offset as usize);
    // Emit the 5-byte META prologue + N-byte inv table; the XOR pass at the
    // bottom will encrypt all prologue bytes along with the instruction stream.
    out.push(f.num_params as u8);
    out.push((f.num_regs & 0xFF) as u8);
    out.push(((f.num_regs >> 8) & 0xFF) as u8);
    out.push(f.num_upvals as u8);
    out.push(if f.is_vararg { 1 } else { 0 });
    out.extend_from_slice(inv);

    let mut i = 0usize;
    while i < f.instrs.len() {
        if fused[i] {
            // Emit LCLC superop: one opcode byte + 4 u16 operands (a1,b1,a2,b2).
            let canonical = opmap.opcode_of(OpKind::LoadConstLoadConst);
            let emitted = perm[(canonical - 1) as usize];
            out.push(emitted);
            // Operands from instruction i (a1, b1) then instruction i+1 (a2, b2).
            for op in &f.instrs[i].operands {
                push_operand_simple(&mut out, op);
            }
            for op in &f.instrs[i + 1].operands {
                push_operand_simple(&mut out, op);
            }
            i += 2;
            continue;
        }
        let instr = &f.instrs[i];
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
        i += 1;
    }
    // XOR every byte by its keystream (1-based pc).
    for (idx, byte) in out.iter_mut().enumerate() {
        let pc = (idx + 1) as u32;
        *byte ^= keystream_byte(pc, proto_id, k0, k1);
    }
    out
}

/// Push a single operand as a u16. Used for LCLC superop operands which are
/// always Reg or Const (never JmpTarget, so no relocation is needed).
fn push_operand_simple(out: &mut Vec<u8>, op: &Operand) {
    let v: u16 = match op {
        Operand::Reg(r) => r.0,
        Operand::Const(c) => c.0,
        Operand::Proto(p) => p.0,
        Operand::SmallInt(n) => *n as u16,
        Operand::UpvalIdx(u) => *u,
        Operand::JmpTarget(_) => panic!("LCLC does not accept JmpTarget operands"),
    };
    push_u16(out, v);
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
    use crate::opmap::{OpMap, ALL_OPS};
    use luau_lir::{ConstIdx, LirFunction, LirInstr, OpKind, Operand, ProtoId, Reg};
    use rand::SeedableRng;

    fn make_rng() -> rand_chacha::ChaCha20Rng {
        rand_chacha::ChaCha20Rng::from_seed([0u8; 32])
    }

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
        let n_ops = ALL_OPS.len();
        let perm: Vec<u8> = (1..=n_ops as u8).collect();
        let inv = perm.clone(); // identity permutation is self-inverse
        let mut rng = make_rng();
        let bytes = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv, &mut rng);
        // 5-byte META + 36-byte inv + 1 opcode byte + 2 operand bytes = 44.
        assert_eq!(bytes.len(), 44);
        // With k0=0 and k1=0 the keystream is all zero (pid=0), so prologue
        // bytes appear unmodified.
        assert_eq!(bytes[0], 0);       // num_params
        assert_eq!(bytes[1], 0);       // num_regs lo
        assert_eq!(bytes[2], 0);       // num_regs hi
        assert_eq!(bytes[3], 0);       // num_upvals
        assert_eq!(bytes[4], 0);       // is_vararg
        // bytes[5..41] = identity inv table = 1, 2, ..., 36.
        for i in 0..36 {
            assert_eq!(bytes[5 + i], (i + 1) as u8);
        }
        assert_eq!(bytes[41], opmap.opcode_of(OpKind::Return));
        assert_eq!(bytes[42], 0xFF);
        assert_eq!(bytes[43], 0xFF);
    }

    #[test]
    fn fuses_adjacent_loadconsts_under_favorable_seed() {
        // 3 LoadConsts then Return.
        // With seed [0u8;32], the first coin byte is 118 (even → fuse pair at i=0).
        // The third LC (i=2) has no eligible pair (next is Return), coin is drawn
        // anyway but fusion doesn't trigger.
        // Result: LCLC(9 bytes) + LC(5 bytes) + Return(3 bytes) after prologue.
        // Prologue = 5 + 36 = 41 bytes.
        // Total = 41 + 9 + 5 + 3 = 58 bytes.
        let f = LirFunction {
            id: ProtoId(0),
            num_params: 0,
            num_regs: 3,
            num_upvals: 0,
            is_vararg: false,
            consts: vec![
                luau_mir::Constant::Number(1.0),
                luau_mir::Constant::Number(2.0),
                luau_mir::Constant::Number(3.0),
            ],
            instrs: vec![
                LirInstr {
                    op: OpKind::LoadConst,
                    operands: vec![Operand::Reg(Reg(0)), Operand::Const(ConstIdx(0))],
                },
                LirInstr {
                    op: OpKind::LoadConst,
                    operands: vec![Operand::Reg(Reg(1)), Operand::Const(ConstIdx(1))],
                },
                LirInstr {
                    op: OpKind::LoadConst,
                    operands: vec![Operand::Reg(Reg(2)), Operand::Const(ConstIdx(2))],
                },
                LirInstr {
                    op: OpKind::Return,
                    operands: vec![Operand::Reg(Reg(0xFFFF))],
                },
            ],
            label_positions: vec![],
            closure_upval_sources: vec![],
            build_results_values: vec![],
        };
        let opmap = OpMap::new(&[0u8; 32]);
        let n_ops = ALL_OPS.len();
        let perm: Vec<u8> = (1..=n_ops as u8).collect();
        let inv = perm.clone();
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([0u8; 32]);
        let bytes = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv, &mut rng);
        // Prologue = 5 + 36 = 41 bytes (k0=0, k1=0, pid=0 → keystream all zeros).
        // LCLC: 1 + 8 = 9 bytes.
        // LC:   1 + 4 = 5 bytes.
        // Return: 1 + 2 = 3 bytes.
        assert_eq!(bytes.len(), 41 + 9 + 5 + 3, "expected fused LCLC + lone LC + Return");
        // Verify the opcode at prologue end is LCLC (identity perm: canonical = emitted).
        assert_eq!(bytes[41], opmap.opcode_of(OpKind::LoadConstLoadConst),
            "first instruction byte should be LCLC opcode");
    }
}
