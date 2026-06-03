//! Renders the VM template against the LIR + opcode map to produce a single
//! Luau chunk.

use crate::encode::encode_function;
use crate::opmap::{OpMap, ALL_OPS};
use crate::EmitError;
use luau_lir::{LirProgram, OpKind};
use luau_mir::Constant;
use minijinja::Environment;
use rand_chacha::ChaCha20Rng;

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

    let consts: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| format_const_pool(&f.consts, &key_a, &key_b, i as u64))
        .collect();

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
) -> String {
    let parts: Vec<String> = consts
        .iter()
        .map(|c| format_const(c, key_a, key_b, proto_salt))
        .collect();
    parts.join(", ")
}

fn format_const(c: &Constant, key_a: &[u8; 32], key_b: &[u8; 32], proto_salt: u64) -> String {
    // Plan 11: every constant of every type is uniformly wrapped as
    // `_cw(tag, encrypted_bytes)`. Tags: 0=string, 1=number, 2=bool, 3=nil.
    let (tag, plaintext): (u8, Vec<u8>) = match c {
        Constant::Nil => (3, Vec::new()),
        Constant::Bool(true) => (2, vec![1]),
        Constant::Bool(false) => (2, vec![0]),
        Constant::Number(n) => (1, n.to_le_bytes().to_vec()),
        Constant::String(s) => (0, s.as_bytes().to_vec()),
    };
    let enc = encrypt_bytes(&plaintext, key_a, key_b, proto_salt);
    format!("_cw({}, \"{}\")", tag, encode_luau_string_literal(&enc))
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
    fn format_const_emits_cw_for_all_types() {
        let ka = [0u8; 32];
        let kb = [0u8; 32];
        let s_out = format_const(&Constant::String("hi".into()), &ka, &kb, 0);
        assert!(s_out.starts_with("_cw(0,"), "string: {s_out}");
        let n_out = format_const(&Constant::Number(3.14), &ka, &kb, 0);
        assert!(n_out.starts_with("_cw(1,"), "number: {n_out}");
        let b_out = format_const(&Constant::Bool(true), &ka, &kb, 0);
        assert!(b_out.starts_with("_cw(2,"), "bool: {b_out}");
        let nil_out = format_const(&Constant::Nil, &ka, &kb, 0);
        assert!(nil_out.starts_with("_cw(3,"), "nil: {nil_out}");
    }
}
