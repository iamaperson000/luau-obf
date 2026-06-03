# Luau Obfuscator — Plan 14: Stage-0 Self-Decrypting Bootstrap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the VM dispatcher, helper functions, constant pool, and bytecode out of plain text view. After Plan 14, opening the obfuscated file shows ~30 lines of small "stage-0" bootstrap code plus one giant encrypted string. The reader cannot statically read the dispatcher, identify handler bodies, find `_KA`/`_KB`/`_k0`/`_k1`, or grep for opcode patterns without first executing the stage-0 decryption.

This is the architectural change the deobfuscation cycle has identified as the #1 priority every round since round 3.

**Architecture:**

Today's output is one self-contained Luau chunk:
```
[ aliases ][ _KA/_KB/_k0/_k1 ][ helpers ][ CONSTS table ][ CODE table ][ vm_call function ][ bootstrap call ]
```

After Plan 14:
```
[ tiny aliases + stage-0 keys ][ stage-0 decrypt helper ][ STAGE1_PAYLOAD = "<encrypted source>" ][ loadstring(decrypt(STAGE1_PAYLOAD))() ]
```

The stage-1 source is the current full output (aliases + all helpers + CONSTS/CODE/vm_call/bootstrap). Render still produces that text first; then a final emit step encrypts it under a second per-build key and emits the stage-0 wrapper around it. The stage-0 bootstrap, on execution, decrypts the payload into a Luau source string and evaluates it via `loadstring`.

The deobfuscator must:
1. Identify the stage-0 decrypt function (still readable).
2. Apply it to STAGE1_PAYLOAD to recover the stage-1 source.
3. Resume deobfuscation against the recovered source (which is what we have today).

Plan 14 doesn't make deobfuscation impossible — Luau source is fundamentally readable once you eval it. But it adds:
- An explicit barrier: pure static `grep`/`head` returns ~30 useless lines.
- A separate keying layer: stage-0 uses different keys than stage-1.
- A point where future work can layer additional obfuscation (multi-stage chains, runtime-derived stage-0 keys, integrity checks before decoding).

**Out of scope:**
- Multi-stage chains (stage-2, stage-3, …).
- Runtime-derived stage-0 keys (deriving from env vars, script name, etc.). Stage-0 keys stay as plain seed-derived constants for now.
- Anti-debug detection.
- Anti-tamper integrity hashing.

**Tech Stack:** Same as Plans 1-13.

---

## Implementation notes

**Encryption format for the payload.** The stage-1 source is a UTF-8 string of Luau code (currently ~9KB after mangling). XOR-encrypt against a 32-byte cycling key and a per-byte position mix:

```rust
fn encrypt_payload(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    for (i, b) in plaintext.iter().enumerate() {
        let k = key[i % 32];
        let pos = (i & 0xFF) as u8;
        out.push(b ^ k ^ pos);
    }
    out
}
```

Same operation on the Luau side. Self-inverse (XOR).

**Stage-0 keys are different from stage-1 keys.** The seed-derived `_KA`, `_KB`, `_k0`, `_k1` from Plans 8-12 are inside the stage-1 source. Stage-0 uses a separate 32-byte key derived from the seed BEFORE any of the stage-1 keys (so the consumption order is `[stage0_key, key_a, key_b, k0_k1, ...]`).

**Stage-0 template structure.** Roughly:
```luau
local _s = "<encrypted-bytes-as-\\NNN-escaped-string>"
local _k = "\<key byte 0>\<key byte 1>..." -- the 32-byte key as another byte-string
local function _d(s)
    local out = {}
    local kb = {}
    for i = 1, 32 do kb[i] = string.byte(_k, i) end
    for i = 1, #s do
        local b = string.byte(s, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1) % 256
        out[i] = string.char(bit32.bxor(bit32.bxor(b, kb[idx]), pos))
    end
    return table.concat(out)
end
return loadstring(_d(_s))()
```

(With Plan-9-style identifier mangling applied to `_s`, `_k`, `_d`, `out`, `kb`, `i`, `b`, `idx`, `pos`. These are all locals in the stage-0 wrapper.)

`loadstring` is a Luau global. It compiles the source string and returns a function; calling it executes the stage-1 chunk. The chunk's existing bootstrap (`vm_call(0, main_args, main_n, {})`) runs and returns the program's result.

`loadstring` returns `(function | nil, error_msg)`. For robustness, we could check the function isn't nil and error appropriately, but for obfuscation purposes a tight bootstrap is preferable. If `loadstring` fails (which it shouldn't, given encryption round-trips), the wrapper just `nil`s.

**Identifier mangling order.** Plan-9's mangler runs on the FULL rendered output. Plan-14 splits the render into two passes:
1. Stage-1 render: produce the current full output. Apply Plan-9 mangling. Apply comment stripping.
2. Encrypt the stage-1 string under the stage-0 key.
3. Render the stage-0 bootstrap template. Apply Plan-9 mangling separately to the stage-0 wrapper (it has its own locals: `_s`, `_k`, `_d`, etc.).
4. The final output is just the mangled stage-0 wrapper.

Stage-0 and stage-1 should use DIFFERENT mangling maps — so identifier `_s` in stage-0 has nothing to do with any name in stage-1. This means calling `build_name_map(rng)` twice with the same rng yields two different maps consumed serially.

Add a stage-0-specific set of `MANGLE_TARGETS`:

```rust
pub const STAGE0_MANGLE_TARGETS: &[&str] = &[
    "_s", "_k", "_d", "out", "kb", "i", "b", "idx", "pos",
];
```

(Plus any Luau builtins we use — `string`, `bit32`, `loadstring`, `table` — must NOT be in this list.)

**Bootstrap args forwarding.** The stage-1 source ends with `return vm_call(0, main_args, main_n, {})`. The stage-0 wrapper invokes the chunk via `loadstring(_d(_s))()` — but `loadstring` produces a function that takes its own varargs. To forward the outer wrapper's `...` into stage-1, the stage-0 ending becomes:

```luau
return loadstring(_d(_s))(...)
```

The `...` at the top of any Luau chunk is the script's command-line varargs. The stage-1 chunk's top-level `local main_args = { ... }` then picks them up correctly.

**Determinism.** Same seed → same stage-0 key, same payload encryption, same wrapper. The rng consumption order is fixed.

**Output size impact.** The stage-1 source is ~9KB. After XOR encryption it's the same byte count but ~3-4x more characters when rendered as `\NNN`-escaped Luau string literals (3 chars per byte). So the output grows from ~21KB to ~35-40KB. That's acceptable.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  render.rs                 # MODIFY: produce stage-1 output via existing path,
                            #         then wrap in stage-0 bootstrap;
                            #         derive stage-0 key from rng FIRST.
  stage0.rs                 # CREATE: stage-0 wrapper render + encryption
crates/luau-emit/src/
  mangle.rs                 # MODIFY: add STAGE0_MANGLE_TARGETS;
                            #         add build_stage0_name_map similar to build_name_map
crates/luau-obf/src/
  lib.rs                    # MODIFY: acceptance tests that no opcode patterns
                            #         are visible in raw output, and that running
                            #         the chunk still produces the right result
README.md                   # MODIFY: status line
```

No changes to HIR / MIR / LIR / VM template. The stage-1 chunk is identical to what we produce today.

---

## Task 1: Stage-0 wrapper module

**Files:**
- Create: `crates/luau-emit/src/stage0.rs`
- Modify: `crates/luau-emit/src/lib.rs`

- [ ] **Step 1: Create the new module**

Write `/Volumes/External Drive/luau-obf/crates/luau-emit/src/stage0.rs`:

```rust
//! Stage-0 self-decrypting bootstrap. Wraps a stage-1 Luau source string in
//! a small wrapper that decrypts and `loadstring`-evaluates it at runtime.
//!
//! After this wrapper is rendered and mangled, the obfuscated output's
//! top-level structure is unrecognizable — only the wrapper's locals,
//! a giant encrypted byte-string literal, and a `loadstring()` invocation
//! are visible. The dispatcher, helpers, opcode constants, constant pool,
//! and bytecode all live inside the encrypted payload.

use rand::SeedableRng;
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
```

- [ ] **Step 2: Expose the module**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/lib.rs`, add `pub mod stage0;` alongside the existing `pub mod` declarations.

- [ ] **Step 3: Make `encode_luau_string_literal` pub**

The stage0 module needs `crate::render::encode_luau_string_literal`. If it's currently private (`fn`), change to `pub(crate) fn` in `render.rs`.

- [ ] **Step 4: Run the tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-emit`

Expected: 3 new tests pass. No changes to the workspace's other tests yet (the wrapper isn't invoked from render).

- [ ] **Step 5: Commit**

```bash
git add crates/luau-emit/src/stage0.rs crates/luau-emit/src/lib.rs crates/luau-emit/src/render.rs
git commit -m "feat(emit): stage-0 self-decrypting wrapper module"
```

---

## Task 2: Wire stage-0 into the render pipeline

**Files:**
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-emit/src/mangle.rs`

- [ ] **Step 1: Define `STAGE0_MANGLE_TARGETS` and a helper**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/mangle.rs`, add:

```rust
pub const STAGE0_MANGLE_TARGETS: &[&str] = &[
    "_s", "_k", "_d", "out", "kb", "i", "b", "idx", "pos",
];

pub fn build_stage0_name_map(rng: &mut rand_chacha::ChaCha20Rng) -> HashMap<String, String> {
    use rand::seq::SliceRandom;
    let alphabet: Vec<char> = ('a'..='z').collect();
    let mut suffixes: Vec<String> = Vec::with_capacity(26 * 26);
    for a in &alphabet {
        for b in &alphabet {
            suffixes.push(format!("_{}{}", a, b));
        }
    }
    suffixes.shuffle(rng);
    let mut map: HashMap<String, String> = HashMap::new();
    for (i, name) in STAGE0_MANGLE_TARGETS.iter().enumerate() {
        map.insert((*name).to_string(), suffixes[i].clone());
    }
    map
}
```

(The function is nearly identical to `build_name_map` — keeping them separate ensures the stage-0 and stage-1 maps are independent.)

- [ ] **Step 2: Restructure `render` to emit stage-0 wrapping stage-1**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, the existing `render` function returns the stage-1 source directly. Modify it to:

1. Derive the stage-0 key FIRST (consumes rng bytes before any stage-1 derivation).
2. Build stage-1 as before, applying comment-strip and stage-1 mangling.
3. Encrypt stage-1 under the stage-0 key.
4. Render the stage-0 wrapper.
5. Apply stage-0 mangling + comment-strip to the wrapper.
6. Return the mangled wrapper.

Concretely:

```rust
pub fn render(
    program: &LirProgram,
    opmap: &OpMap,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    // Stage-0 key BEFORE any other rng consumption that affects stage-1.
    let stage0_key = crate::stage0::derive_stage0_key(rng);

    // ... existing render body produces `stage1_source` (the full Luau chunk we
    // used to return directly). Take the existing post-template-render path
    // through comment-stripping and identifier mangling, but assign the result
    // to a local instead of returning it.

    let stage1_source = {
        // ... existing logic verbatim, except:
        // - The output of the final `mangle_identifiers` call is `stage1_source`
        //   rather than the function's return value.
        let raw_output = tmpl.render(minijinja::context! {
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
        let stripped = crate::mangle::strip_comments(&raw_output);
        let name_map = crate::mangle::build_name_map(rng);
        crate::mangle::mangle_identifiers(&stripped, &name_map)
    };

    // Encrypt and wrap.
    let encrypted = crate::stage0::encrypt_payload(stage1_source.as_bytes(), &stage0_key);
    let stage0_text = crate::stage0::render_stage0(&encrypted, &stage0_key);

    // Mangle the stage-0 wrapper's identifiers.
    let stage0_stripped = crate::mangle::strip_comments(&stage0_text);
    let stage0_map = crate::mangle::build_stage0_name_map(rng);
    let stage0_mangled = crate::mangle::mangle_identifiers(&stage0_stripped, &stage0_map);

    Ok(stage0_mangled)
}
```

Note: The rng consumption order is now `[stage0_key, …all stage-1 stuff…, stage1_name_map, stage0_name_map]`. Same seed reliably reproduces output.

- [ ] **Step 3: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all unit tests pass. The differential corpus should also pass — the obfuscated output is now a stage-0 wrapper, but executing it via `luau` decrypts stage-1, evaluates, and runs the program. The observable output is identical to plain run.

If a corpus test fails:
- **#1 cause**: `loadstring` may not accept the decrypted source for some Luau host. Spot-check by running one obfuscated program manually — `cargo run -p luau-obf-cli --release -- runtime-tests/corpus/arithmetic.luau -o /tmp/out.luau --seed 0000... --quiet && luau /tmp/out.luau`. Compare to running the plain `.luau`.
- **#2 cause**: Stage-0 mangling may rename something inside a Luau string literal it shouldn't have. The mangler's string-literal detection should prevent this — verify by checking the generated output for any clearly-corrupted string content.
- **#3 cause**: `loadstring(...)(...)` doesn't forward varargs correctly — verify the stage-0 template ends with `return loadstring(_d(_s))(...)`. The trailing `(...)` forwards the outer chunk's varargs.
- **#4 cause**: The stage-1 source contains a `"` inside a long-bracket comment or a Lua string that needs escaping when written into a Luau byte-string literal. The `encode_luau_string_literal` function handles this — verify by checking the encrypted byte literal in the output.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit/src/render.rs crates/luau-emit/src/mangle.rs
git commit -m "feat(obf): wrap stage-1 output in a stage-0 self-decrypting bootstrap"
```

---

## Task 3: Acceptance tests

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add acceptance tests for the wrapper**

Append to the `tests` mod:

```rust
    #[test]
    fn stage0_wrapper_present() {
        let r = obfuscate("print(1)", Options { seed: Some([121u8; 32]) }).unwrap();
        // The wrapper invokes `loadstring(...)`. Even after mangling, the
        // `loadstring` global stays unmangled (it's a Luau builtin).
        assert!(r.output.contains("loadstring"));
    }

    #[test]
    fn dispatcher_keywords_not_in_raw_output() {
        // The stage-1 source contains the dispatcher pattern `elseif op == N then`.
        // After encryption, no such pattern should appear in the raw output.
        let r = obfuscate("print(1 + 2)", Options { seed: Some([122u8; 32]) }).unwrap();
        // After Plan-14 wrapping, the stage-1 source is encrypted. The string
        // "elseif" should appear at most once (inside our stage-0 wrapper — but
        // actually the wrapper has no `elseif`; only `if/then`). So count == 0.
        let elseif_count = r.output.matches("elseif").count();
        assert!(elseif_count <= 1,
            "expected at most 1 'elseif' (in unlikely stage-0 use), found {}", elseif_count);
    }

    #[test]
    fn opcode_dispatch_pattern_not_in_raw_output() {
        // Pre-Plan-14, the output had `elseif _xy == 14 then` and similar
        // arms. Post-Plan-14, no such patterns should be visible in raw text.
        let r = obfuscate("local a = 1 + 2 print(a)",
                          Options { seed: Some([123u8; 32]) }).unwrap();
        // Search for any `== <integer>` pattern that looks like a dispatcher
        // arm. After stage-0 wrapping, none should appear in raw text.
        let bytes = r.output.as_bytes();
        let mut arm_pattern_count = 0;
        let mut i = 0;
        while i + 8 < bytes.len() {
            // Look for `== \d+ then` or `op == \d+`.
            if &bytes[i..i + 3] == b"== " {
                let mut j = i + 3;
                while j < bytes.len() && bytes[j].is_ascii_digit() { j += 1; }
                if j > i + 3 && j + 5 < bytes.len() && &bytes[j..j + 5] == b" then" {
                    arm_pattern_count += 1;
                }
            }
            i += 1;
        }
        assert_eq!(arm_pattern_count, 0,
            "found {} dispatcher arm patterns in raw output", arm_pattern_count);
    }

    #[test]
    fn output_runs_correctly_through_loadstring() {
        // Sanity: the wrapped output executes and produces the same value
        // as the plain source. The differential corpus harness exercises
        // this for many programs; here we just confirm one simple case.
        let src = "print(42)";
        let r = obfuscate(src, Options { seed: Some([124u8; 32]) }).unwrap();
        // Write the obfuscated chunk and execute it; capture stdout.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(), "luau failed: {:?}", out);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("42"), "expected 42 in output, got: {stdout}");
    }
```

(`tempfile` is already a dev-dep based on the existing CLI tests; use the existing convention.)

- [ ] **Step 2: Run the tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all four new tests pass, including the new `output_runs_correctly_through_loadstring`.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 14 acceptance — dispatcher hidden behind stage-0 bootstrap"
```

---

## Task 4: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 14 — the entire VM (dispatcher, helpers, constant pool, bytecode)
is now wrapped in a stage-0 self-decrypting bootstrap. The raw output file
contains only a small XOR decryption helper, a 32-byte key, an encrypted
payload string, and a `loadstring(decrypt(payload))(...)` invocation. A
reader cannot statically see the dispatcher, opcode handlers, encryption
constants, or bytecode without first executing the stage-0 decryption.
Plans 1–13 features unchanged; full corpus passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 14 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All differential corpus tests pass — the stage-0 wrapper transparently delivers stage-1's runtime behavior.
- [ ] `stage0_wrapper_present` passes — `loadstring` appears in the raw output.
- [ ] `dispatcher_keywords_not_in_raw_output` passes — `elseif` count is at most 1.
- [ ] `opcode_dispatch_pattern_not_in_raw_output` passes — no `== N then` patterns visible.
- [ ] `output_runs_correctly_through_loadstring` passes — executed output matches plain source's behavior.
- [ ] All Plan 8-13 acceptance tests still pass.
- [ ] Spot-check: head of an obfuscated file shows ~10 lines of stage-0 wrapper followed by a giant encrypted byte string. No `_KA`, no `vm_call`, no `OP_*` (already gone), no dispatcher.
