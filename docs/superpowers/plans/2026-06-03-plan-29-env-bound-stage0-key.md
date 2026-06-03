# Luau Obfuscator — Plan 29: Env-bound stage-0 key

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Round 5's top-impact fix: bind the stage-0 decryption key to a
runtime environment value so the obfuscated blob can't be decrypted by
just running the wrapper offline (e.g., in `luau` on the analyst's
laptop). On Roblox the binding value is `game.PlaceId`; in other
embedders it's a configurable lookup expression.

**Constraint.** The `runtime-tests/` corpus harness runs the obfuscated
output via the `luau` CLI (no Roblox). So the binding **must be
optional** with a default of "no binding" — otherwise the test suite
breaks. We add the plumbing now; users who want Roblox binding set the
option.

---

## Design

### Public option

In `crates/luau-obf/src/lib.rs`:

```rust
pub struct Options {
    pub seed: Option<[u8; 32]>,
    /// Optional runtime expression whose string representation is mixed
    /// into the stage-0 key at decryption time. Off by default.
    /// Example: `Some("tostring(game.PlaceId)".into())` for Roblox.
    pub env_binding: Option<String>,
}
```

(Add to existing `Options`, with `#[derive(Default)]` or explicit
`Default` impl returning `env_binding: None`.)

### How binding works

The stage-0 wrapper currently has:

```
local _k = "<32 bytes literally>"
```

With binding enabled, the wrapper instead has:

```
local _kbase = "<32 bytes literally>"
local _bind = <user expression>     -- e.g. tostring(game.PlaceId)
local _k = _mix(_kbase, _bind)
```

Where `_mix(base, bind)` produces the actual 32-byte key by XOR-folding
`bind` into `base`:

```luau
local function _mix(base, bind)
    local kb = {}
    for i = 1, 32 do kb[i] = string.byte(base, i) end
    if bind ~= nil then
        local bs = tostring(bind)
        for i = 1, #bs do
            local k_idx = ((i - 1) % 32) + 1
            kb[k_idx] = bit32.bxor(kb[k_idx], string.byte(bs, i))
        end
    end
    local out = {}
    for i = 1, 32 do out[i] = string.char(kb[i]) end
    return table.concat(out)
end
```

### Rust-side encryption

The build-time encryption must know the `bind` string too — otherwise
the ciphertext can't be decrypted on the target. So:

- If `env_binding` is `Some(expr)`, the obfuscator ALSO needs a literal
  *expected bind value* to bake into the cipher. Add:

  ```rust
  pub struct Options {
      pub seed: Option<[u8; 32]>,
      pub env_binding: Option<EnvBinding>,
  }
  pub struct EnvBinding {
      /// Luau expression evaluated at runtime, e.g. "tostring(game.PlaceId)".
      pub runtime_expr: String,
      /// Expected value of that expression at runtime (a string). Used at
      /// BUILD TIME to derive the actual stage-0 key. If the runtime
      /// expression does not match this string, decryption FAILS.
      pub expected_value: String,
  }
  ```

This makes the binding a *commitment* — the build promises "I will only
decrypt on a host where the runtime_expr evaluates to expected_value".
For Roblox: `runtime_expr = "tostring(game.PlaceId)"`,
`expected_value = "12345678"`. On any other host, the XOR-folded key is
different, and decryption produces gibberish that `loadstring` rejects.

### Build-time key derivation

After `derive_stage0_key(rng)` produces the base 32 bytes, if binding is
on, fold the expected_value into the base the same way the Luau `_mix`
does, then use the *folded* key to encrypt. The wrapper still embeds the
base bytes literally (those are what `_kbase` holds); only at runtime
does `_mix` reconstruct the folded key by XOR'ing in the actual runtime
value. If `actual == expected`, the folded key matches the build-time
folded key → decryption succeeds. Otherwise: gibberish.

---

## Implementation

### `crates/luau-obf/src/lib.rs`

- Add `EnvBinding` struct (or `pub struct` with two `pub` fields).
- Extend `Options` with `env_binding: Option<EnvBinding>`.
- Thread `env_binding` through to `luau_emit::emit`. The cleanest path:
  change `emit` signature to accept the binding. (`emit` currently only
  takes `(lir, rng)` — add `binding: Option<&EnvBinding>`.)

### `crates/luau-emit/src/lib.rs` + `render.rs` + `stage0.rs`

- Plumb `Option<&EnvBinding>` to `render`.
- In `render`, pass to `stage0::render_stage0`.
- In `stage0`, when binding is present:
  - Compute `folded_key = fold(stage0_key, expected_value.as_bytes())`
    where fold = XOR-into 32-byte slots cyclically (matches Luau `_mix`).
  - Encrypt with `folded_key`.
  - Emit wrapper with `_kbase = base_bytes`, `_bind = <runtime_expr>`,
    `_k = _mix(_kbase, _bind)`.
- When binding is `None`: existing behavior. (No `_mix`, `_k` is just
  the base bytes.)

**Crucial:** `runtime_expr` is inserted **as raw Luau code**, NOT as a
string literal. It's an expression. The build is trusting the caller's
input here; document the security implication.

### Mangling

Identifier mangling currently rewrites variable names in stage-0. The
new locals (`_kbase`, `_bind`, `_mix`) need to participate in mangling
the same way `_k`, `_d`, `_s` do. Inspect `crates/luau-emit/src/mangle.rs`
(esp. `build_stage0_name_map`) and add the new identifiers to the set.

---

## File structure

```
crates/luau-obf/src/lib.rs               # MODIFY: Options + EnvBinding
crates/luau-emit/src/lib.rs              # MODIFY: emit signature
crates/luau-emit/src/render.rs           # MODIFY: thread binding
crates/luau-emit/src/stage0.rs           # MODIFY: render_stage0 binding path
crates/luau-emit/src/mangle.rs           # MODIFY: name map adds new idents
crates/luau-obf-cli/src/main.rs          # OPTIONAL: --env-binding-expr / --env-binding-expected CLI flags
README.md                                # MODIFY: status
docs/superpowers/plans/2026-06-03-plan-29-env-bound-stage0-key.md  # CREATE
```

---

## Task 1: Implement env-binding plumbing

- [ ] Define `EnvBinding` and extend `Options`.
- [ ] Update `emit` signature; thread binding through `render`.
- [ ] Implement `_mix` in the Luau template; implement the equivalent
      `fold` helper in Rust `stage0.rs`. Add a unit test that asserts
      the Rust fold and the Luau `_mix` are bit-identical (run `luau`
      via Command).
- [ ] Update mangle name map.
- [ ] **Default behavior unchanged.** All existing tests pass without
      modification.

- [ ] Commit: `feat(emit): optional env-bound stage-0 key`.

## Task 2: Acceptance tests

- [ ] `env_binding_matches_decrypts` — obfuscate with binding
      `runtime_expr = "\"ABC\""`, `expected_value = "ABC"`. The
      runtime_expr evaluates to literal "ABC", so decryption succeeds.
      Run via `luau`, assert expected output.
- [ ] `env_binding_mismatch_fails` — obfuscate with binding
      `runtime_expr = "\"WRONG\""`, `expected_value = "ABC"`. Run via
      `luau`. Expect non-zero exit OR garbled output (loadstring should
      fail on garbled bytes). Assert the test program's expected stdout
      is NOT produced.
- [ ] `env_binding_disabled_default` — existing pipeline (binding None)
      produces an identical output structure as before this plan (no
      `_mix`/`_kbase`/`_bind` strings).

- [ ] Commit: `test: Plan 29 acceptance — env-binding match/mismatch`.

## Task 3: Optional CLI flag

- [ ] Add `--env-bind-expr <expr>` and `--env-bind-expected <value>`
      flags to the CLI. If only one is given, error out.
- [ ] Commit: `feat(cli): --env-bind-expr / --env-bind-expected flags`.

## Task 4: Status

- [ ] README. Commit: `docs: mark Plan 29 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green (default binding off).
- [ ] 25-program corpus — green (binding off path unchanged).
- [ ] With binding on (`runtime_expr = "\"ABC\"", expected = "ABC"`),
      pow2(10) program prints 1024.
- [ ] With binding mismatch, program does NOT print 1024.
- [ ] Mangled output uses new identifiers; no `_kbase` / `_bind` /
      `_mix` raw names visible in mangled stage-0.
