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
) -> Result<String, EmitError> {
    let opcodes: Vec<(String, u8)> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();

    let ops: std::collections::HashMap<String, u8> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();

    let (key_a, key_b) = derive_string_keys(rng);

    let mut consts: Vec<String> = Vec::with_capacity(program.functions.len());
    for (i, f) in program.functions.iter().enumerate() {
        consts.push(format_const_pool(&f.consts, &key_a, &key_b, i as u64, rng)?);
    }

    use rand::RngCore;
    let mut k_buf = [0u8; 8];
    rng.fill_bytes(&mut k_buf);
    let k0 = u32::from_le_bytes([k_buf[0], k_buf[1], k_buf[2], k_buf[3]]);
    let k1 = u32::from_le_bytes([k_buf[4], k_buf[5], k_buf[6], k_buf[7]]) | 1;
    // k1 forced odd so the keystream's `pc * k1` term varies with pc on a byte basis.

    let codes: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let bytes = encode_function(f, opmap, i as u32, k0, k1);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();

    let key_a_lit = format_byte_array_literal(&key_a);
    let key_b_lit = format_byte_array_literal(&key_b);

    let mut env = Environment::new();
    env.add_template("vm", luau_runtime::VM_TEMPLATE)
        .map_err(|e| EmitError::Template(e.to_string()))?;
    let tmpl = env.get_template("vm").unwrap();
    let output = tmpl.render(minijinja::context! {
        opcodes => opcodes,
        ops => ops,
        consts => consts,
        codes => codes,
        key_a => key_a_lit,
        key_b => key_b_lit,
        k0 => k0,
        k1 => k1,
    })
    .map_err(|e| EmitError::Template(e.to_string()))?;

    let stripped = crate::mangle::strip_comments(&output);
    let name_map = crate::mangle::build_name_map(rng);
    let mangled = crate::mangle::mangle_identifiers(&stripped, &name_map);
    Ok(mangled)
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
    }
}

fn format_const_pool(
    consts: &[Constant],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    let mut parts = Vec::with_capacity(consts.len());
    for c in consts {
        parts.push(format_const(c, key_a, key_b, proto_salt, rng)?);
    }
    Ok(parts.join(", "))
}

fn format_const(
    c: &Constant,
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    // Plan 12: every constant is built into a uniform BLOB_SIZE-byte blob
    // containing the tag, length, payload, and random padding. The blob is
    // then XOR-encrypted. The wrapper call site is `_cw("<bytes>")` — a
    // single argument, identical-looking regardless of constant type.
    let blob = build_constant_blob(c, rng)?;
    let enc = encrypt_bytes(&blob, key_a, key_b, proto_salt);
    Ok(format!("_cw(\"{}\")", encode_luau_string_literal(&enc)))
}

pub(crate) fn derive_string_keys(rng: &mut ChaCha20Rng) -> ([u8; 32], [u8; 32]) {
    use rand::RngCore;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    rng.fill_bytes(&mut a);
    rng.fill_bytes(&mut b);
    (a, b)
}

pub(crate) fn encrypt_bytes(
    plaintext: &[u8],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    for (i, b) in plaintext.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}

pub(crate) fn format_byte_array_literal(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    parts.join(", ")
}

fn encode_luau_string_literal(bytes: &[u8]) -> String {
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
        let enc = encrypt_bytes(plaintext, &key_a, &key_b, proto_salt);
        let dec: Vec<u8> = enc.iter().enumerate().map(|(i, b)| {
            let ka = key_a[i % 32];
            let kb = key_b[i % 32];
            let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
            b ^ ka ^ pos ^ kb
        }).collect();
        assert_eq!(dec, plaintext);
    }

    #[test]
    fn encrypt_changes_bytes_for_typical_input() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        let enc = encrypt_bytes(b"hello world", &key_a, &key_b, 0);
        assert_ne!(enc, b"hello world");
    }

    #[test]
    fn empty_string_encrypts_to_empty() {
        let enc = encrypt_bytes(b"", &[0u8; 32], &[0u8; 32], 0);
        assert!(enc.is_empty());
    }

    #[test]
    fn different_proto_salts_change_ciphertext() {
        let key_a = [3u8; 32];
        let key_b = [4u8; 32];
        let e1 = encrypt_bytes(b"deposit", &key_a, &key_b, 1);
        let e2 = encrypt_bytes(b"deposit", &key_a, &key_b, 2);
        assert_ne!(e1, e2);
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
            let out = format_const(c, &ka, &kb, 0, &mut rng).unwrap();
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
}
