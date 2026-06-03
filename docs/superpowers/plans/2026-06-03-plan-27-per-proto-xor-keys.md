# Luau Obfuscator — Plan 27: Per-proto constant-pool XOR keys

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Currently every proto's constant pool is encrypted with the same
two 32-byte keys `(key_a, key_b)`; only the `proto_salt` differs. The Round 4
adversary recovered the shared key offline by bucketing ciphertext bytes by
`i mod 32` across all protos and exploiting the high backslash density of
stage-1. Round 5 named per-proto distinct XOR keys as the cheap defense.

After this plan: each proto P gets its own `(key_a_P, key_b_P)` pair,
derived deterministically from the build-time RNG. The adversary loses
the cross-proto bucketing; per-proto buckets are 2-4x smaller and the
known-plaintext attack's signal-to-noise collapses for short protos.

---

## Implementation

### Rust side: `crates/luau-emit/src/render.rs`

- `derive_string_keys` already returns one `(key_a, key_b)` pair. **Add a
  new function:**

  ```rust
  pub(crate) fn derive_per_proto_string_keys(
      rng: &mut ChaCha20Rng,
      n_protos: usize,
  ) -> Vec<([u8; 32], [u8; 32])>;
  ```

  Implementation: in a loop, `rng.fill_bytes(&mut a); rng.fill_bytes(&mut b);`
  push. This consumes `64 * n_protos` bytes from the RNG — that's the
  cost. Determinism preserved as long as it's called at the same point
  in the schedule.

- Replace the single `derive_string_keys` call at `render.rs:96` with the
  new function. Order: it must come at the same RNG offset for the
  legacy 1-proto case to be reproducible? No — the RNG schedule will
  change unconditionally; that's fine, the corpus tests don't assert
  cross-version determinism, only within-build determinism.

- Update `format_const_pool` to take the per-proto `(key_a, key_b)` for
  the proto being rendered. Iterate over the keys vector by index.

- `encrypt_bytes` signature stays the same; called with the per-proto
  keys.

### Template side: `crates/luau-runtime/assets/vm.luau.j2`

- Replace the single `local _KA = { {{ key_a }} }` and
  `local _KB = { {{ key_b }} }` with per-proto arrays-of-arrays:

  ```jinja
  local _KAS = {
  {% for k in keys_a %}    { {{ k }} },
  {% endfor %}}
  local _KBS = {
  {% for k in keys_b %}    { {{ k }} },
  {% endfor %}}
  ```

- Modify `_decrypt(enc, salt)` (line 36) to receive a proto_id and look
  up the right keys. The salt parameter at the call site is already the
  proto_id (see `_const` at line 53: `_decrypt(c._e, proto_id)`). Rename
  for clarity:

  ```luau
  local function _decrypt(enc, proto_id)
      local ka = _KAS[proto_id + 1]
      local kb = _KBS[proto_id + 1]
      local out = {}
      local salt_term = (proto_id * 7919) % 4294967296
      for i = 1, #enc do
          local b = _sbyte(enc, i)
          local idx = ((i - 1) % 32) + 1
          local pos = ((i - 1) + salt_term) % 256
          out[i] = _schar(_xor(_xor(_xor(b, ka[idx]), pos), kb[idx]))
      end
      return _tconcat(out)
  end
  ```

  Keep the salt_term mixing — extra entropy is free. The signature of
  `_decrypt(enc, salt)` shrinks to `_decrypt(enc, proto_id)` with the
  same value passed in. **Do not rename callers** unless they're already
  passing `proto_id`; they are (single caller: line 53).

### Context passed to template (render.rs)

```rust
keys_a => keys_a_lits,  // Vec<String>, each is comma-joined byte list
keys_b => keys_b_lits,
// `key_a`, `key_b` are removed from the context.
```

---

## File structure

```
crates/luau-emit/src/render.rs                 # MODIFY: derive per-proto, thread through
crates/luau-runtime/assets/vm.luau.j2          # MODIFY: _KAS/_KBS arrays-of-arrays
README.md                                       # MODIFY: status
docs/superpowers/plans/2026-06-03-plan-27-per-proto-xor-keys.md  # CREATE
```

---

## Task 1: Implement per-proto key derivation and threading

- [ ] Add `derive_per_proto_string_keys` in `render.rs`.
- [ ] Update the orchestration in `render()` (around line 96).
- [ ] Update `format_const_pool` / `format_const` signatures to take a
      `(&[u8; 32], &[u8; 32])` pair rather than two shared refs.
- [ ] Update the template: replace `{{ key_a }} / {{ key_b }}` with
      `{% for %}` blocks over `keys_a` / `keys_b`.
- [ ] Adjust `_decrypt` to index by proto_id.

- [ ] **Tests:**
  - Existing `encrypt_decrypt_round_trips` test: keep, but parameterize
    over `(key_a, key_b)` independent of proto count.
  - Add `different_protos_have_different_keys` — derive 4 protos with
    fixed seed, assert no two pairs equal.
  - Add `same_seed_same_keys` — same seed twice → identical per-proto
    keys.

- [ ] Run `cargo test -p luau-emit --release` + corpus harness.
- [ ] Commit: `feat(emit): per-proto constant-pool XOR keys`.

## Task 2: Acceptance test

- [ ] In `crates/luau-obf/src/lib.rs` add `per_proto_keys_changes_pool_bytes`:
      obfuscate the same source with two different seeds, decode the
      stage-0 wrapper, find the `_KAS` table line, assert the two builds
      have different key bytes for proto 1.

  (Optional shortcut: just assert that obfuscated outputs differ for
  per-proto-key-only seed variation. The cleaner version requires
  parsing the stage-0 wrapper which is brittle. Just go with full
  workspace tests + the existing seed-variance acceptance pattern.)

- [ ] Commit: `test: Plan 27 acceptance — per-proto keys, semantics
      preserved`.

## Task 3: Status bump

- [ ] README. Commit: `docs: mark Plan 27 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green.
- [ ] 25-program corpus harness — green for 8 seeds each.
- [ ] Stage-0 output literal contains TWO `_KAS = {` / `_KBS = {`
      definitions (one each, with N inner arrays for N protos).
- [ ] Constant-pool decryption still works (no garbled strings;
      `bank.luau` runs and prints expected output).
