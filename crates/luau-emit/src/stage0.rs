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

/// Optional runtime environment binding for the stage-0 key.
/// When present, the stage-0 wrapper mixes a runtime expression value into the
/// key at load time. The Rust side pre-folds `expected_value` into the cipher key.
///
/// `runtime_expr` is inserted VERBATIM as a Luau expression — trust the caller to
/// supply valid Luau. For example: `tostring(game.PlaceId)` or `"ABC"`.
#[derive(Debug, Clone)]
pub struct EmitEnvBinding {
    pub runtime_expr: String,
    pub expected_value: String,
}

/// XOR-fold `bind` bytes into the base key, cyclically over 32 bytes.
/// This matches the Luau `_mix` function emitted in `render_stage0` when a
/// binding is present.
pub fn fold_key(base: &[u8; 32], bind: &[u8]) -> [u8; 32] {
    let mut k = *base;
    for (i, &b) in bind.iter().enumerate() {
        k[i % 32] ^= b;
    }
    k
}

/// Render the stage-0 bootstrap text given the encrypted payload + key.
/// Caller is responsible for applying identifier mangling + comment
/// stripping to the result if desired.
pub fn render_stage0(encrypted_payload: &[u8], base_key: &[u8; 32], binding: Option<&EmitEnvBinding>) -> String {
    let payload_lit = crate::render::encode_luau_string_literal(encrypted_payload);
    match binding {
        None => {
            let key_lit = crate::render::encode_luau_string_literal(base_key);
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
        Some(b) => {
            let key_lit = crate::render::encode_luau_string_literal(base_key);
            let runtime_expr = &b.runtime_expr;
            format!(
                r#"local _s = "{payload}"
local _kbase = "{key}"
local _bind = {runtime_expr}
local function _mix(base, bind)
    local kb = {{}}
    for i = 1, 32 do kb[i] = string.byte(base, i) end
    if bind ~= nil then
        local bs = tostring(bind)
        for i = 1, #bs do
            local k_idx = ((i - 1) % 32) + 1
            kb[k_idx] = bit32.bxor(kb[k_idx], string.byte(bs, i))
        end
    end
    local out = {{}}
    for i = 1, 32 do out[i] = string.char(kb[i]) end
    return table.concat(out)
end
local _k = _mix(_kbase, _bind)
{decrypt_fn}
return loadstring(_d(_s))(...)
"#,
                payload = payload_lit,
                key = key_lit,
                runtime_expr = runtime_expr,
                decrypt_fn = STAGE0_DECRYPT_LUAU,
            )
        }
    }
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
        let text = render_stage0(payload, &key, None);
        assert!(text.contains("loadstring"));
        assert!(text.contains("local _s"));
        assert!(text.contains("local _k"));
    }

    #[test]
    fn fold_key_xors_cyclically() {
        let base = [0u8; 32];
        let bind = b"ABCD";
        let k = fold_key(&base, bind);
        assert_eq!(k[0], b'A');
        assert_eq!(k[1], b'B');
        assert_eq!(k[2], b'C');
        assert_eq!(k[3], b'D');
        for i in 4..32 { assert_eq!(k[i], 0); }
    }

    #[test]
    fn fold_key_wraps_at_32() {
        let base = [0u8; 32];
        let bind = vec![0xFFu8; 33];
        let k = fold_key(&base, &bind);
        assert_eq!(k[0], 0); // 0xFF XOR 0xFF
        for i in 1..32 { assert_eq!(k[i], 0xFF); }
    }

    #[test]
    fn render_stage0_no_binding_omits_mix_helpers() {
        let key = [1u8; 32];
        let payload = b"return 42";
        let text = render_stage0(payload, &key, None);
        assert!(!text.contains("_kbase"));
        assert!(!text.contains("_bind"));
        assert!(!text.contains("_mix"));
        assert!(text.contains("local _k ="));  // simple direct assignment
    }

    #[test]
    fn render_stage0_with_binding_emits_mix_helpers() {
        let key = [1u8; 32];
        let payload = b"return 42";
        let binding = EmitEnvBinding {
            runtime_expr: r#""ABC""#.to_string(),
            expected_value: "ABC".to_string(),
        };
        let text = render_stage0(payload, &key, Some(&binding));
        assert!(text.contains("_kbase"));
        assert!(text.contains("_bind"));
        assert!(text.contains("_mix"));
        assert!(text.contains("local _k = _mix("));
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
