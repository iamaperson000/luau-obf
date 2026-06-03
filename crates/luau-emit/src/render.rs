//! Renders the VM template against the LIR + opcode map to produce a single
//! Luau chunk.

use crate::encode::encode_function;
use crate::opmap::{OpMap, ALL_OPS};
use crate::EmitError;
use luau_lir::{LirProgram, OpKind};
use luau_mir::Constant;
use minijinja::Environment;
use rand_chacha::ChaCha20Rng;

/// Plan 12: every encrypted constant is a uniform fixed-size blob. The type
/// tag, the original length, and the payload all live INSIDE the encrypted
/// blob. BLOB_SIZE is set to 64 because the corpus contains at least one
/// string longer than 29 bytes (`string_interp.luau` has a 34-byte string).
pub(crate) const BLOB_SIZE: usize = 64;
pub(crate) const MAX_STRING_LEN: usize = BLOB_SIZE - 3;

fn random_padding(rng: &mut ChaCha20Rng, len: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut out = vec![0u8; len];
    rng.fill_bytes(&mut out);
    out
}

fn build_constant_blob(
    c: &Constant,
    rng: &mut ChaCha20Rng,
) -> Result<Vec<u8>, EmitError> {
    let mut blob = vec![0u8; BLOB_SIZE];
    blob[0] = match c {
        Constant::String(_) => 0,
        Constant::Number(_) => 1,
        Constant::Bool(_) => 2,
        Constant::Nil => 3,
    };
    match c {
        Constant::String(s) => {
            let bytes = s.as_bytes();
            if bytes.len() > MAX_STRING_LEN {
                return Err(EmitError::Template(format!(
                    "string constant exceeds {}-byte max: {:?}",
                    MAX_STRING_LEN, s
                )));
            }
            blob[1] = (bytes.len() & 0xFF) as u8;
            blob[2] = ((bytes.len() >> 8) & 0xFF) as u8;
            blob[3..3 + bytes.len()].copy_from_slice(bytes);
            let pad = random_padding(rng, BLOB_SIZE - 3 - bytes.len());
            blob[3 + bytes.len()..].copy_from_slice(&pad);
        }
        Constant::Number(n) => {
            blob[1] = 8;
            blob[2] = 0;
            blob[3..11].copy_from_slice(&n.to_le_bytes());
            let pad = random_padding(rng, BLOB_SIZE - 11);
            blob[11..].copy_from_slice(&pad);
        }
        Constant::Bool(b) => {
            blob[1] = 1;
            blob[2] = 0;
            blob[3] = if *b { 1 } else { 0 };
            let pad = random_padding(rng, BLOB_SIZE - 4);
            blob[4..].copy_from_slice(&pad);
        }
        Constant::Nil => {
            blob[1] = 0;
            blob[2] = 0;
            let pad = random_padding(rng, BLOB_SIZE - 3);
            blob[3..].copy_from_slice(&pad);
        }
    }
    Ok(blob)
}

pub fn render(
    program: &LirProgram,
    opmap: &OpMap,
    rng: &mut ChaCha20Rng,
    binding: Option<&crate::stage0::EmitEnvBinding>,
) -> Result<String, EmitError> {
    // Plan 14: derive stage-0 key BEFORE any other rng consumption that
    // affects stage-1. Same seed -> same stage-0 key.
    let stage0_key = crate::stage0::derive_stage0_key(rng);

    let stage1_source = {
        let opcodes: Vec<(String, u8)> = ALL_OPS
            .iter()
            .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
            .collect();

        let ops: std::collections::HashMap<String, u8> = ALL_OPS
            .iter()
            .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
            .collect();

        let per_proto_keys = derive_per_proto_string_keys(rng, program.functions.len());

        use rand::RngCore;
        let mut k_buf = [0u8; 8];
        rng.fill_bytes(&mut k_buf);
        let k0 = u32::from_le_bytes([k_buf[0], k_buf[1], k_buf[2], k_buf[3]]);
        let k1 = u32::from_le_bytes([k_buf[4], k_buf[5], k_buf[6], k_buf[7]]) | 1;
        // k1 forced odd so the keystream's `pc * k1` term varies with pc on a byte basis.

        // Plan 13: generate a per-proto opcode permutation drawn from the shared
        // rng. The permutation must come before encode_function consumes the rng
        // (currently it doesn't, but we keep the ordering explicit) so that
        // determinism holds across builds with the same seed.
        let n_ops = crate::opmap::ALL_OPS.len();
        let perms_and_invs: Vec<(Vec<u8>, Vec<u8>)> = (0..program.functions.len())
            .map(|_| crate::encode::make_opcode_permutation(n_ops, rng))
            .collect();

        // Plan 30 / Plan 32 Task 2+3: encode_function returns (bytes, bs_final, crc).
        // bs_final is the LCG state after consuming all encrypted bytecode bytes.
        // crc is the CRC32 of the encrypted bytecode (Plan 32 Task 3: stored as
        // constant slot 0 so the VM can verify bytecode integrity at entry).
        let mut codes: Vec<String> = Vec::with_capacity(program.functions.len());
        let mut bs_finals: Vec<u32> = Vec::with_capacity(program.functions.len());
        let mut crcs: Vec<u32> = Vec::with_capacity(program.functions.len());
        for (i, f) in program.functions.iter().enumerate() {
            let (perm, inv) = &perms_and_invs[i];
            let (bytes, bs_final, crc) = encode_function(f, opmap, i as u32, k0, k1, perm, inv, rng);
            codes.push(format!("\"{}\"", encode_luau_string_literal(&bytes)));
            bs_finals.push(bs_final);
            crcs.push(crc);
        }

        // Now build constant pools, seeded by each proto's bs_final.
        // Plan 32 Task 3: prepend a CRC32 constant (slot 0) to every proto's pool.
        // LIR-level Operand::Const(c) references are emitted as c.0+1 by the
        // encoder, so slot 0 is invisible to user code.
        let mut consts: Vec<String> = Vec::with_capacity(program.functions.len());
        for (i, f) in program.functions.iter().enumerate() {
            let (ka, kb) = &per_proto_keys[i];
            // Prepend the CRC constant at slot 0.
            let crc_const = Constant::Number(crcs[i] as f64);
            let mut full_consts = vec![crc_const];
            full_consts.extend_from_slice(&f.consts);
            consts.push(format_const_pool(&full_consts, ka, kb, i as u64, bs_finals[i], rng)?);
        }

        let keys_a_lits: Vec<String> = per_proto_keys.iter().map(|(a, _)| format_byte_array_literal(a)).collect();
        let keys_b_lits: Vec<String> = per_proto_keys.iter().map(|(_, b)| format_byte_array_literal(b)).collect();

        let mut env = Environment::new();
        env.add_template("vm", luau_runtime::VM_TEMPLATE)
            .map_err(|e| EmitError::Template(e.to_string()))?;
        let tmpl = env.get_template("vm").unwrap();
        let raw_output = tmpl.render(minijinja::context! {
            opcodes => opcodes,
            ops => ops,
            consts => consts,
            codes => codes,
            keys_a => keys_a_lits,
            keys_b => keys_b_lits,
            k0 => k0,
            k1 => k1,
        })
        .map_err(|e| EmitError::Template(e.to_string()))?;

        let stripped = crate::mangle::strip_comments(&raw_output);
        let name_map = crate::mangle::build_name_map(rng);
        crate::mangle::mangle_identifiers(&stripped, &name_map)
    };

    // Plan 14 + Plan 32 Task 4: encrypt the stage-1 source and wrap it in a
    // stage-0 bootstrap.  The effective RC4 key is derived as:
    //   key = fnv_fingerprint(base) XOR base [XOR bind_fold if binding present]
    // where `base` is the literal `_kbase` embedded in the wrapper.
    // Hashing `base` (not the ciphertext) avoids the chicken-and-egg where
    // key depends on ciphertext depends on key.
    let stage0_text = if let Some(b) = binding {
        let runtime_key = crate::stage0::derive_runtime_key(
            &stage0_key,
            Some(b.expected_value.as_str()),
        );
        let encrypted = crate::stage0::encrypt_payload(stage1_source.as_bytes(), &runtime_key);
        crate::stage0::render_stage0(&encrypted, &stage0_key, Some(b))
    } else {
        let runtime_key = crate::stage0::derive_runtime_key(&stage0_key, None);
        let encrypted = crate::stage0::encrypt_payload(stage1_source.as_bytes(), &runtime_key);
        crate::stage0::render_stage0(&encrypted, &stage0_key, None)
    };

    let stage0_stripped = crate::mangle::strip_comments(&stage0_text);
    let stage0_map = crate::mangle::build_stage0_name_map(rng);
    let stage0_mangled = crate::mangle::mangle_identifiers(&stage0_stripped, &stage0_map);

    Ok(stage0_mangled)
}

fn opname(k: OpKind) -> &'static str {
    match k {
        OpKind::LoadNil => "LoadNil",
        OpKind::LoadTrue => "LoadTrue",
        OpKind::LoadFalse => "LoadFalse",
        OpKind::LoadConst => "LoadConst",
        OpKind::Move => "Move",
        OpKind::Add => "Add", OpKind::Sub => "Sub", OpKind::Mul => "Mul",
        OpKind::Div => "Div", OpKind::Mod => "Mod", OpKind::Pow => "Pow",
        OpKind::Concat => "Concat",
        OpKind::Lt => "Lt", OpKind::Le => "Le", OpKind::Eq => "Eq",
        OpKind::Not => "Not", OpKind::Neg => "Neg", OpKind::Len => "Len",
        OpKind::GetGlobal => "GetGlobal", OpKind::SetGlobal => "SetGlobal",
        OpKind::Call => "Call", OpKind::Return => "Return",
        OpKind::Jmp => "Jmp",
        OpKind::JmpIfTrue => "JmpIfTrue", OpKind::JmpIfFalse => "JmpIfFalse",
        OpKind::Closure => "Closure",
        OpKind::NewTable => "NewTable",
        OpKind::GetTable => "GetTable",
        OpKind::SetTable => "SetTable",
        OpKind::GetUpval => "GetUpval",
        OpKind::SetUpval => "SetUpval",
        OpKind::CallVar => "CallVar",
        OpKind::BuildResults => "BuildResults",
        OpKind::Vararg => "Vararg",
        OpKind::ReturnMulti => "ReturnMulti",
        // Plan 30: encode-time superop — never in user MIR, only from fusion.
        OpKind::LoadConstLoadConst => "LoadConstLoadConst",
    }
}

fn format_const_pool(
    consts: &[Constant],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    bs_seed: u32,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    let mut parts = Vec::with_capacity(consts.len());
    for c in consts {
        parts.push(format_const(c, key_a, key_b, proto_salt, bs_seed, rng)?);
    }
    Ok(parts.join(", "))
}

fn format_const(
    c: &Constant,
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    bs_seed: u32,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    // Plan 12: every constant is built into a uniform BLOB_SIZE-byte blob
    // containing the tag, length, payload, and random padding. The blob is
    // then XOR-encrypted. The wrapper call site is `_cw("<bytes>")` — a
    // single argument, identical-looking regardless of constant type.
    let blob = build_constant_blob(c, rng)?;
    let enc = encrypt_bytes(&blob, key_a, key_b, proto_salt, bs_seed);
    Ok(format!("_cw(\"{}\")", encode_luau_string_literal(&enc)))
}

pub(crate) fn derive_per_proto_string_keys(
    rng: &mut ChaCha20Rng,
    n_protos: usize,
) -> Vec<([u8; 32], [u8; 32])> {
    use rand::RngCore;
    let mut out = Vec::with_capacity(n_protos);
    for _ in 0..n_protos {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        rng.fill_bytes(&mut a);
        rng.fill_bytes(&mut b);
        out.push((a, b));
    }
    out
}

/// Encrypt a constant-pool blob.
///
/// `bs_seed` is the byte-cipher's final LCG state after pre-decrypting the
/// proto's bytecode (Plan 32 Task 2).  Mixing it into the constant cipher
/// means that static decryption of constants requires simulating the LCG
/// through the entire encrypted bytecode first.
///
/// The `mix` advance step uses the SAME LCG constants (1_103_515_245, 12345)
/// as the byte-cipher so the Luau side can reuse the same formula.
pub(crate) fn encrypt_bytes(
    plaintext: &[u8],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    bs_seed: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    let mut mix: u32 = bs_seed;
    for (i, b) in plaintext.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = ((i as u64)
            .wrapping_add(proto_salt.wrapping_mul(7919))
            .wrapping_add((mix & 0xFF) as u64)
            & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
        // Advance using plaintext byte so that Luau decryption side advances
        // with the just-decrypted byte (symmetric).
        mix = mix
            .wrapping_mul(1_103_515_245)
            .wrapping_add(*b as u32)
            .wrapping_add(12345);
    }
    out
}

pub(crate) fn format_byte_array_literal(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    parts.join(", ")
}

pub(crate) fn encode_luau_string_literal(bytes: &[u8]) -> String {
    // Use zero-padded 3-digit decimal escapes (\NNN) for every byte that is not
    // safe printable ASCII.  Three digits is Luau's maximum for numeric string
    // escapes, so the next character can *never* be consumed as part of this
    // escape.  This avoids the subtle bug where the unpadded form (e.g. `\14`)
    // followed by a printable digit (e.g. `0`) would be mis-parsed by Luau as a
    // single 3-digit escape `\140` (byte 140) instead of byte 14 + character '0'.
    let mut out = String::with_capacity(bytes.len() * 4);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"'  => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            // Safe printable ASCII (not a special character above).
            // These are never confused with a preceding \NNN escape because
            // \NNN is 3 digits — the maximum — leaving no room for extension.
            0x20..=0x7E => out.push(b as char),
            // All other bytes: zero-padded 3-digit decimal.
            _ => out.push_str(&format!("\\{:03}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trips() {
        let key_a = [42u8; 32];
        let key_b = [99u8; 32];
        let plaintext = b"print";
        let proto_salt = 3u64;
        let bs_seed: u32 = 0xABCD1234;
        let enc = encrypt_bytes(plaintext, &key_a, &key_b, proto_salt, bs_seed);
        // Decrypt by re-running the same mix sequence.
        let mut dec = Vec::with_capacity(plaintext.len());
        let mut mix: u32 = bs_seed;
        for (i, b) in enc.iter().enumerate() {
            let ka = key_a[i % 32];
            let kb = key_b[i % 32];
            let pos = ((i as u64)
                .wrapping_add(proto_salt.wrapping_mul(7919))
                .wrapping_add((mix & 0xFF) as u64)
                & 0xFF) as u8;
            let plain = b ^ ka ^ pos ^ kb;
            dec.push(plain);
            mix = mix
                .wrapping_mul(1_103_515_245)
                .wrapping_add(plain as u32)
                .wrapping_add(12345);
        }
        assert_eq!(dec, plaintext);
    }

    #[test]
    fn encrypt_changes_bytes_for_typical_input() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        let enc = encrypt_bytes(b"hello world", &key_a, &key_b, 0, 0);
        assert_ne!(enc, b"hello world");
    }

    #[test]
    fn empty_string_encrypts_to_empty() {
        let enc = encrypt_bytes(b"", &[0u8; 32], &[0u8; 32], 0, 0);
        assert!(enc.is_empty());
    }

    #[test]
    fn different_proto_salts_change_ciphertext() {
        let key_a = [3u8; 32];
        let key_b = [4u8; 32];
        let e1 = encrypt_bytes(b"deposit", &key_a, &key_b, 1, 0);
        let e2 = encrypt_bytes(b"deposit", &key_a, &key_b, 2, 0);
        assert_ne!(e1, e2);
    }

    #[test]
    fn different_bs_seeds_change_ciphertext() {
        let key_a = [5u8; 32];
        let key_b = [6u8; 32];
        let e1 = encrypt_bytes(b"constant", &key_a, &key_b, 1, 0xDEADBEEF);
        let e2 = encrypt_bytes(b"constant", &key_a, &key_b, 1, 0xCAFEBABE);
        assert_ne!(e1, e2, "different bs_seed must produce different ciphertext");
    }

    #[test]
    fn format_const_emits_one_arg_cw_for_all_types() {
        use rand::SeedableRng;
        let ka = [0u8; 32];
        let kb = [0u8; 32];
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        for c in &[
            Constant::String("hi".into()),
            Constant::Number(3.14),
            Constant::Bool(true),
            Constant::Nil,
        ] {
            let out = format_const(c, &ka, &kb, 0, 0, &mut rng).unwrap();
            assert!(out.starts_with("_cw(\""), "expected single-arg form: {out}");
            assert!(out.ends_with("\")"), "expected single-arg form: {out}");
        }
    }

    #[test]
    fn build_blob_encodes_string_length() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([1u8; 32]);
        let blob = build_constant_blob(&Constant::String("hello".into()), &mut rng).unwrap();
        assert_eq!(blob.len(), BLOB_SIZE);
        assert_eq!(blob[0], 0); // tag = string
        assert_eq!(blob[1] as usize + blob[2] as usize * 256, 5); // length
        assert_eq!(&blob[3..8], b"hello");
    }

    #[test]
    fn build_blob_encodes_number() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([1u8; 32]);
        let blob = build_constant_blob(&Constant::Number(2.5), &mut rng).unwrap();
        assert_eq!(blob.len(), BLOB_SIZE);
        assert_eq!(blob[0], 1);
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&blob[3..11]);
        assert_eq!(f64::from_le_bytes(buf), 2.5);
    }

    #[test]
    fn build_blob_encodes_bool() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([2u8; 32]);
        let blob_true = build_constant_blob(&Constant::Bool(true), &mut rng).unwrap();
        assert_eq!(blob_true.len(), BLOB_SIZE);
        assert_eq!(blob_true[0], 2);
        assert_eq!(blob_true[3], 1);
        let blob_false = build_constant_blob(&Constant::Bool(false), &mut rng).unwrap();
        assert_eq!(blob_false[0], 2);
        assert_eq!(blob_false[3], 0);
    }

    #[test]
    fn build_blob_encodes_nil() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([3u8; 32]);
        let blob = build_constant_blob(&Constant::Nil, &mut rng).unwrap();
        assert_eq!(blob.len(), BLOB_SIZE);
        assert_eq!(blob[0], 3);
    }

    #[test]
    fn rejects_string_too_long() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([1u8; 32]);
        let huge: String = "a".repeat(MAX_STRING_LEN + 1);
        let r = build_constant_blob(&Constant::String(huge), &mut rng);
        assert!(r.is_err());
    }

    #[test]
    fn per_proto_keys_distinct() {
        use rand::SeedableRng;
        let mut rng = ChaCha20Rng::from_seed([7u8; 32]);
        let keys = derive_per_proto_string_keys(&mut rng, 4);
        assert_eq!(keys.len(), 4);
        for i in 0..keys.len() {
            for j in (i+1)..keys.len() {
                assert_ne!(keys[i].0, keys[j].0, "key_a {} == key_a {}", i, j);
                assert_ne!(keys[i].1, keys[j].1, "key_b {} == key_b {}", i, j);
            }
        }
    }

    #[test]
    fn per_proto_keys_deterministic() {
        use rand::SeedableRng;
        let mut r1 = ChaCha20Rng::from_seed([9u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([9u8; 32]);
        let k1 = derive_per_proto_string_keys(&mut r1, 3);
        let k2 = derive_per_proto_string_keys(&mut r2, 3);
        assert_eq!(k1, k2);
    }

    /// Plan 32 Task 2 cross-impl test: Rust encrypt_bytes with a given bs_seed
    /// must be decryptable by the Luau `_decrypt` function using the same bs_seed.
    /// This validates that both sides implement the tangled constant cipher
    /// identically.
    #[test]
    fn rust_luau_tangled_const_cipher_agree() {
        let key_a = [37u8; 32];
        let key_b = [99u8; 32];
        let proto_id: u32 = 0; // use proto_id 0 → proto_salt = 0 → _KAS[1]/_KBS[1]
        let proto_salt: u64 = 0;
        let bs_seed: u32 = 0xCAFEBEEF;
        let plaintext = b"hello tangled constants";

        let enc = encrypt_bytes(plaintext, &key_a, &key_b, proto_salt, bs_seed);

        // Build Luau source that decrypts using the same algorithm.
        let ka_lit: String = key_a.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
        let kb_lit: String = key_b.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
        let enc_lit = encode_luau_string_literal(&enc);

        let luau_src = format!(
            r#"local _xor = bit32.bxor
local _sbyte = string.byte
local _schar = string.char
local _tconcat = table.concat
local _KAS = {{ {{ {ka} }} }}
local _KBS = {{ {{ {kb} }} }}
local enc = "{enc_lit}"
local proto_id = {pid}
local bs_seed = {bs}
local function _mul32(a, b)
    local a_lo = a % 65536
    local a_hi = (a - a_lo) / 65536
    local b_lo = b % 65536
    local b_hi = (b - b_lo) / 65536
    local mid = (a_lo * b_hi + a_hi * b_lo) % 65536
    return (a_lo * b_lo + mid * 65536) % 4294967296
end
local function _decrypt(e, pid, seed)
    local ka = _KAS[pid + 1]
    local kb = _KBS[pid + 1]
    local out = {{}}
    local salt_term = (pid * 7919) % 4294967296
    local mix = seed
    for i = 1, #e do
        local b = _sbyte(e, i)
        local idx = ((i - 1) % 32) + 1
        local pos = ((i - 1) + salt_term + (mix % 256)) % 256
        local plain = _xor(_xor(_xor(b, ka[idx]), pos), kb[idx])
        out[i] = _schar(plain)
        mix = (_mul32(mix, 1103515245) + plain + 12345) % 4294967296
    end
    return _tconcat(out)
end
print(_decrypt(enc, proto_id, bs_seed))
"#,
            ka = ka_lit,
            kb = kb_lit,
            enc_lit = enc_lit,
            pid = proto_id,
            bs = bs_seed,
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tangled_const_test.luau");
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
        let luau_result = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let expected = String::from_utf8(plaintext.to_vec()).unwrap();
        assert_eq!(
            luau_result, expected,
            "Luau tangled-const decryption did not recover plaintext"
        );
    }
}
