//! Stage-0 self-decrypting bootstrap. Wraps a stage-1 Luau source string in
//! a small wrapper that decrypts and `loadstring`-evaluates it at runtime.
//!
//! After this wrapper is rendered and mangled, the obfuscated output's
//! top-level structure is unrecognizable — only the wrapper's locals,
//! a giant encrypted byte-string literal, and a `loadstring()` invocation
//! are visible. The dispatcher, helpers, opcode constants, constant pool,
//! and bytecode all live inside the encrypted payload.

use rand_chacha::ChaCha20Rng;

/// Encrypt the stage-1 source under a 32-byte key + position byte XOR.
pub fn encrypt_payload(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    for (i, b) in plaintext.iter().enumerate() {
        let k = key[i % 32];
        let pos = (i & 0xFF) as u8;
        out.push(b ^ k ^ pos);
    }
    out
}

/// Render the stage-0 bootstrap text given the encrypted payload + key.
/// Caller is responsible for applying identifier mangling + comment
/// stripping to the result if desired.
pub fn render_stage0(encrypted_payload: &[u8], key: &[u8; 32]) -> String {
    let payload_lit = crate::render::encode_luau_string_literal(encrypted_payload);
    let key_lit = crate::render::encode_luau_string_literal(key);
    format!(
        r#"local _s = "{payload}"
local _k = "{key}"
local function _d(s)
    local out = {{}}
    local kb = {{}}
    for i = 1, 32 do kb[i] = string.byte(_k, i) end
    for i = 1, #s do
        local b = string.byte(s, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1) % 256
        out[i] = string.char(bit32.bxor(bit32.bxor(b, kb[idx]), pos))
    end
    return table.concat(out)
end
return loadstring(_d(_s))(...)
"#,
        payload = payload_lit,
        key = key_lit,
    )
}

pub fn derive_stage0_key(rng: &mut ChaCha20Rng) -> [u8; 32] {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rng.fill_bytes(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn encrypt_round_trips() {
        let key = [42u8; 32];
        let plaintext = b"hello, world! print(\"foo\")";
        let enc = encrypt_payload(plaintext, &key);
        // Decrypt by re-applying.
        let dec: Vec<u8> = enc.iter().enumerate().map(|(i, b)| {
            let k = key[i % 32];
            let pos = (i & 0xFF) as u8;
            b ^ k ^ pos
        }).collect();
        assert_eq!(dec, plaintext);
    }

    #[test]
    fn render_stage0_contains_loadstring_and_payload() {
        let key = [1u8; 32];
        let payload = b"return 42";
        let text = render_stage0(payload, &key);
        assert!(text.contains("loadstring"));
        assert!(text.contains("local _s"));
        assert!(text.contains("local _k"));
    }

    #[test]
    fn deterministic_key_for_same_seed() {
        let mut r1 = ChaCha20Rng::from_seed([7u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([7u8; 32]);
        assert_eq!(derive_stage0_key(&mut r1), derive_stage0_key(&mut r2));
    }
}
