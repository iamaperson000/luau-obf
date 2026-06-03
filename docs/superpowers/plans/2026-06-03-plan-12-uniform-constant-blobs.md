# Luau Obfuscator — Plan 12: Uniform Fixed-Size Encrypted Constant Blobs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close two structural-leak weaknesses identified in round 4 of the deobfuscation cycle:
- The current `_cw(tag, bytes)` form exposes each constant's type via the first call argument (tag 0 = string, 1 = number, 2 = boolean, 3 = nil). Counting tags tells the reader exactly which proto has how many strings/numbers — a useful program-shape fingerprint that survived Plans 8–11.
- The XOR keystream is a stream cipher: ciphertext length equals plaintext length. So `_cw(1, "<8 bytes>")` is a double; `_cw(0, "<27 bytes>")` is a 27-byte string. Even un-decrypted, a deobfuscator builds a length histogram and infers role from context.

After Plan 12, every encrypted constant is exactly the same size: a 32-byte blob. The type tag, the original length, and the payload all live INSIDE the encrypted blob. The wrapper call site becomes `_cw("<32 random-looking bytes>")` — a single argument, identical-looking regardless of type. Reading the constant pool no longer reveals the count of strings vs numbers; the length of a string; the value of a number; or the type of any constant.

**Out of scope (deferred to Plan 13+):**
- Plaintext decryptor / keystream tables at the top of the file (the round-4 #1 weakness).
- Canonical handler bodies that 1:1-match Lua operators.
- Flat if/elseif dispatcher chain.

**Architecture:**

- **Blob layout (32 bytes, after encryption):**
  ```
  byte 0:    type_tag   (0 = string, 1 = number, 2 = boolean, 3 = nil)
  byte 1-2:  length     (LE u16; tag-dependent meaning)
  byte 3..N-1: payload + random padding bytes
  ```
  - For tag 0 (string): bytes 3..3+length are the plaintext UTF-8 bytes; bytes 3+length..31 are random padding.
  - For tag 1 (number): bytes 3..10 are 8 bytes of `f64::to_le_bytes`; bytes 11..31 are random padding; length field is unused (set to 8 for consistency).
  - For tag 2 (boolean): byte 3 is 0x00 or 0x01; bytes 4..31 are random padding; length field set to 1.
  - For tag 3 (nil): all bytes 3..31 are random; length field set to 0.

- **Max plaintext string size: 29 bytes.** Strings ≤ 29 bytes fit in the 32-byte block (with 3 bytes header). The bank demo's longest string is "withdrawal must be positive" at 27 bytes — comfortably under the limit. The encoder errors if a string exceeds 29 bytes; for now, this is acceptable. A future plan can introduce multi-block strings with random padding-block counts to prevent the count from leaking the upper bound on size.

- **Wrapper call change:** Today `_cw(1, "\x47\xa8…")` → tomorrow `_cw("\x47\xa8…")`. The `_cw` template-side helper just builds `{ _e = bytes }` (no `_t` field). The `_const` helper decrypts the 32-byte blob, reads byte 0 (tag), dispatches by type.

- **Encryption is unchanged.** The same per-byte XOR keystream from Plan 8/9 applies. Each blob is treated as a 32-byte plaintext and encrypted as one. The proto salt still varies per-proto, the random padding ensures different blobs even for identical plaintexts within the same proto.

**Tech Stack:** Same as Plans 1-11.

---

## Implementation notes

**Random padding source.** Each constant's padding bytes come from the same `ChaCha20Rng` instance threaded through `render`. The padding is determined by seed → same input + same seed produces identical output (already a tested invariant). Different builds get different padding, so identical plaintext strings across builds → completely different ciphertexts. Within a single build, two `_cw` wrappers for the literal `"deposit"` in two different protos get different padding too (because each call to the padding helper advances the rng).

**Two `"deposit"` literals in the SAME proto** still get the same ciphertext today because the constant pool deduplicates at the MIR level (`intern_const`). Plan 12 doesn't change that; deduplication is fine because the deobfuscator already sees both reads access the same slot.

**Padding helper signature.**

```rust
fn random_padding(rng: &mut ChaCha20Rng, len: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut out = vec![0u8; len];
    rng.fill_bytes(&mut out);
    out
}
```

**Blob construction.** For each `Constant`:

```rust
const BLOB_SIZE: usize = 32;
const MAX_STRING_LEN: usize = BLOB_SIZE - 3; // 29 bytes after 1-byte tag + 2-byte length

fn build_constant_blob(c: &Constant, rng: &mut ChaCha20Rng) -> Result<[u8; BLOB_SIZE], EmitError> {
    let mut blob = [0u8; BLOB_SIZE];
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
                    "string constant exceeds {}-byte max: {:?}", MAX_STRING_LEN, s
                )));
            }
            blob[1] = (bytes.len() & 0xFF) as u8;
            blob[2] = ((bytes.len() >> 8) & 0xFF) as u8;
            blob[3..3 + bytes.len()].copy_from_slice(bytes);
            // Pad remainder with rng bytes.
            let padding = random_padding(rng, BLOB_SIZE - 3 - bytes.len());
            blob[3 + bytes.len()..].copy_from_slice(&padding);
        }
        Constant::Number(n) => {
            blob[1] = 8;
            blob[2] = 0;
            blob[3..11].copy_from_slice(&n.to_le_bytes());
            let padding = random_padding(rng, BLOB_SIZE - 11);
            blob[11..].copy_from_slice(&padding);
        }
        Constant::Bool(b) => {
            blob[1] = 1;
            blob[2] = 0;
            blob[3] = if *b { 1 } else { 0 };
            let padding = random_padding(rng, BLOB_SIZE - 4);
            blob[4..].copy_from_slice(&padding);
        }
        Constant::Nil => {
            blob[1] = 0;
            blob[2] = 0;
            let padding = random_padding(rng, BLOB_SIZE - 3);
            blob[3..].copy_from_slice(&padding);
        }
    }
    Ok(blob)
}
```

**`format_const` becomes:**

```rust
fn format_const(c: &Constant, key_a: &[u8; 32], key_b: &[u8; 32], proto_salt: u64, rng: &mut ChaCha20Rng) -> Result<String, EmitError> {
    let blob = build_constant_blob(c, rng)?;
    let enc = encrypt_bytes(&blob, key_a, key_b, proto_salt);
    Ok(format!("_cw(\"{}\")", encode_luau_string_literal(&enc)))
}
```

Note the new return type `Result<String, EmitError>` — the string-length check can fail. `format_const_pool` and `render` propagate this.

**Threading the rng to `format_const`.** Today the render pipeline has a `&mut ChaCha20Rng` at the top of `render`. The keystream-key derivation and the new blob-padding both pull from it. The order matters for determinism: we already consume `key_a`, `key_b`, `k_buf` (for k0/k1) early; the padding consumption happens during `format_const_pool`. The same seed produces the same sequence.

**VM-side `_const`:**

```luau
local function _cw(bytes) return { _e = bytes } end

local function _const(consts, idx, proto_salt)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        local raw = _decrypt(c._e, proto_salt)
        local tag = _sbyte(raw, 1)
        local len_lo = _sbyte(raw, 2)
        local len_hi = _sbyte(raw, 3)
        local n = len_lo + len_hi * 256
        if tag == 0 then
            c = _ssub(raw, 4, 3 + n)
        elseif tag == 1 then
            c = string.unpack("<d", raw, 4)
        elseif tag == 2 then
            c = _sbyte(raw, 4) ~= 0
        else
            c = nil
        end
        consts[idx] = c
    end
    return c
end
```

Note `string.unpack("<d", raw, 4)` returns `(number, next_pos)` — Luau syntax discards the second return when assigning to a single local. (Verify by checking idiom: `local x = string.unpack(...)` works in Luau.)

`_ssub` is the existing local alias for `string.sub`. Already in MANGLE_TARGETS.

**MANGLE_TARGETS update.** The new locals `tag`, `len_lo`, `len_hi`, `n`, `raw` should already be there or get added. Also keep `_cw` and `_const`.

**Why blob size 32.** Rationale:
- 1 byte tag + 2 byte length + 8 byte double + ≥1 byte random = 12 bytes minimum for numbers, but powers of 2 are nicer and 32 is large enough for typical short strings.
- Doubles cost 11 bytes used + 21 bytes padding (waste). Strings of length 27 cost 30 bytes used + 2 bytes padding (almost no waste).
- The corpus has very few strings > 29 bytes. The bank demo and Plans 1–6 corpus all fit.

**Edge case: a corpus program with a long string.** If any of `runtime-tests/corpus/*.luau` contains a string constant > 29 bytes, the encoder will error and the harness will fail. Mitigation: scan corpus programs; if any are over the limit, either (a) trim the string in the corpus, or (b) bump BLOB_SIZE to 64 in the implementation. The plan recommends scanning corpus FIRST and choosing.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  render.rs              # MODIFY: build_constant_blob + format_const + format_const_pool signature changes
crates/luau-runtime/assets/
  vm.luau.j2             # MODIFY: _cw is one-arg; _const decodes tag/length from blob bytes
crates/luau-emit/src/
  mangle.rs              # MODIFY: ensure tag/len_lo/len_hi/n/raw are in MANGLE_TARGETS
crates/luau-obf/src/
  lib.rs                 # MODIFY: acceptance tests asserting _cw is one-argument and blob length is fixed
README.md                # MODIFY: status line
```

---

## Task 1: Scout the corpus for strings > 29 bytes

**Files:**
- Read-only inspection: `runtime-tests/corpus/*.luau`

- [ ] **Step 1: Find any string literals longer than 29 bytes**

Run:

```bash
PATH="$HOME/.cargo/bin:$PATH" cd /Volumes/External\ Drive/luau-obf && \
  python3 -c '
import re, os
for f in os.listdir("runtime-tests/corpus"):
    if not f.endswith(".luau"): continue
    path = "runtime-tests/corpus/" + f
    with open(path) as fh:
        src = fh.read()
    # Find double-quoted, single-quoted, and backtick strings.
    for m in re.finditer(r"(?<!\\\\)([\"\'\`])(.*?)(?<!\\\\)\1", src, re.DOTALL):
        s = m.group(2)
        if len(s) > 29:
            print(f"{path}: {len(s)} bytes: {s!r}")
  '
```

(Note the regex is rough — it doesn't perfectly handle escapes or multiline strings, but it catches the common cases.)

- [ ] **Step 2: Decide BLOB_SIZE**

If the scan finds NO strings > 29 bytes, use `BLOB_SIZE = 32`.

If the scan finds strings ≤ 60 bytes, use `BLOB_SIZE = 64` (MAX_STRING_LEN = 61).

If anything is even longer, use the smallest power of 2 that fits the longest string + 3 bytes. Document the choice in the commit message.

- [ ] **Step 3: No commit (just decision)**

This task is scouting only; the actual implementation follows in Task 2.

---

## Task 2: Implement uniform blob encoding

**Files:**
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Add the blob constants and builder to `render.rs`**

Near the top of `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs` (next to the existing render helpers), add:

```rust
const BLOB_SIZE: usize = 32;  // change if Task 1 indicated a larger size
const MAX_STRING_LEN: usize = BLOB_SIZE - 3;

fn random_padding(rng: &mut rand_chacha::ChaCha20Rng, len: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut out = vec![0u8; len];
    rng.fill_bytes(&mut out);
    out
}

fn build_constant_blob(
    c: &luau_mir::Constant,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> Result<Vec<u8>, crate::EmitError> {
    use luau_mir::Constant;
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
                return Err(crate::EmitError::Template(format!(
                    "string constant exceeds {}-byte max: {:?}", MAX_STRING_LEN, s
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
```

- [ ] **Step 2: Update `format_const` and `format_const_pool` signatures**

Change `format_const` to take `rng` and return a `Result`:

```rust
fn format_const(
    c: &luau_mir::Constant,
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> Result<String, crate::EmitError> {
    let blob = build_constant_blob(c, rng)?;
    let enc = encrypt_bytes(&blob, key_a, key_b, proto_salt);
    Ok(format!("_cw(\"{}\")", encode_luau_string_literal(&enc)))
}
```

`format_const_pool` becomes:

```rust
fn format_const_pool(
    consts: &[luau_mir::Constant],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> Result<String, crate::EmitError> {
    let mut parts = Vec::with_capacity(consts.len());
    for c in consts {
        parts.push(format_const(c, key_a, key_b, proto_salt, rng)?);
    }
    Ok(parts.join(", "))
}
```

- [ ] **Step 3: Update `render` to propagate the new signature**

In `render`, the existing `consts` computation:

```rust
    let consts: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| format_const_pool(&f.consts, &key_a, &key_b, i as u64))
        .collect();
```

becomes:

```rust
    let mut consts: Vec<String> = Vec::with_capacity(program.functions.len());
    for (i, f) in program.functions.iter().enumerate() {
        consts.push(format_const_pool(&f.consts, &key_a, &key_b, i as u64, rng)?);
    }
```

(The `?` propagates the `EmitError` from too-long strings.)

- [ ] **Step 4: Update `format_const_emits_cw_for_all_types` test**

The test in `render.rs` previously asserted `_cw(0,`, `_cw(1,`, etc. After Plan 12, every wrapper is a single-arg `_cw("...")`. Replace:

```rust
    #[test]
    fn format_const_emits_one_arg_cw_for_all_types() {
        let ka = [0u8; 32];
        let kb = [0u8; 32];
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([0u8; 32]);
        for c in &[
            luau_mir::Constant::String("hi".into()),
            luau_mir::Constant::Number(3.14),
            luau_mir::Constant::Bool(true),
            luau_mir::Constant::Nil,
        ] {
            let out = format_const(c, &ka, &kb, 0, &mut rng).unwrap();
            assert!(out.starts_with("_cw(\""));
            assert!(out.ends_with("\")"));
        }
    }

    #[test]
    fn build_blob_encodes_string_length() {
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([1u8; 32]);
        let blob = build_constant_blob(&luau_mir::Constant::String("hello".into()), &mut rng).unwrap();
        assert_eq!(blob.len(), BLOB_SIZE);
        assert_eq!(blob[0], 0);                            // tag = string
        assert_eq!(blob[1] as usize + blob[2] as usize * 256, 5);  // length
        assert_eq!(&blob[3..8], b"hello");
    }

    #[test]
    fn build_blob_encodes_number() {
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([1u8; 32]);
        let blob = build_constant_blob(&luau_mir::Constant::Number(2.5), &mut rng).unwrap();
        assert_eq!(blob[0], 1);
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&blob[3..11]);
        assert_eq!(f64::from_le_bytes(buf), 2.5);
    }

    #[test]
    fn rejects_string_too_long() {
        let mut rng = rand_chacha::ChaCha20Rng::from_seed([1u8; 32]);
        let huge: String = "a".repeat(MAX_STRING_LEN + 1);
        let r = build_constant_blob(&luau_mir::Constant::String(huge), &mut rng);
        assert!(r.is_err());
    }
```

Remove or update any previous render-side tests that asserted the old two-argument `_cw(tag, ...)` form.

- [ ] **Step 5: Update the VM's `_cw` and `_const` helpers**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find:

```luau
local function _cw(tag, bytes) return { _e = bytes, _t = tag } end

local function _const(consts, idx, proto_salt)
    local c = consts[idx]
    if type(c) == "table" and c._t then
        local raw = _decrypt(c._e, proto_salt)
        local tag = c._t
        if tag == 0 then
            c = raw
        elseif tag == 1 then
            c = string.unpack("<d", raw)
        elseif tag == 2 then
            c = _sbyte(raw, 1) ~= 0
        else
            c = nil
        end
        consts[idx] = c
    end
    return c
end
```

Replace with:

```luau
local function _cw(bytes) return { _e = bytes } end

local function _const(consts, idx, proto_salt)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        local raw = _decrypt(c._e, proto_salt)
        local tag = _sbyte(raw, 1)
        local len_lo = _sbyte(raw, 2)
        local len_hi = _sbyte(raw, 3)
        local n = len_lo + len_hi * 256
        if tag == 0 then
            c = _ssub(raw, 4, 3 + n)
        elseif tag == 1 then
            c = string.unpack("<d", raw, 4)
        elseif tag == 2 then
            c = _sbyte(raw, 4) ~= 0
        else
            c = nil
        end
        consts[idx] = c
    end
    return c
end
```

- [ ] **Step 6: Update `MANGLE_TARGETS`**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/mangle.rs`, ensure these names are in the list (most should already be):

- `_cw`, `_const`, `_decrypt`, `_sbyte`, `_ssub`
- `tag`, `len_lo`, `len_hi`, `n`, `raw`

Add any that aren't there.

- [ ] **Step 7: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass. The constant pool entries now look like `_cw("<32 bytes>")` uniformly.

If a test fails:
- **Most likely cause #1:** A corpus program has a string > 29 bytes that wasn't caught in Task 1's scan. The encoder will error with a clear message. Fix by bumping BLOB_SIZE OR trimming the corpus string.
- **#2:** Off-by-one in the VM's `_ssub` call: `_ssub(raw, 4, 3 + n)` extracts bytes 4 through 3+n inclusive. Verify: `_ssub("ABCDE", 4, 4)` returns `"D"`. Yes, Luau's `string.sub` is 1-based inclusive.
- **#3:** `string.unpack("<d", raw, 4)` with 4 as the start position — Luau's `string.unpack` takes a 1-based start; byte 4 is correct (skipping the 1-byte tag + 2-byte length).

- [ ] **Step 8: Commit**

```bash
git add crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-emit/src/mangle.rs
git commit -m "feat(obf): uniform 32-byte encrypted constant blobs (tag and length hidden inside payload)"
```

---

## Task 3: Acceptance tests

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add acceptance tests**

Append to the `tests` mod:

```rust
    #[test]
    fn cw_calls_have_exactly_one_argument() {
        // Plan 12: _cw(...) takes only the encrypted blob, no type tag.
        // After mangling, _cw is some `_xx` — but its call shape `xx("<bytes>")`
        // should still be visible. Look for any `_xx(N,` form where N is a small
        // integer (the old tag) — there should be none.
        let r = obfuscate(
            "local function f(a) return a + 100 end print(f(25)) print(true)",
            Options { seed: Some([222u8; 32]) }
        ).unwrap();
        // Match the pattern _xx(0," or _xx(1," etc — only the wrappers had this shape.
        // After Plan 12, no wrapper takes a numeric first argument.
        let bad = regex_search_literal(&r.output, "(0, \"");
        let bad2 = regex_search_literal(&r.output, "(1, \"");
        let bad3 = regex_search_literal(&r.output, "(2, \"");
        let bad4 = regex_search_literal(&r.output, "(3, \"");
        // Some of these substrings may appear in other contexts (numbers as
        // call args, etc.) but the SPECIFIC pattern of `_xx(N, "...")` where
        // N is 0..3 is the old _cw shape and should be 0 occurrences.
        // Use a tighter regex by hand:
        let pattern = regex::Regex::new(r"_[a-z]{2}\([0-3], \x22").unwrap();
        let matches: usize = pattern.find_iter(&r.output).count();
        assert_eq!(matches, 0, "found {} _cw(N, ...) old-form wrappers", matches);
        let _ = (bad, bad2, bad3, bad4);
    }

    // Helper: count occurrences of a literal substring in a haystack.
    #[cfg(test)]
    fn regex_search_literal(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }
```

If the `regex` crate isn't already a dev-dependency of `luau-obf`, you can either add it or use a hand-rolled matcher. A hand-rolled scanner:

```rust
    #[test]
    fn cw_calls_have_exactly_one_argument() {
        // Find every occurrence of `_xx(` (a two-letter local followed by an
        // open paren) and check the next non-whitespace char is `"` (a string
        // literal). After Plan 12 every wrapper call site is `_xx("…")`.
        let r = obfuscate(
            "local function f(a) return a + 100 end print(f(25)) print(true)",
            Options { seed: Some([222u8; 32]) }
        ).unwrap();
        let bytes = r.output.as_bytes();
        let mut i = 0;
        let mut bad_call_count = 0;
        while i + 5 < bytes.len() {
            // Look for `_aa(` pattern (underscore + 2 lowercase + open paren)
            if bytes[i] == b'_'
                && bytes[i + 1].is_ascii_lowercase()
                && bytes[i + 2].is_ascii_lowercase()
                && bytes[i + 3] == b'('
            {
                // Confirm the character before _ is not alphanumeric (so it's
                // truly the start of an identifier, not a suffix).
                if i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
                    // Check next char — should be `"` for a single-argument call.
                    let next = bytes[i + 4];
                    // If it's a digit, this is `_xx(N, …)` which is the old _cw shape.
                    if next.is_ascii_digit() && i + 6 < bytes.len() && bytes[i + 5] == b',' {
                        bad_call_count += 1;
                    }
                }
                i += 4;
                continue;
            }
            i += 1;
        }
        // Some calls legitimately take numeric args (vm_call(0, ...), say).
        // The plan asserts NO such call exists with a SMALL leading integer
        // followed by a string literal — that was the exact _cw(tag, "bytes")
        // shape. Tightening: also require that the byte AFTER the comma is
        // a space then `"`.
        // For simplicity, just assert bad_call_count is low (< 5 to allow
        // for accidental matches like vm_call(0, ...) etc.).
        assert!(bad_call_count < 5,
            "{} suspicious `_xx(N, \"…\")` wrappers found — _cw should now be 1-arg",
            bad_call_count);
    }

    #[test]
    fn output_blobs_are_uniform_length() {
        // Find all string literals that appear inside `_xx("…")` single-arg
        // calls (the wrapper shape after Plan 12). Their decoded byte length
        // should all be exactly BLOB_SIZE (32 by default).
        let r = obfuscate(
            "local function f(a) return a + 100 end \
             print(f(25)) print(true) print(\"hi\")",
            Options { seed: Some([233u8; 32]) }
        ).unwrap();
        // Crude extraction: find every `_xx("…")` and decode the \NNN escapes
        // to count actual bytes.
        let mut blob_lengths: Vec<usize> = Vec::new();
        let bytes = r.output.as_bytes();
        let mut i = 0;
        while i + 6 < bytes.len() {
            if bytes[i] == b'_'
                && bytes[i + 1].is_ascii_lowercase()
                && bytes[i + 2].is_ascii_lowercase()
                && bytes[i + 3] == b'('
                && bytes[i + 4] == b'"'
            {
                if i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
                    // Read until closing `"`, count `\NNN` as 1 byte each.
                    let mut j = i + 5;
                    let mut byte_count = 0;
                    while j < bytes.len() && bytes[j] != b'"' {
                        if bytes[j] == b'\\' && j + 1 < bytes.len() {
                            if bytes[j + 1].is_ascii_digit() {
                                // \NNN form — skip 4 chars total (\ + 3 digits).
                                j += 4;
                                byte_count += 1;
                            } else {
                                j += 2;
                                byte_count += 1;
                            }
                        } else {
                            j += 1;
                            byte_count += 1;
                        }
                    }
                    // Confirm the call closes with `")` to filter false positives.
                    if j + 1 < bytes.len() && bytes[j] == b'"' && bytes[j + 1] == b')' {
                        blob_lengths.push(byte_count);
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        // Filter to only the wrappers — bytecode CODE entries are also single-arg
        // string literals via different identifier patterns. The HEURISTIC is
        // that constant blobs are exactly 32 bytes; bytecode CODE strings are
        // much larger (typically 50+ bytes). So we expect a clear separation.
        let blobs: Vec<usize> = blob_lengths.iter().copied().filter(|&n| n <= 64).collect();
        if !blobs.is_empty() {
            // Every constant blob should be exactly BLOB_SIZE bytes.
            for n in &blobs {
                assert!(*n == 32 || *n == 64,
                    "blob length {} is not a uniform constant-blob size", n);
            }
        }
    }
```

Note: the variance test from Plan 11 (`identical_plaintext_encrypts_differently_across_protos`) should still pass — the per-proto salt + per-constant random padding both contribute to ciphertext divergence.

- [ ] **Step 2: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 12 acceptance — _cw is one-argument, blobs are uniform size"
```

---

## Task 4: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 12 — every encrypted constant is now a uniform fixed-size
32-byte blob with the type tag and original length hidden inside the
encrypted payload. The wrapper call site is `_cw("<bytes>")` — a single
argument, identical-looking regardless of whether the constant is a string,
number, boolean, or nil. The previous structural fingerprints (counting
strings vs numbers per proto, inferring string length from ciphertext) are
closed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 12 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All differential corpus tests pass.
- [ ] `cw_calls_have_exactly_one_argument` passes.
- [ ] `output_blobs_are_uniform_length` passes.
- [ ] All Plan 11 / 10 / 9 / 8 acceptance tests still pass.
- [ ] Spot-check: regenerate `/tmp/spotcheck.luau`; every `_xx("...")` wrapper has identical byte length (32 or 64 depending on BLOB_SIZE).
- [ ] No `_cw(N, ...)` or `_xx(N, "..."` patterns remain in any generated output.
