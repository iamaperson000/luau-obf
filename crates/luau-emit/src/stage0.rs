//! Stage-0 self-decrypting bootstrap. Wraps a stage-1 Luau source string in
//! a small wrapper that decrypts and `loadstring`-evaluates it at runtime.
//!
//! After this wrapper is rendered and mangled, the obfuscated output's
//! top-level structure is unrecognizable — only the wrapper's locals,
//! a giant encrypted byte-string literal, and a `loadstring()` invocation
//! are visible. The dispatcher, helpers, opcode constants, constant pool,
//! and bytecode all live inside the encrypted payload.

use rand_chacha::ChaCha20Rng;

/// The Luau _d(s) function that decrypts RC4 payloads.
/// Used by both render_stage0 and the rc4_rust_luau_agree test.
const STAGE0_DECRYPT_LUAU: &str = r#"local function _d(s)
    local S = {}
    for i = 0, 255 do S[i] = i end
    local kb = {}
    for i = 1, 32 do kb[i] = string.byte(_k, i) end
    local j = 0
    for i = 0, 255 do
        j = (j + S[i] + kb[(i % 32) + 1]) % 256
        S[i], S[j] = S[j], S[i]
    end
    local ii = 0
    local jj = 0
    for _ = 1, 256 do
        ii = (ii + 1) % 256
        jj = (jj + S[ii]) % 256
        S[ii], S[jj] = S[jj], S[ii]
    end
    local out = {}
    for n = 1, #s do
        ii = (ii + 1) % 256
        jj = (jj + S[ii]) % 256
        S[ii], S[jj] = S[jj], S[ii]
        local k = S[(S[ii] + S[jj]) % 256]
        out[n] = string.char(bit32.bxor(string.byte(s, n), k))
    end
    return table.concat(out)
end"#;

/// Encrypt (or decrypt — symmetric) the stage-1 source using RC4 with a
/// 256-byte keystream drop to defeat the positional-bucketing attack.
pub fn encrypt_payload(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    rc4_encrypt(plaintext, key)
}

fn rc4_encrypt(plaintext: &[u8], key: &[u8]) -> Vec<u8> {
    // KSA
    let mut s = [0u8; 256];
    for (i, slot) in s.iter_mut().enumerate() { *slot = i as u8; }
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    // Drop-256
    let mut ii: u8 = 0;
    let mut jj: u8 = 0;
    for _ in 0..256 {
        ii = ii.wrapping_add(1);
        jj = jj.wrapping_add(s[ii as usize]);
        s.swap(ii as usize, jj as usize);
    }
    // PRGA
    let mut out = Vec::with_capacity(plaintext.len());
    for &p in plaintext {
        ii = ii.wrapping_add(1);
        jj = jj.wrapping_add(s[ii as usize]);
        s.swap(ii as usize, jj as usize);
        let k_idx = s[ii as usize].wrapping_add(s[jj as usize]) as usize;
        let k = s[k_idx];
        out.push(p ^ k);
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
{decrypt_fn}
return loadstring(_d(_s))(...)
"#,
        payload = payload_lit,
        key = key_lit,
        decrypt_fn = STAGE0_DECRYPT_LUAU,
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
    fn rc4_round_trips() {
        let key = [42u8; 32];
        let plaintext = b"hello, world! print(\"foo\")";
        let enc = encrypt_payload(plaintext, &key);
        assert_ne!(enc, plaintext);
        let dec = encrypt_payload(&enc, &key); // symmetric
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

    #[test]
    fn rc4_rust_luau_agree() {
        // Build a small ciphertext in Rust.
        let key = [37u8; 32];
        let plaintext = b"return \"abc\"";
        let cipher = encrypt_payload(plaintext, &key);

        // Render a tiny Luau program that runs the Luau _d (from STAGE0_DECRYPT_LUAU)
        // on the ciphertext and prints the decrypted bytes.
        let lua = format!(r#"
local _s = "{cipher_lit}"
local _k = "{key_lit}"
{decrypt_fn}
print(_d(_s))
"#,
            cipher_lit = crate::render::encode_luau_string_literal(&cipher),
            key_lit = crate::render::encode_luau_string_literal(&key),
            decrypt_fn = STAGE0_DECRYPT_LUAU,
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.luau");
        std::fs::write(&path, lua).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(), "luau failed: {}", String::from_utf8_lossy(&out.stderr));
        // `print` appends a newline; strip it before comparing.
        let stdout = out.stdout;
        let trimmed = stdout.strip_suffix(b"\n").unwrap_or(&stdout);
        assert_eq!(trimmed, plaintext);
    }
}
