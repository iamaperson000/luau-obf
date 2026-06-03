# Luau Obfuscator — Plan 8: Constant Pool Encryption

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the biggest static-inspection weakness in the current obfuscator: string constants are emitted in plaintext, so opening the output file in any text editor reveals every global name, every method name, every string literal. After Plan 8, all string constants are stored encrypted and decrypted lazily inside the VM. Numbers, booleans, and nil stay as-is — they have no semantic value to hide.

**Architecture:**
- The emit stage gains two seed-derived 32-byte keys, `key_a` and `key_b`. Each string constant goes through three layers — XOR with `key_a` (cycled), XOR with position byte, XOR with `key_b` (cycled) — before rendering. The transformation is fully invertible and self-inverse (each layer is XOR).
- Encrypted strings render in the output as `_enc("<raw-encrypted-bytes>")` instead of `"<plaintext>"`. The VM template defines `_enc(bytes)` as a tag function returning `{ enc = bytes }` — a marker table.
- The VM gains a `_const(consts, idx)` helper. When the slot at `idx` is a marker table, `_const` decrypts the bytes, replaces the slot in place (lazy cache), and returns the plaintext. When the slot is anything else (number, bool, nil), it's returned as-is.
- Every `consts[X]` read in the VM template is replaced with `_const(consts, X)`. After this plan the bytecode-format remains unchanged — only the constant-pool storage format and the VM read path differ.

**Why this design over alternatives:**
- *Eager decryption at startup* would defeat the static-inspection goal less efficiently: a memory dump after startup would reveal everything. Lazy decryption means strings only materialize when actually used.
- *Per-string unique keys* would inflate the output. A shared keyed-XOR with position mixing is plenty for the static-inspection threat model. The cycle (see Plan 11+) will tell us if we need stronger encryption.
- *Wrapping in `{ enc = bytes }`* tables is verbose but type-discriminable. Plain-string sentinel approaches conflict with arbitrary user string data.

**Tech Stack:** Same as Plans 1-7.

**Plan 8 additions:**

| Newly hidden | Still visible |
|---|---|
| All `Constant::String` values in every function's constant pool | Numeric constants — these are usually structural (loop bounds, array indices), not semantically interesting; revisit if the cycle identifies them as weak |
| Global names (`print`, `setmetatable`, etc.) — they pass through the same constant pool | The VM template's structure — `OP_*` constants, handler bodies, function names like `vm_call`, `read_u16`. Plan 9 handles this. |
| User string literals (`"hello"`, error messages, …) | Bytecode bytes — still plaintext bytes in a Luau string literal. Plan 10 (XOR bytecode encoding) addresses this. |

---

## Implementation notes

**Encryption algorithm (Rust side, deterministic from seed):**

```rust
fn derive_keys(rng: &mut ChaCha20Rng) -> ([u8; 32], [u8; 32]) {
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    rng.fill_bytes(&mut a);
    rng.fill_bytes(&mut b);
    (a, b)
}

fn encrypt_string(plaintext: &str, key_a: &[u8; 32], key_b: &[u8; 32]) -> Vec<u8> {
    let bytes = plaintext.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = (i & 0xFF) as u8;
        // Round 1: XOR with key_a. Round 2: XOR with position. Round 3: XOR with key_b.
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}
```

The three "layers" reduce to a single XOR because XOR is associative — that's fine. The point is that key_a and key_b are independent of position-dependent mixing, so the resulting per-byte mask varies along the string. A naive XOR-with-constant-key attack fails because the position term shifts the effective key every byte.

**Decryption (Luau side, matches the Rust side):**

Same operation — XOR is self-inverse. The VM template renders the two keys as Luau byte arrays:

```luau
local _KA = { 0x__, 0x__, … }  -- 32 bytes
local _KB = { 0x__, 0x__, … }  -- 32 bytes

local function _decrypt(enc)
    local out = {}
    for i = 1, #enc do
        local b = _sbyte(enc, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1) % 256
        out[i] = _schar(_xor(_xor(_xor(b, _KA[idx]), pos), _KB[idx]))
    end
    return _tconcat(out)
end
```

Where `_xor`, `_sbyte`, `_schar`, `_tconcat` are local aliases for `bit32.bxor`, `string.byte`, `string.char`, and `table.concat` — declared next to the existing aliases at the top of the template.

**Marker function:**

```luau
local function _enc(bytes) return { _e = bytes } end
```

The marker key `_e` is intentionally short — every string constant materializes one of these, so saving bytes matters. (We could randomize the marker key name per build later; for now, fixed.)

**Const-read helper:**

```luau
local function _const(consts, idx)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        c = _decrypt(c._e)
        consts[idx] = c
    end
    return c
end
```

Caches the plaintext back into the slot on first read.

**Every `consts[b + 1]` becomes `_const(consts, b + 1)`.** Search `vm.luau.j2` for `consts[`. Today there are three such references (`OP_LoadConst`, `OP_GetGlobal`, `OP_SetGlobal`). Each is replaced.

**Variance test (new):** Same source, two different seeds → at least one of the rendered strings in the output is different (because the encryption keys differ).

**Determinism test:** Same source + same seed → byte-identical output. (Already exists for the overall pipeline; trivially preserved here.)

---

## File Structure (changes only)

```
crates/luau-emit/src/
  render.rs              # MODIFY: derive keys, transform Constant::String, emit _enc(...) and _KA/_KB tables
crates/luau-runtime/assets/
  vm.luau.j2             # MODIFY: add _xor/_tconcat aliases, _decrypt helper, _const helper,
                         #         _KA/_KB renderings, replace consts[X] → _const(consts, X)
crates/luau-obf/src/
  lib.rs                 # MODIFY: add variance test asserting different seeds produce different string-shape outputs
README.md                # MODIFY: status line
```

No HIR/MIR/LIR changes. No new tests dir.

---

## Task 1: Encryption primitives in render.rs

**Files:**
- Modify: `crates/luau-emit/src/render.rs`

This task does NOT change the VM template yet — that's Task 2. After Task 1, the workspace builds and tests pass, but the rendered constants are still plaintext (we haven't toggled the new code path). Task 2 wires it on.

- [ ] **Step 1: Add the encryption helpers**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, append two helpers near `format_const_pool`:

```rust
pub(crate) fn derive_string_keys(rng: &mut ChaCha20Rng) -> ([u8; 32], [u8; 32]) {
    use rand::RngCore;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    rng.fill_bytes(&mut a);
    rng.fill_bytes(&mut b);
    (a, b)
}

pub(crate) fn encrypt_string(plaintext: &str, key_a: &[u8; 32], key_b: &[u8; 32]) -> Vec<u8> {
    let bytes = plaintext.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = (i & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}

pub(crate) fn format_byte_array_literal(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
    parts.join(", ")
}
```

- [ ] **Step 2: Add a unit test for round-trip**

In `render.rs`, append a `#[cfg(test)] mod` if there isn't one, with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trips() {
        let key_a = [42u8; 32];
        let key_b = [99u8; 32];
        let plaintext = "print";
        let enc = encrypt_string(plaintext, &key_a, &key_b);
        // Decrypt by re-applying the same XOR chain.
        let dec: Vec<u8> = enc.iter().enumerate().map(|(i, b)| {
            let ka = key_a[i % 32];
            let kb = key_b[i % 32];
            let pos = (i & 0xFF) as u8;
            b ^ ka ^ pos ^ kb
        }).collect();
        assert_eq!(dec, plaintext.as_bytes());
    }

    #[test]
    fn encrypt_changes_bytes_for_typical_input() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        let enc = encrypt_string("hello world", &key_a, &key_b);
        assert_ne!(enc, b"hello world");
    }

    #[test]
    fn empty_string_encrypts_to_empty() {
        let enc = encrypt_string("", &[0u8; 32], &[0u8; 32]);
        assert!(enc.is_empty());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass. The new helpers are unused outside the test mod, so the rest of the workspace is unchanged.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit/src/render.rs
git commit -m "feat(emit): string-constant encryption primitives"
```

---

## Task 2: Wire encryption into render, plumb keys to the template

**Files:**
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Plumb the keys through the render entrypoint**

Replace the body of `render` in `render.rs` so it derives keys at the top, threads them into `format_const_pool`, and passes byte-array literals into the template context:

```rust
pub fn render(
    program: &LirProgram,
    opmap: &OpMap,
    rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    let opcodes: Vec<(String, u8)> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();

    let (key_a, key_b) = derive_string_keys(rng);

    let consts: Vec<String> = program
        .functions
        .iter()
        .map(|f| format_const_pool(&f.consts, &key_a, &key_b))
        .collect();

    let codes: Vec<String> = program
        .functions
        .iter()
        .map(|f| {
            let bytes = encode_function(f, opmap);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();

    let meta: Vec<MetaEntry> = program
        .functions
        .iter()
        .map(|f| MetaEntry {
            num_params: f.num_params,
            num_regs: std::cmp::max(f.num_regs, f.num_params),
            num_upvals: f.num_upvals,
            is_vararg: if f.is_vararg { 1 } else { 0 },
        })
        .collect();

    let key_a_lit = format_byte_array_literal(&key_a);
    let key_b_lit = format_byte_array_literal(&key_b);

    let mut env = Environment::new();
    env.add_template("vm", luau_runtime::VM_TEMPLATE)
        .map_err(|e| EmitError::Template(e.to_string()))?;
    let tmpl = env.get_template("vm").unwrap();
    tmpl.render(minijinja::context! {
        opcodes => opcodes,
        consts => consts,
        codes => codes,
        meta => meta,
        key_a => key_a_lit,
        key_b => key_b_lit,
    })
    .map_err(|e| EmitError::Template(e.to_string()))
}
```

- [ ] **Step 2: Update `format_const_pool` and `format_const` signatures**

Change them to accept the keys and use the encryption path for strings:

```rust
fn format_const_pool(consts: &[Constant], key_a: &[u8; 32], key_b: &[u8; 32]) -> String {
    let parts: Vec<String> = consts.iter().map(|c| format_const(c, key_a, key_b)).collect();
    parts.join(", ")
}

fn format_const(c: &Constant, key_a: &[u8; 32], key_b: &[u8; 32]) -> String {
    match c {
        Constant::Nil => "nil".into(),
        Constant::Bool(true) => "true".into(),
        Constant::Bool(false) => "false".into(),
        Constant::Number(n) => {
            if n.is_nan() { "(0/0)".into() }
            else if n.is_infinite() && *n > 0.0 { "(1/0)".into() }
            else if n.is_infinite() { "(-1/0)".into() }
            else { format!("{}", n) }
        }
        Constant::String(s) => {
            let enc = encrypt_string(s, key_a, key_b);
            format!("_enc(\"{}\")", encode_luau_string_literal(&enc))
        }
    }
}
```

- [ ] **Step 3: Run tests, expect failures**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: the workspace's unit tests pass, but the differential corpus tests FAIL — the rendered output now contains `_enc(...)` calls referring to a function the VM doesn't define yet. That's correct; Task 3 wires the VM side.

If a *compile-time* error appears (not a runtime corpus failure), fix it. If only the corpus differential tests are red, proceed to Task 3 without committing yet.

If the corpus tests are GREEN (unexpected), something is wrong — STOP and report.

- [ ] **Step 4: Do NOT commit yet**

Tasks 2 and 3 are tightly coupled; their commit lands together at the end of Task 3 to keep history bisectable to a working state.

---

## Task 3: VM-side decryption + replace `consts[X]` reads

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Add the new local aliases at the top of the template**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find the block of `local _foo = …` declarations near the top (just after `local _ENV_ = ...`). Add these aliases right after `local _floor = math.floor`:

```luau
local _xor = bit32.bxor
local _tconcat = table.concat
```

- [ ] **Step 2: Render the keys**

After the `_tpack` declaration block, add:

```luau
local _KA = { {{ key_a }} }
local _KB = { {{ key_b }} }

local function _decrypt(enc)
    local out = {}
    for i = 1, #enc do
        local b = _sbyte(enc, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1) % 256
        out[i] = _schar(_xor(_xor(_xor(b, _KA[idx]), pos), _KB[idx]))
    end
    return _tconcat(out)
end

local function _enc(bytes) return { _e = bytes } end

local function _const(consts, idx)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        c = _decrypt(c._e)
        consts[idx] = c
    end
    return c
end
```

- [ ] **Step 3: Replace `consts[…]` reads with `_const(consts, …)`**

Search the template for `consts[`. Today (just after Plan 7) there are three reads:

```luau
elseif op == OP_LoadConst then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    regs[a] = consts[b + 1]
```

becomes:

```luau
elseif op == OP_LoadConst then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    regs[a] = _const(consts, b + 1)
```

Same substitution for `OP_GetGlobal` and `OP_SetGlobal`:

```luau
elseif op == OP_GetGlobal then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    regs[a] = _ENV_[_const(consts, b + 1)]
elseif op == OP_SetGlobal then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    _ENV_[_const(consts, a + 1)] = regs[b]
```

Verify with grep that no `consts[` references remain.

- [ ] **Step 4: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: every test passes, including all 23 differential corpus programs. Strings are now stored encrypted in the output but decrypt to plaintext at first use.

If a specific corpus program fails, the most likely causes are:
- Forgot to update one of the three `consts[X]` reads.
- Off-by-one in the position mixing (`i - 1` vs `i`) between Rust encryption and Luau decryption — they must agree on 0-based vs 1-based indexing.
- The `_enc("...")` literal has an escape-encoding mismatch with `encode_luau_string_literal`. Spot-check the rendered output: `grep _enc <generated-file>` and ensure the inner string looks like `\NNN`-escaped bytes.

If a failure persists, paste the divergence and report BLOCKED.

- [ ] **Step 5: Commit (Task 2 + Task 3 combined)**

```bash
git add crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2
git commit -m "feat(obf): encrypted string constants with lazy VM-side decryption"
```

The Task 2 changes were staged but uncommitted; this commit lands both.

---

## Task 4: Variance & spot-check tests

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add tests that prove encryption is happening**

Append to the `tests` mod in `crates/luau-obf/src/lib.rs`:

```rust
    #[test]
    fn output_does_not_contain_plaintext_global_name() {
        // Cardinal test: "print" must NOT appear in the obfuscated output.
        // Compile a program that uses print; the output should contain _enc(...)
        // and NOT the literal string "print" inside a Luau string literal.
        let r = obfuscate("print(\"hello\")", Options { seed: Some([55u8; 32]) }).unwrap();
        // We allow the substring "print" to appear in things like comments or
        // template scaffolding; what we forbid is a Luau string literal
        // containing the word print. Check that no `"print"` substring exists.
        assert!(!r.output.contains("\"print\""), "found plaintext \"print\" in output");
        assert!(!r.output.contains("'print'"), "found plaintext 'print' in output");
        // Sanity: confirm the encrypted-string marker IS present.
        assert!(r.output.contains("_enc("), "expected _enc(...) marker in output");
    }

    #[test]
    fn different_seeds_change_encrypted_byte_shape() {
        // Same source, two different seeds → the encrypted byte sequences
        // for "print" are different (because the keys are different).
        let a = obfuscate("print(1)", Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate("print(1)", Options { seed: Some([20u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }
```

- [ ] **Step 2: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass. If `output_does_not_contain_plaintext_global_name` fails because `"print"` literally appears in the VM template (e.g., a comment), edit the template to remove or alias the offending occurrence — but it shouldn't.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: encrypted constants hide plaintext global names"
```

---

## Task 5: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 8 — string constants in the constant pool are now stored
encrypted (XOR with per-build position-mixed key) and decrypted lazily inside
the VM. Static inspection of the obfuscated output no longer reveals global
names, method names, or user string literals. Plans 1–7 features unchanged;
full corpus passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 8 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs from Plans 1–6 pass differential testing (Plan 7 added no corpus programs; Plan 8 adds none either).
- [ ] `output_does_not_contain_plaintext_global_name` passes — proves `"print"` doesn't leak as a plaintext Luau string literal.
- [ ] `different_seeds_change_encrypted_byte_shape` passes.
- [ ] The previously-passing same-seed determinism tests still pass.
- [ ] Spot-check one generated output: `cargo run -p luau-obf-cli --release -- test.luau -o /tmp/test_obf.luau --seed 0000000000000000000000000000000000000000000000000000000000000000 --quiet && grep -c _enc /tmp/test_obf.luau` should print a count > 0; `grep '"print"' /tmp/test_obf.luau` should print nothing.
