use crate::opmap::OpMap;
use luau_lir::{BlockLabel, LirFunction, LirInstr, OpKind, Operand, UpvalSource};
use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashMap;

/// Standard CRC32 with reversed polynomial 0xEDB88320.
/// Initial value 0xFFFFFFFF, final XOR 0xFFFFFFFF.
/// Computed over the raw (encrypted) bytecode bytes.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}

/// Initialize the stateful LCG keystream `bs` for a given proto.
/// bs_init(pid) = (pid + 1) * 2654435761 + k0  (mod 2^32)
#[inline]
pub(crate) fn lcg_init(proto_id: u32, k0: u32) -> u32 {
    proto_id.wrapping_add(1).wrapping_mul(2_654_435_761).wrapping_add(k0)
}

/// Advance the stateful LCG keystream: compute ks for this byte, then step bs.
/// Returns (ks, new_bs).
/// `pc` is 1-based; `bs` is the current keystream state.
#[inline]
pub(crate) fn lcg_step(bs: u32, pc: u32, proto_id: u32, k0: u32, k1: u32) -> (u8, u32) {
    let pos_mix = pc
        .wrapping_mul(k1)
        .wrapping_add(k0)
        .wrapping_add(proto_id.wrapping_mul(2_654_435_761));
    let ks = ((bs & 0xFF) ^ (pos_mix & 0xFF)) as u8;
    let new_bs = bs
        .wrapping_mul(1_103_515_245)
        .wrapping_add(ks as u32)
        .wrapping_add(12345);
    (ks, new_bs)
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

/// Encode and encrypt a LIR function's bytecode.
///
/// Returns `(encrypted_bytes, bs_final, crc)` where:
/// - `bs_final` is the LCG state after consuming every encrypted byte
///   (Plan 32 Task 2: used to tangle the constant-pool cipher).
/// - `crc` is the CRC32 of the encrypted bytecode bytes (Plan 32 Task 3:
///   stored as constant slot 0; checked at vm_call entry for integrity).
///
/// Constant operands (`Operand::Const(c)`) are emitted as `c.0 + 1` so that
/// slot 0 in every proto's constant pool is reserved for the CRC32 value.
pub fn encode_function(
    f: &LirFunction,
    opmap: &OpMap,
    proto_id: u32,
    k0: u32,
    k1: u32,
    perm: &[u8],
    inv: &[u8],
    rng: &mut rand_chacha::ChaCha20Rng,
) -> (Vec<u8>, u32, u32) {
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
                // +1: slot 0 in every proto's constant pool is reserved for the
                // CRC32 integrity value (Plan 32 Task 3). LIR const indices are
                // 0-based; the Luau runtime table is 1-based with CRC at [1],
                // so user const N ends up at consts[N+2] (handler adds +1 more).
                Operand::Const(c) => push_u16(&mut out, c.0 + 1),
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
    // XOR every byte by its stateful LCG keystream (1-based pc).
    // The LCG state `bs` makes each byte depend on all prior decoded bytes,
    // defeating static byte-by-byte decoding.
    let mut bs = lcg_init(proto_id, k0);
    for (idx, byte) in out.iter_mut().enumerate() {
        let pc = (idx + 1) as u32;
        let (ks, new_bs) = lcg_step(bs, pc, proto_id, k0, k1);
        *byte ^= ks;
        bs = new_bs;
    }
    // CRC32 of the encrypted bytecode (Plan 32 Task 3). Stored as constant slot 0.
    // Computed AFTER XOR encryption so tampering with `out` changes the CRC.
    let crc = crc32(&out);
    // Return encrypted bytes, final LCG state, and CRC.
    (out, bs, crc)
}

/// Push a single operand as a u16. Used for LCLC superop operands which are
/// always Reg or Const (never JmpTarget, so no relocation is needed).
fn push_operand_simple(out: &mut Vec<u8>, op: &Operand) {
    let v: u16 = match op {
        Operand::Reg(r) => r.0,
        // +1: same CRC slot-0 reservation as the main operand loop (Task 3).
        Operand::Const(c) => c.0 + 1,
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
    use crate::render::encode_luau_string_literal;
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
        let (bytes, _bs_final, _crc) = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv, &mut rng);
        // 5-byte META + 36-byte inv + 1 opcode byte + 2 operand bytes = 44.
        assert_eq!(bytes.len(), 44);
        // With the stateful LCG, bytes are encrypted even with k0=k1=pid=0.
        // Verify by decrypting them back and checking the plaintext.
        let k0: u32 = 0;
        let k1: u32 = 0;
        let proto_id: u32 = 0;
        let mut bs = lcg_init(proto_id, k0);
        let mut plain = bytes.clone();
        for (idx, byte) in plain.iter_mut().enumerate() {
            let pc = (idx + 1) as u32;
            let (ks, new_bs) = lcg_step(bs, pc, proto_id, k0, k1);
            *byte ^= ks;
            bs = new_bs;
        }
        assert_eq!(plain[0], 0);       // num_params
        assert_eq!(plain[1], 0);       // num_regs lo
        assert_eq!(plain[2], 0);       // num_regs hi
        assert_eq!(plain[3], 0);       // num_upvals
        assert_eq!(plain[4], 0);       // is_vararg
        // plain[5..41] = identity inv table = 1, 2, ..., 36.
        for i in 0..36 {
            assert_eq!(plain[5 + i], (i + 1) as u8);
        }
        assert_eq!(plain[41], opmap.opcode_of(OpKind::Return));
        assert_eq!(plain[42], 0xFF);
        assert_eq!(plain[43], 0xFF);
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
        let (bytes, _bs_final, _crc) = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv, &mut rng);
        // Prologue = 5 + 36 = 41 bytes.
        // LCLC: 1 + 8 = 9 bytes.
        // LC:   1 + 4 = 5 bytes.
        // Return: 1 + 2 = 3 bytes.
        assert_eq!(bytes.len(), 41 + 9 + 5 + 3, "expected fused LCLC + lone LC + Return");
        // Verify by decrypting bytes[41] back to check it's LCLC opcode.
        let k0: u32 = 0;
        let k1: u32 = 0;
        let proto_id: u32 = 0;
        let mut bs = lcg_init(proto_id, k0);
        let mut plain = bytes.clone();
        for (idx, byte) in plain.iter_mut().enumerate() {
            let pc = (idx + 1) as u32;
            let (ks, new_bs) = lcg_step(bs, pc, proto_id, k0, k1);
            *byte ^= ks;
            bs = new_bs;
        }
        assert_eq!(plain[41], opmap.opcode_of(OpKind::LoadConstLoadConst),
            "first instruction byte should be LCLC opcode");
    }

    /// Cross-impl check: Rust LCG encrypt then Luau LCG decrypt must recover plaintext.
    /// This is the gate test for Plan 32 Task 1.
    #[test]
    fn rust_luau_stateful_keystream_agree() {
        let plaintext: Vec<u8> = (0..100u8).collect();
        let k0: u32 = 0xDEADBEEF;
        let k1: u32 = 0x12345679; // odd
        let proto_id: u32 = 7;

        // Encrypt in Rust using the stateful LCG.
        let mut enc = plaintext.clone();
        let mut bs = lcg_init(proto_id, k0);
        for (i, byte) in enc.iter_mut().enumerate() {
            let pc = (i + 1) as u32;
            let (ks, new_bs) = lcg_step(bs, pc, proto_id, k0, k1);
            *byte ^= ks;
            bs = new_bs;
        }

        // Build Luau source that decrypts using the same LCG and prints hex.
        // The luau CLI does not expose `io`, so we use print() and compare hex.
        // _mul32 avoids IEEE-754 double precision loss for u32 multiplications.
        let enc_lit = encode_luau_string_literal(&enc);
        let luau_src = format!(
            r#"local _k0 = {k0}
local _k1 = {k1}
local _xor = bit32.bxor
local _sbyte = string.byte
local _enc = "{enc_lit}"
local proto_id = {proto_id}
local function _mul32(a, b)
    local a_lo = a % 65536
    local a_hi = (a - a_lo) / 65536
    local b_lo = b % 65536
    local b_hi = (b - b_lo) / 65536
    local mid = (a_lo * b_hi + a_hi * b_lo) % 65536
    return (a_lo * b_lo + mid * 65536) % 4294967296
end
local _bs = (_mul32(proto_id + 1, 2654435761) + _k0) % 4294967296
local function _byte(code, pc, pid)
    local raw = _sbyte(code, pc)
    local pos_mix = (_mul32(pc, _k1) + _k0 + _mul32(pid, 2654435761)) % 4294967296
    local ks = _xor(_bs % 256, pos_mix % 256)
    _bs = (_mul32(_bs, 1103515245) + ks + 12345) % 4294967296
    return _xor(raw, ks)
end
local out = {{}}
for i = 1, #_enc do
    out[i] = _byte(_enc, i, proto_id)
end
local hex = {{}}
for i = 1, #_enc do
    hex[i] = string.format("%02x", out[i])
end
print(table.concat(hex))
"#,
            k0 = k0,
            k1 = k1,
            proto_id = proto_id,
            enc_lit = enc_lit,
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.luau");
        std::fs::write(&path, &luau_src).unwrap();
        let out = std::process::Command::new("luau")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "luau failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Compare hex strings (print adds a trailing newline which we trim).
        let luau_hex = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let expected_hex: String = plaintext.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            luau_hex, expected_hex,
            "Luau decryption did not recover plaintext — encoder/decoder out of sync"
        );
    }

    // ── Plan 32 Task 3: CRC32 unit tests ─────────────────────────────────────

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"a"), 0xE8B7BE43);
        assert_eq!(crc32(b"abc"), 0x352441C2);
        assert_eq!(crc32(b"hello world"), 0x0D4A1185);
    }

    #[test]
    fn rust_luau_crc32_agree() {
        let inputs: Vec<&[u8]> = vec![b"", b"a", b"abc", b"hello world", b"\x00\x01\x02\xff\xfe"];
        for input in inputs {
            let rust_crc = crc32(input);
            let input_lit = encode_luau_string_literal(input);
            let luau = format!(
                r#"local _xor = bit32.bxor
local _sbyte = string.byte
local _floor = math.floor
local _crc_table = {{}}
do
    for i = 0, 255 do
        local c = i
        for _ = 1, 8 do
            if c % 2 == 1 then c = _xor(_floor(c / 2), 3988292384)
            else c = _floor(c / 2) end
        end
        _crc_table[i] = c
    end
end
local function _crc32(s)
    local crc = 4294967295
    for i = 1, #s do
        crc = _xor(_floor(crc / 256), _crc_table[_xor(crc % 256, _sbyte(s, i))])
    end
    return _xor(crc, 4294967295)
end
print(_crc32("{input_lit}"))
"#,
                input_lit = input_lit,
            );
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("t.luau");
            std::fs::write(&path, luau).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(), "luau failed: {}", String::from_utf8_lossy(&out.stderr));
            let luau_crc: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
            assert_eq!(rust_crc, luau_crc,
                "input {:?}: rust=0x{:x} luau=0x{:x}", input, rust_crc, luau_crc);
        }
    }
}
