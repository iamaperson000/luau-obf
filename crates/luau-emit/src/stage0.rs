//! Stage-0 self-decrypting bootstrap. Wraps a stage-1 Luau source string in
//! a small wrapper that decrypts and `loadstring`-evaluates it at runtime.
//!
//! After this wrapper is rendered and mangled, the obfuscated output's
//! top-level structure is unrecognizable — only the wrapper's locals,
//! a giant encrypted byte-string literal, and a `loadstring()` invocation
//! are visible. The dispatcher, helpers, opcode constants, constant pool,
//! and bytecode all live inside the encrypted payload.
//!
//! # Plan 32 Task 4: FNV-fingerprint key derivation
//!
//! The RC4 decryption key is no longer the literal `_kbase` embedded in the
//! wrapper.  Instead the effective key is:
//!
//!   key = _kbase XOR fnv_fingerprint(_kbase) [XOR bind_fold if binding present]
//!
//! `_kbase` IS still present literally, but an adversary needs to run the FNV
//! hash to recover the actual key.
//!
//! Note: the original plan called for hashing `_s` (the ciphertext) so that
//! tampering with it would break decryption.  That creates a chicken-and-egg:
//! the ciphertext depends on the key which depends on the ciphertext.  We scope
//! down to hashing `_kbase` instead; stage-1 integrity is covered by Plan 29's
//! env-bind and Plan 32 Task 3's per-proto CRC check.

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

/// 32-lane FNV-1a fingerprint function (Luau).
/// Each lane starts from a different offset so we get 32 independent u32 hashes,
/// then takes the low byte of (h >> 16) XOR (h & 0xFFFF) as its output byte.
/// This mirrors `fnv_fingerprint` in Rust exactly.
///
/// NOTE: no local aliases for bit32.bxor / math.floor are created inside this
/// function.  Aliases use short identifiers (like _xf) that the mangler may
/// rename to the same token as the loop variable `k`, causing a shadowing
/// collision ("attempt to call a number value").  Calling the globals directly
/// is safe because global names are never in the mangle table.
const STAGE0_FNV_LUAU: &str = r#"local function _fnv_fp(s)
    local fp = {}
    for k = 0, 31 do
        local h = (2166136261 + k * 2654435761) % 4294967296
        for i = 1, #s do
            h = bit32.bxor(h, string.byte(s, i))
            local lo = h % 65536
            local hi = math.floor(h / 65536)
            h = ((lo * 16777619) % 4294967296 + (hi * 16777619 * 65536) % 4294967296) % 4294967296
        end
        fp[k + 1] = string.char(bit32.bxor(math.floor(h / 65536), h % 65536) % 256)
    end
    return table.concat(fp)
end"#;

/// The Luau _mix(kbase, s_fp, bind) function used when env-binding is present.
/// Combines kbase, the FNV fingerprint, and an optional runtime bind value.
const STAGE0_MIX3_LUAU: &str = r#"local function _mix(kbase, s_fp, bind)
    local kb = {}
    for i = 1, 32 do kb[i] = bit32.bxor(string.byte(kbase, i), string.byte(s_fp, i)) end
    if bind ~= nil and bind ~= "" then
        local bs = tostring(bind)
        for i = 1, #bs do
            local k_idx = ((i - 1) % 32) + 1
            kb[k_idx] = bit32.bxor(kb[k_idx], string.byte(bs, i))
        end
    end
    local out = {}
    for i = 1, 32 do out[i] = string.char(kb[i]) end
    return table.concat(out)
end"#;

/// FNV-1a with 32 parallel lanes — produces a 32-byte fingerprint.
///
/// Lane `k` starts from `FNV_OFFSET + k * 2654435761`, processes every byte of
/// `bytes`, then contributes `(h >> 16) XOR (h & 0xFFFF)` (low byte) to the
/// output.  This mirrors the `_fnv_fp` Luau function emitted by `STAGE0_FNV_LUAU`.
pub fn fnv_fingerprint(bytes: &[u8]) -> [u8; 32] {
    const FNV_OFFSET: u32 = 2_166_136_261;
    const FNV_PRIME: u32 = 16_777_619;
    let mut out = [0u8; 32];
    for k in 0..32usize {
        let mut h: u32 = FNV_OFFSET.wrapping_add((k as u32).wrapping_mul(2_654_435_761));
        for &b in bytes {
            h ^= b as u32;
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Combine upper and lower half-words, take low byte.
        out[k] = ((h >> 16) ^ (h & 0xFFFF)) as u8;
    }
    out
}

/// Derive the effective RC4 key from the committed base, a fingerprint of the
/// base, and an optional env-binding string.
///
///   key[i] = base[i] XOR fp[i] XOR bind_fold[i]
///
/// Note: we hash `base` (the literal `_kbase` in the wrapper), NOT the
/// ciphertext, to avoid the chicken-and-egg where key ↔ ciphertext depend on
/// each other.  Stage-1 integrity is covered by Plan 32 Task 3 CRC checks.
pub fn derive_runtime_key(base: &[u8; 32], bind: Option<&str>) -> [u8; 32] {
    let fp = fnv_fingerprint(base);
    let mut key = [0u8; 32];
    for i in 0..32 {
        key[i] = base[i] ^ fp[i];
    }
    if let Some(s) = bind {
        for (i, &b) in s.as_bytes().iter().enumerate() {
            key[i % 32] ^= b;
        }
    }
    key
}

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

/// Render the stage-0 bootstrap text given the encrypted payload + base key.
///
/// The effective RC4 key is derived at runtime as:
///   key = _mix(_kbase, _fnv_fp(_kbase) [, _bind])
/// which XORs the base with its own FNV fingerprint (and optionally a bound
/// runtime value).  Caller is responsible for applying identifier mangling +
/// comment stripping to the result if desired.
pub fn render_stage0(encrypted_payload: &[u8], base_key: &[u8; 32], binding: Option<&EmitEnvBinding>) -> String {
    let payload_lit = crate::render::encode_luau_string_literal(encrypted_payload);
    let key_lit = crate::render::encode_luau_string_literal(base_key);
    match binding {
        None => {
            // No env-binding: derive key purely from kbase XOR fnv_fp(kbase).
            format!(
                r#"local _s = "{payload}"
local _kbase = "{key}"
{fnv_fn}
local _k = (function(kbase, s_fp)
    local out = {{}}
    for i = 1, 32 do out[i] = string.char(bit32.bxor(string.byte(kbase, i), string.byte(s_fp, i))) end
    return table.concat(out)
end)(_kbase, _fnv_fp(_kbase))
{decrypt_fn}
return loadstring(_d(_s))(...)
"#,
                payload = payload_lit,
                key = key_lit,
                fnv_fn = STAGE0_FNV_LUAU,
                decrypt_fn = STAGE0_DECRYPT_LUAU,
            )
        }
        Some(b) => {
            let runtime_expr = &b.runtime_expr;
            format!(
                r#"local _s = "{payload}"
local _kbase = "{key}"
local _bind = {runtime_expr}
{fnv_fn}
{mix_fn}
local _k = _mix(_kbase, _fnv_fp(_kbase), _bind)
{decrypt_fn}
return loadstring(_d(_s))(...)
"#,
                payload = payload_lit,
                key = key_lit,
                runtime_expr = runtime_expr,
                fnv_fn = STAGE0_FNV_LUAU,
                mix_fn = STAGE0_MIX3_LUAU,
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
        assert!(text.contains("local _kbase"));
        assert!(text.contains("_fnv_fp"));
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
    fn render_stage0_no_binding_uses_kbase_and_fnv() {
        let key = [1u8; 32];
        let payload = b"return 42";
        let text = render_stage0(payload, &key, None);
        // No-binding branch still uses _kbase and _fnv_fp; it just omits _bind and _mix.
        assert!(text.contains("_kbase"));
        assert!(text.contains("_fnv_fp"));
        assert!(!text.contains("_bind"));
        assert!(!text.contains("local function _mix"));
        assert!(text.contains("local _k ="));
    }

    #[test]
    fn render_stage0_with_binding_emits_fnv_and_mix_helpers() {
        let key = [1u8; 32];
        let payload = b"return 42";
        let binding = EmitEnvBinding {
            runtime_expr: r#""ABC""#.to_string(),
            expected_value: "ABC".to_string(),
        };
        let text = render_stage0(payload, &key, Some(&binding));
        assert!(text.contains("_kbase"));
        assert!(text.contains("_bind"));
        assert!(text.contains("_fnv_fp"));
        assert!(text.contains("_mix"));
        assert!(text.contains("local _k = _mix("));
    }

    #[test]
    fn deterministic_key_for_same_seed() {
        let mut r1 = ChaCha20Rng::from_seed([7u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([7u8; 32]);
        assert_eq!(derive_stage0_key(&mut r1), derive_stage0_key(&mut r2));
    }

    // --- Plan 32 Task 4: fnv_fingerprint unit tests ---

    #[test]
    fn fnv_fingerprint_deterministic() {
        let a = fnv_fingerprint(b"hello world");
        let b = fnv_fingerprint(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn fnv_fingerprint_changes_with_input() {
        let a = fnv_fingerprint(b"abc");
        let b = fnv_fingerprint(b"xyz");
        assert_ne!(a, b);
    }

    #[test]
    fn fnv_fingerprint_empty_input() {
        // Should produce a deterministic non-zero result (lanes start at different offsets).
        let fp = fnv_fingerprint(b"");
        // Just check it's 32 bytes and consistent.
        assert_eq!(fp.len(), 32);
        let fp2 = fnv_fingerprint(b"");
        assert_eq!(fp, fp2);
    }

    #[test]
    fn derive_runtime_key_no_bind_differs_from_base() {
        let base = [0xABu8; 32];
        let key = derive_runtime_key(&base, None);
        // key = base XOR fnv_fingerprint(base); since fp is non-zero on average, key != base.
        let fp = fnv_fingerprint(&base);
        let expected: [u8; 32] = std::array::from_fn(|i| base[i] ^ fp[i]);
        assert_eq!(key, expected);
    }

    #[test]
    fn derive_runtime_key_with_bind_folds_in() {
        let base = [0u8; 32];
        let key_no_bind = derive_runtime_key(&base, None);
        let key_bind = derive_runtime_key(&base, Some("ABC"));
        assert_ne!(key_no_bind, key_bind);
    }

    /// Cross-impl test: Rust fnv_fingerprint must agree with the Luau _fnv_fp
    /// emitted in STAGE0_FNV_LUAU for a set of reference inputs.
    #[test]
    fn rust_luau_fnv_fingerprint_agree() {
        let inputs: &[&[u8]] = &[b"abc", b"hello world", &[0u8; 32], &[0xFFu8; 32]];
        for input in inputs {
            let rust_fp = fnv_fingerprint(input);

            // Build a space-separated decimal list using table.concat + print
            // (no io.write — not available in the Luau sandbox).
            let luau = format!(
                r#"{fnv_fn}
local s = "{input_lit}"
local fp = _fnv_fp(s)
local parts = {{}}
for i = 1, #fp do parts[i] = tostring(string.byte(fp, i)) end
print(table.concat(parts, " "))
"#,
                fnv_fn = STAGE0_FNV_LUAU,
                input_lit = crate::render::encode_luau_string_literal(input),
            );

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("fnv_test.luau");
            std::fs::write(&path, &luau).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(
                out.status.success(),
                "luau failed for input {:?}: {}",
                input,
                String::from_utf8_lossy(&out.stderr)
            );

            let line = String::from_utf8_lossy(&out.stdout);
            let luau_bytes: Vec<u8> = line
                .split_whitespace()
                .map(|s| s.parse::<u8>().expect("expected decimal byte"))
                .collect();
            assert_eq!(
                luau_bytes.as_slice(),
                &rust_fp[..],
                "FNV fingerprint mismatch for input {:?}: rust={:?} luau={:?}",
                input, &rust_fp[..], luau_bytes
            );
        }
    }

    /// End-to-end cross-impl test: encrypt with derive_runtime_key, then
    /// decrypt in Luau using the stage-0 template (no binding branch).
    #[test]
    fn rust_luau_stage0_fnv_round_trip_no_bind() {
        let base: [u8; 32] = {
            let mut b = [0u8; 32];
            for (i, slot) in b.iter_mut().enumerate() { *slot = (i * 7 + 13) as u8; }
            b
        };
        let plaintext = b"return \"plan32task4\"";
        let runtime_key = derive_runtime_key(&base, None);
        let ciphertext = encrypt_payload(plaintext, &runtime_key);

        let stage0 = render_stage0(&ciphertext, &base, None);
        // Replace `return loadstring(...)` with `print(loadstring(...))` so we can see output.
        let luau = stage0.replace(
            "return loadstring(_d(_s))(...)",
            "print(loadstring(_d(_s))())",
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stage0_fnv_rt.luau");
        std::fs::write(&path, &luau).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(
            out.status.success(),
            "luau failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = out.stdout;
        let trimmed = stdout.strip_suffix(b"\n").unwrap_or(&stdout);
        assert_eq!(trimmed, b"plan32task4");
    }

    /// End-to-end cross-impl test: encrypt with derive_runtime_key + bind, decrypt in Luau.
    #[test]
    fn rust_luau_stage0_fnv_round_trip_with_bind() {
        let base: [u8; 32] = {
            let mut b = [0u8; 32];
            for (i, slot) in b.iter_mut().enumerate() { *slot = (i * 11 + 5) as u8; }
            b
        };
        let bind_value = "mysecretenv";
        let plaintext = b"return \"bound\"";
        let runtime_key = derive_runtime_key(&base, Some(bind_value));
        let ciphertext = encrypt_payload(plaintext, &runtime_key);

        let binding = EmitEnvBinding {
            runtime_expr: format!("\"{}\"", bind_value),
            expected_value: bind_value.to_string(),
        };
        let stage0 = render_stage0(&ciphertext, &base, Some(&binding));
        let luau = stage0.replace(
            "return loadstring(_d(_s))(...)",
            "print(loadstring(_d(_s))())",
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stage0_fnv_bind.luau");
        std::fs::write(&path, &luau).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(
            out.status.success(),
            "luau failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = out.stdout;
        let trimmed = stdout.strip_suffix(b"\n").unwrap_or(&stdout);
        assert_eq!(trimmed, b"bound");
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
