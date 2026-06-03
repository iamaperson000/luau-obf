# Luau Obfuscator — Plan 28: Stream-cipher stage-0

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Replace the positional XOR in stage-0 (`b ^ key[i%32] ^ (i%256)`)
with a stream cipher whose keystream is non-positional. The Round 4
adversary recovered the 32-byte key in ~30 lines of Python by bucketing
ciphertext bytes by `i mod 32` and exploiting `\\` density. A stream
cipher with internal state defeats that bucketing because position no
longer maps deterministically to keystream slot.

Use **RC4 (ARC4)** for the keystream. RC4 is broken cryptographically
but is **textbook stream cipher** that's trivial in both Rust and Luau
and is exactly the right tool for "make the per-byte keystream non-positional".

---

## Algorithm

RC4 keystream init (KSA):

```
S[0..256] = identity
j = 0
for i in 0..256:
    j = (j + S[i] + key[i % key_len]) % 256
    swap(S[i], S[j])
```

Keystream byte (PRGA):

```
i = (i + 1) % 256
j = (j + S[i]) % 256
swap(S[i], S[j])
return S[(S[i] + S[j]) % 256]
```

Encryption / decryption: `ciphertext[k] = plaintext[k] XOR keystream_byte`.

Use a **256-byte drop** at the start (drop the first 256 keystream bytes
before encrypting) — this avoids the well-known RC4 bias in the first
output bytes and adds a free anti-known-plaintext margin.

---

## Implementation

### `crates/luau-emit/src/stage0.rs`

Add:

```rust
/// RC4-with-drop-256 encryption / decryption (the operation is symmetric).
pub fn rc4_encrypt(plaintext: &[u8], key: &[u8]) -> Vec<u8> {
    let mut s = [0u8; 256];
    for i in 0..256 { s[i] = i as u8; }
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    // Drop 256.
    let mut ii: u8 = 0;
    let mut jj: u8 = 0;
    for _ in 0..256 {
        ii = ii.wrapping_add(1);
        jj = jj.wrapping_add(s[ii as usize]);
        s.swap(ii as usize, jj as usize);
    }
    let mut out = Vec::with_capacity(plaintext.len());
    for &p in plaintext {
        ii = ii.wrapping_add(1);
        jj = jj.wrapping_add(s[ii as usize]);
        s.swap(ii as usize, jj as usize);
        let k = s[(s[ii as usize].wrapping_add(s[jj as usize])) as usize];
        out.push(p ^ k);
    }
    out
}
```

Replace `encrypt_payload` with a call to `rc4_encrypt` keyed on the
existing 32-byte stage-0 key.

### `render_stage0`

Replace the Luau `_d(s)` helper body with an RC4 decryptor in pure
Luau. The current body is ~10 lines; the RC4 version is ~25.

Template:

```luau
local function _d(s)
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
end
```

**Note:** Luau supports tuple assignment for the swap. No `xor` on the
key derivation lookup since the KSA uses arithmetic, not XOR.

### Verify symmetry

The Rust `rc4_encrypt` and the Luau `_d` must produce the same
keystream for the same key. Add a Rust test that:

- Builds a known plaintext + 32-byte key.
- Runs `rc4_encrypt` to get ciphertext.
- Calls `luau` CLI via `std::process::Command` on a tiny test script
  that decrypts the ciphertext and prints the result.
- Asserts plaintext recovered.

This guards against KSA / PRGA / drop-count mismatch.

---

## File structure

```
crates/luau-emit/src/stage0.rs   # MODIFY: rc4_encrypt + new render_stage0 body
README.md                         # MODIFY: status
docs/superpowers/plans/2026-06-03-plan-28-stage0-stream-cipher.md  # CREATE
```

---

## Task 1: Implement RC4 + replace stage-0 cipher

- [ ] Add `rc4_encrypt` to `stage0.rs` with a unit test that
      `rc4_encrypt(rc4_encrypt(plaintext, key), key) == plaintext`
      (symmetric round-trip).
- [ ] Replace `encrypt_payload` body to call `rc4_encrypt`. Keep the
      function name and signature (callers don't change).
- [ ] Update `render_stage0` to emit the RC4 decryptor body.
- [ ] **Cross-implementation test:** add `rust_luau_rc4_agree` —
      `rc4_encrypt` a fixed plaintext under a fixed key, then write the
      ciphertext to a tempfile, write a tiny Luau script that runs the
      Luau `_d` function (extracted into a callable form), and verify
      they round-trip identically.
      *Simpler alternative:* just run the FULL pipeline (obfuscate → run
      via `luau`) for the corpus; the corpus already requires
      round-trip correctness, so any KSA/PRGA mismatch will be caught
      immediately. Pick whichever is faster.
- [ ] Run `cargo test -p luau-emit --release`.
- [ ] Run the corpus difftest — this is the real gate.

- [ ] Commit: `feat(emit): stage-0 RC4 stream cipher with drop-256`.

## Task 2: Acceptance test

- [ ] In `crates/luau-obf/src/lib.rs`, add `stage0_rc4_round_trips`
      with seeds [11, 22, 33, 44]. Each obfuscates a representative
      program (use the same pow2 program as plan 25), writes to tempfile,
      runs via `luau`, asserts expected output.
- [ ] Commit: `test: Plan 28 acceptance — RC4 stage-0 round-trips`.

## Task 3: Status

- [ ] README. Commit: `docs: mark Plan 28 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green.
- [ ] 25-program × 8-seed corpus — green.
- [ ] Stage-0 wrapper no longer contains the positional formula
      `idx = ((i - 1) % 32) + 1` and `pos = (i - 1) % 256`; instead
      contains an RC4 KSA visible in the wrapper.
- [ ] Per-slot known-plaintext attack from Round 4 fails: bucketing
      ciphertext bytes by `i mod 32` and taking the modal byte no
      longer yields a constant signal (this is by construction of RC4
      keystream, no automated test needed).
