# Luau Obfuscator — Plan 11: Constant Pool & Proto Header Uniformity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the structural fingerprints that survive Plans 7–10:
- The top-level `META` array exposes `(num_params, num_regs, num_upvals, is_vararg)` as a plaintext 4-tuple per proto, leaking the entire program shape.
- The constant pool mixes wrapped strings (`_xx("\NNN…")`) with bare plaintext numbers (`100`, `25`, `1000`) and booleans (`true`, `false`). Counting wrappers per proto reveals string-vs-number ratios; bare numeric literals like `1000` immediately suggest "transaction amount."

After Plan 11, META is folded into each proto's encrypted bytecode prologue (i.e. it vanishes from the rendered file) and every non-nil constant of every type is uniformly stored as `_const_wrap(type_tag, bytes)` — numbers as 8-byte IEEE-754 packs, booleans as 1-byte 0/1, strings as their existing encrypted blob. A reader can no longer distinguish strings from numbers from booleans by structural inspection.

**What this does NOT close (deferred to Plan 12+):**
- Decoder in plaintext (`_byte`, `_decrypt`, `_KA`, `_KB`) — addressed by a stage-0 self-decrypting bootstrap later.
- Canonical handler bodies — addressed by either dispatcher reshape or MIR-level expression mangling later.
- String-length leak even after encryption — addressed by fixed-block padding later.

**Architecture:**
- **Folded META:** The first 5 bytes of every proto's encoded bytecode become the proto header: 1 byte `num_params`, 2 bytes `num_regs` (LE u16), 1 byte `num_upvals`, 1 byte `is_vararg`. The bytecode XOR keystream applies to these bytes too (same `_byte(code, pc, proto_id)` flow), so they're stored ciphered. The Luau VM's `vm_call` reads them first (pc starts at 1, prologue runs pc=1..5, instruction stream begins at pc=6). The `META` minijinja variable and its top-level `local META = { … }` table are removed entirely from the template.
- **Uniform constant wrapping:** The render-time helper `format_const` always emits `_cw(tag, "<encrypted bytes>")` — never bare `100` or `true`. Numbers serialize via Rust's `f64::to_le_bytes` (8 bytes), booleans as a single 0/1 byte, strings as their existing byte sequence. The VM's `_const` helper reads the tag and dispatches: tag 0 = string (existing path); tag 1 = number (`string.unpack("d", bytes)`); tag 2 = boolean (`bytes[1] ~= 0`); tag 3 = nil (no payload). The encryption keystream is shared across all types — same `_KA`, `_KB`, `_k0`/`_k1` + per-proto salt as Plan 8 / Plan 9.
- The new `_cw` wrapper REPLACES the existing `_enc` wrapper. The existing string-marker shape `{ _e = bytes }` becomes `{ _e = bytes, _t = tag }`. The `_const` helper still caches the decrypted value back into the slot.

**Tech Stack:** Same as Plans 1-10.

---

## Implementation notes

**Folded META layout.** Each proto's bytecode is now: `[num_params:u8, num_regs:u16_le, num_upvals:u8, is_vararg:u8, instructions...]`. Total 5-byte prologue + the existing instruction stream. The XOR keystream is unchanged: keystream is over the entire CODE string starting at pc=1, so prologue bytes are encrypted along with everything else.

The encoder applies the same keystream operation; the Luau VM reads:

```luau
local num_params = _byte(code, 1, proto_id)
local num_regs = _byte(code, 2, proto_id) + _byte(code, 3, proto_id) * 256
local num_upvals = _byte(code, 4, proto_id)
local is_vararg = _byte(code, 5, proto_id) == 1
local pc = 6  -- instructions begin here
```

This replaces:
```luau
local meta = META[proto_id + 1]
local num_params = meta[1]
local num_regs = meta[2]
local is_vararg = meta[4] == 1
local pc = 1
```

(Plan-9 mangling renames `num_params`, `num_regs`, `num_upvals`, `is_vararg`, `pc`, `proto_id` to opaque names. That's already in MANGLE_TARGETS. Add `meta` no — it no longer exists.)

`num_upvals` isn't currently used by `vm_call` directly (closures track their own upvalue count from the bytecode), but storing it makes the prologue layout uniform and forward-compatible. Verify it's actually unused before deciding to read or skip it.

**Edge cases for folded META.**
- `num_regs` can exceed 255 (e.g. functions with many locals); 2 bytes is enough up to 65535 — sufficient for any practical Luau program.
- `num_params` can exceed 255 in pathological cases but no realistic program does; 1 byte suffices.
- `is_vararg` is just 0/1; 1 byte is overkill but keeps the prologue byte-aligned.

**Uniform constant wrapping.** The current rendered constant pool looks like:

```lua
-- proto 4's constants:
{ _xx("\147\28..."), 0, _xx("\82\104..."), true }
```

After Plan 11, every entry has the same shape:

```lua
-- proto 4's constants:
{ _cw(0, "\147\28..."), _cw(1, "\<encoded 0>"), _cw(0, "\82\104..."), _cw(2, "\1") }
```

Where:
- tag 0 = string (decoded bytes are the plaintext UTF-8 / byte string)
- tag 1 = number (decoded bytes are 8-byte little-endian IEEE 754 double, decoded via `string.unpack("<d", bytes)`)
- tag 2 = boolean (decoded bytes are 1 byte, `0x01` → true, `0x00` → false)
- tag 3 = nil (no bytes — encoded as `_cw(3, "")`)

The `_cw` wrapper IS the new `_enc`. Remove the old `_enc` definition. The marker table becomes `{ _e = bytes, _t = tag }`.

The `_const` helper:
```luau
local function _const(consts, idx, proto_salt)
    local c = consts[idx]
    if type(c) == "table" and c._t then
        local raw = _decrypt(c._e, proto_salt)
        if c._t == 0 then         -- string
            c = raw
        elseif c._t == 1 then     -- number
            c = string.unpack("<d", raw)
        elseif c._t == 2 then     -- boolean
            c = _sbyte(raw, 1) ~= 0
        else                       -- nil (tag 3)
            c = nil
        end
        consts[idx] = c
    end
    return c
end
```

Note that `consts[idx] = nil` for a tag-3 slot collapses it back to nil — which subsequent reads will see as nil too, and `type(nil) == "nil"`, so the marker-detection branch won't re-fire. That's correct for nil but slightly inefficient (re-reads always go through the `type(c) == "table"` check and find it false). Fine.

**Why not just leave nil unencrypted.** Because then nil is the ONE type that's still distinguishable. Even bare `nil` in the constant pool says "this slot is nil." Uniformity over all types is the goal. (In practice, almost no constants are nil — the lowerer rarely emits `Constant::Nil` — so this is mostly defensive.)

**Number encoding.** `f64::to_le_bytes` produces 8 bytes. The decryption operates per-byte same as strings; the salt is the proto index. Decoded bytes are passed to `string.unpack("<d", bytes)` which returns the IEEE 754 number, plus a "next position" we ignore.

**Boolean encoding.** Single byte: `b'\x01'` for true, `b'\x00'` for false. The byte goes through XOR encryption like every other byte.

**No new template-context plumbing.** `_cw` and `_const` are template-side helpers. The render pipeline already passes `_KA`, `_KB`, `k0`, `k1` for encryption. No new variables needed.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  render.rs              # MODIFY: format_const emits _cw(tag, bytes) for all types;
                         #         remove the META context (no longer needed by template)
  encode.rs              # MODIFY: encode_function prepends a 5-byte prologue
crates/luau-runtime/assets/
  vm.luau.j2             # MODIFY: remove top-level META; read prologue at vm_call entry;
                         #         _enc renamed to _cw, takes tag; _const dispatches by tag
crates/luau-emit/src/
  mangle.rs              # MODIFY: replace _enc with _cw in MANGLE_TARGETS; add `meta` removal
                         #         (it's still in MANGLE_TARGETS but no longer appears in template)
crates/luau-obf/src/
  lib.rs                 # MODIFY: add tests asserting META is absent from output,
                         #         and that bare numeric constants are absent
README.md                # MODIFY: status line
```

---

## Task 1: Fold META into the bytecode prologue

**Files:**
- Modify: `crates/luau-emit/src/encode.rs`
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Encoder prologue**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/encode.rs`, modify `encode_function` to prepend the 5-byte prologue BEFORE encoding instructions:

```rust
pub fn encode_function(
    f: &LirFunction,
    opmap: &OpMap,
    proto_id: u32,
    k0: u32,
    k1: u32,
) -> Vec<u8> {
    // Prologue: num_params:u8, num_regs:u16_le, num_upvals:u8, is_vararg:u8.
    let mut out: Vec<u8> = Vec::new();
    out.push(f.num_params as u8);
    out.push((f.num_regs & 0xFF) as u8);
    out.push(((f.num_regs >> 8) & 0xFF) as u8);
    out.push(f.num_upvals as u8);
    out.push(if f.is_vararg { 1 } else { 0 });

    // ... existing encoding logic into `out` continues here ...
    // (The instruction encoding pass that was producing the body of `out`
    //  before. The XOR-encryption loop at the end still applies to the
    //  whole `out` array, including the prologue.)
}
```

The existing instruction-encoding code that wrote into `out` continues. The XOR loop at the end now encrypts the prologue along with the instructions.

- [ ] **Step 2: Remove META from the render context**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, the `render` function currently builds `meta: Vec<MetaEntry>` and passes it as `meta => meta` in the template context. Delete that.

Also delete the `MetaEntry` struct and its derives (it's no longer used anywhere).

- [ ] **Step 3: Remove META from the template**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find:

```luau
-- Proto metadata: { num_params, num_regs, num_upvals, is_vararg }
local META = {
{% for m in meta %}    { {{ m.num_params }}, {{ m.num_regs }}, {{ m.num_upvals }}, {{ m.is_vararg }} },
{% endfor %}}
```

Delete the entire block (including the comment).

- [ ] **Step 4: Rewrite the vm_call prologue**

In the same template, find the start of `vm_call`:

```luau
vm_call = function(proto_id, args, nargs, upvals)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local meta = META[proto_id + 1]
    local num_params = meta[1]
    local num_regs = meta[2]
    local is_vararg = meta[4] == 1
    upvals = upvals or {}
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local frame_varargs
    if is_vararg then
        ...
    end
    local pc = 1
    local code_len = #code
```

Replace with:

```luau
vm_call = function(proto_id, args, nargs, upvals)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local num_params = _byte(code, 1, proto_id)
    local num_regs = _byte(code, 2, proto_id) + _byte(code, 3, proto_id) * 256
    local is_vararg = _byte(code, 5, proto_id) == 1
    upvals = upvals or {}
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local frame_varargs
    if is_vararg then
        ...
    end
    local pc = 6
    local code_len = #code
```

(We skip `num_upvals` at byte 4 since it isn't used elsewhere. The byte is still emitted by the encoder for layout uniformity.)

- [ ] **Step 5: Run the workspace + corpus tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every test passes. The bytecode is one 5-byte prologue longer per proto, but the executions are identical.

If a test fails:
- Off-by-one in `pc = 6`: verify that the instruction stream truly starts at byte 6.
- `num_regs` mismatch between encoded `f.num_regs` (u16 LE) and the VM's read `_byte(code, 2, proto_id) + _byte(code, 3, proto_id) * 256`: verify byte order.
- Forgot to delete the `local meta = ...` line.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-emit/src/encode.rs crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2
git commit -m "feat(obf): fold proto header (META) into encrypted bytecode prologue"
```

---

## Task 2: Uniform constant wrapping (numbers + booleans + nil)

**Files:**
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-runtime/assets/vm.luau.j2`
- Modify: `crates/luau-emit/src/mangle.rs`

- [ ] **Step 1: Rewrite `format_const` to emit `_cw(tag, bytes)` uniformly**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, replace `format_const`:

```rust
fn format_const(c: &Constant, key_a: &[u8; 32], key_b: &[u8; 32], proto_salt: u64) -> String {
    let (tag, plaintext): (u8, Vec<u8>) = match c {
        Constant::Nil => (3, Vec::new()),
        Constant::Bool(true) => (2, vec![1]),
        Constant::Bool(false) => (2, vec![0]),
        Constant::Number(n) => (1, n.to_le_bytes().to_vec()),
        Constant::String(s) => (0, s.as_bytes().to_vec()),
    };
    let enc = encrypt_bytes(&plaintext, key_a, key_b, proto_salt);
    format!("_cw({}, \"{}\")", tag, encode_luau_string_literal(&enc))
}
```

Rename the existing `encrypt_string` to `encrypt_bytes` (it already operates on a byte slice — the rename is just for clarity since we no longer encrypt strings exclusively):

```rust
pub(crate) fn encrypt_bytes(
    plaintext: &[u8],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(plaintext.len());
    for (i, b) in plaintext.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}
```

(The signature change from `&str` to `&[u8]` is transparent to callers if they pass `s.as_bytes()`.)

Update any existing call sites of `encrypt_string` to `encrypt_bytes`.

- [ ] **Step 2: Update existing tests in render.rs**

The previous tests called `encrypt_string("print", ...)`. Update them to `encrypt_bytes(b"print", ...)`. Same logic.

Add a new test:

```rust
    #[test]
    fn format_const_emits_cw_for_all_types() {
        let ka = [0u8; 32];
        let kb = [0u8; 32];
        let s_out = format_const(&Constant::String("hi".into()), &ka, &kb, 0);
        assert!(s_out.starts_with("_cw(0,"));
        let n_out = format_const(&Constant::Number(3.14), &ka, &kb, 0);
        assert!(n_out.starts_with("_cw(1,"));
        let b_out = format_const(&Constant::Bool(true), &ka, &kb, 0);
        assert!(b_out.starts_with("_cw(2,"));
        let nil_out = format_const(&Constant::Nil, &ka, &kb, 0);
        assert!(nil_out.starts_with("_cw(3,"));
    }
```

- [ ] **Step 3: Rewrite the VM's `_enc` and `_const` helpers**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find the existing definitions:

```luau
local function _enc(bytes) return { _e = bytes } end

local function _const(consts, idx, proto_salt)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        c = _decrypt(c._e, proto_salt)
        consts[idx] = c
    end
    return c
end
```

Replace with:

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

(`string.unpack` is a Luau global; we don't alias it. The mangler leaves it alone.)

- [ ] **Step 4: Update `MANGLE_TARGETS`**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/mangle.rs`, find `MANGLE_TARGETS`. Replace `"_enc"` with `"_cw"`. Add `"raw"` and `"tag"` (the new locals inside `_const`). Verify by inspection that the existing `"_const"`, `"_decrypt"`, etc. are still present.

- [ ] **Step 5: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Every existing test must still pass. The constant pool's appearance is dramatically different (no more bare numbers), but the runtime semantics are identical: `_const` decodes each constant to its original Lua value on first access.

If a test fails — most likely cause is a mismatch between Rust's `f64::to_le_bytes` and Luau's `string.unpack("<d", bytes)`. Verify: Rust little-endian 8 bytes for 3.14 should match Luau unpacking. Standard IEEE 754 — they agree.

Another possible failure: Luau number-to-string formatting differs slightly between values that came from the constant pool (now via `string.unpack`) vs values computed at runtime. For corpus differential tests, this matters only if a `print` of a "loaded constant" produces a slightly different string than a `print` of the same number from runtime computation. Test: `print(3.14)` should produce `"3.14"` in both paths.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-emit/src/mangle.rs
git commit -m "feat(obf): uniform constant encryption — wrap numbers, booleans, nil"
```

---

## Task 3: Acceptance tests

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add tests asserting META is gone and bare constants are gone**

Append to the `tests` mod:

```rust
    #[test]
    fn output_has_no_top_level_meta_table() {
        let r = obfuscate("print(1)", Options { seed: Some([170u8; 32]) }).unwrap();
        // META is a renamed identifier in Plan-9's MANGLE_TARGETS, so the literal
        // "META" should never appear, AND no `local _xx = {` pattern that looks
        // like a 4-tuple-per-row table should appear at the top.
        assert!(!r.output.contains("META"));
        // Look for the canonical META shape: `{ 0, N, 0, 0 },` lines. After Plan
        // 11, this pattern should be absent (one occurrence is `_KA = { … }`
        // and another is `_KB = { … }` — those are byte arrays, single row each).
        let four_tuple_lines: usize = r.output.lines().filter(|l| {
            // Match e.g. "    { 0, 5, 0, 0 },"
            let t = l.trim();
            t.starts_with("{ ") && t.ends_with("},")
                && t.matches(',').count() == 3
        }).count();
        assert!(four_tuple_lines == 0,
            "found {} suspected META rows in output", four_tuple_lines);
    }

    #[test]
    fn output_has_no_bare_numeric_constants_in_const_pool() {
        // Plan 11: every constant of every type is wrapped in _cw(tag, bytes).
        // The constant pool tables should contain ONLY _cw(...) entries (or
        // potentially nothing, if a proto has no constants).
        // Heuristic: find lines that look like a constants entry of the form
        // `{ <comma-separated values> },` and assert no bare integer/decimal
        // numeric literals appear as top-level items.
        let r = obfuscate(
            "local function f(a) return a + 100 end print(f(25))",
            Options { seed: Some([180u8; 32]) }
        ).unwrap();
        // The numbers 100 and 25 from the source should NOT appear as bare
        // integers anywhere in the output (they're encrypted as _cw(1, "...")).
        let bare_hundred_pattern_count: usize = r.output.matches(", 100,").count()
            + r.output.matches("{ 100,").count()
            + r.output.matches(", 100 ").count();
        assert_eq!(bare_hundred_pattern_count, 0,
            "found bare `100` in output, expected encrypted as _cw");
        let bare_25_count: usize = r.output.matches(", 25,").count()
            + r.output.matches("{ 25,").count()
            + r.output.matches(", 25 ").count();
        assert_eq!(bare_25_count, 0,
            "found bare `25` in output, expected encrypted as _cw");
    }

    #[test]
    fn output_has_no_bare_boolean_in_const_pool() {
        // A program with `return true` / `return false` puts those into the
        // constant pool. After Plan 11, they're _cw(2, ...) and never appear
        // as the bare words `true,` `false,` in a constants table.
        let r = obfuscate(
            "local function f() return true end print(f())",
            Options { seed: Some([190u8; 32]) }
        ).unwrap();
        // Heuristic: count occurrences of `true,` or `, true,` or `{ true,`
        // — these would be const-pool entries. After Plan 11, expected 0.
        let bare_true: usize = r.output.matches(", true,").count()
            + r.output.matches("{ true,").count();
        assert_eq!(bare_true, 0, "found bare `true,` in output");
    }
```

- [ ] **Step 2: Run all tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every test passes. The 3 new acceptance tests prove META and bare constants are gone.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 11 acceptance — no META, no bare constant literals"
```

---

## Task 4: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 11 — constant pool and proto-header uniformity. The top-level
META table is gone (proto headers fold into each function's encrypted bytecode
prologue). Every constant of every type is uniformly wrapped as `_cw(tag,
bytes)` — strings, numbers, booleans, and nil all encrypt the same way, so
reading the constant pool no longer tells you "this proto has N strings and
M numbers." Full differential corpus still passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 11 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs from Plans 1–6 pass differential testing.
- [ ] `output_has_no_top_level_meta_table` passes.
- [ ] `output_has_no_bare_numeric_constants_in_const_pool` passes.
- [ ] `output_has_no_bare_boolean_in_const_pool` passes.
- [ ] All Plan 8 / 9 / 10 acceptance tests still pass.
- [ ] Spot-check the generated output for `runtime-tests/corpus/arithmetic.luau` shows: no `local META = {`, no bare numeric or boolean tokens in constants tables, every wrapped constant uses `_cw(N, "…")` form.
