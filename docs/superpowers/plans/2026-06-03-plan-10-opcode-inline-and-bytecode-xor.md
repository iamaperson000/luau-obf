# Luau Obfuscator — Plan 10: Inline Opcode Bytes + Bytecode XOR Encoding

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the two highest-impact static weaknesses identified by a round-2 deobfuscation pass against Plan-9 output:
1. The dispatcher still contains 35 plaintext `local _xx = <small int>` opcode declarations at the top. Even with mangled names, the count and the value set scream "35-opcode VM." Plan 10 eliminates the entire opcode-constants block — opcode bytes are inlined directly into the `elseif op == N then` arms.
2. Bytecode is stored as a Luau string literal with `\NNN` decimal escapes. `string.byte` over each function's `CODE[i]` recovers the raw byte stream trivially — a deobfuscator can scan for the (now eliminated, after #1) opcode-byte set in O(n). Plan 10 XOR-encodes every bytecode byte at compile time against a per-build, per-proto keystream derived from the program counter; the VM decodes on read via a `_byte(pc, proto_id)` helper.

After Plan 10, the obfuscated output has no opcode-table tell AND raw `string.byte(code, pc)` returns meaningless ciphertext bytes. To disassemble, a deobfuscator must reproduce the keystream — which requires lifting `_byte` (a mangled helper buried in the mangled dispatcher).

**Architecture:**
- **Opcode inlining (template-level):** The render pipeline gains an `ops` minijinja context (a dict mapping each opcode name to its per-build byte). The template removes its `{% for opname, num in opcodes %}local OP_{{ opname }} = {{ num }}{% endfor %}` block, and every `OP_LoadConst` mention in the dispatcher becomes `{{ ops.LoadConst }}` — so the rendered file says `elseif op == 14 then` rather than `elseif op == OP_LoadConst then`. Identifier mangling no longer has anything to mangle for opcodes (since they no longer appear as locals); the OP_X entries are removed from `MANGLE_TARGETS`.
- **Bytecode XOR encoding (encode + VM):** A small, seed-derived 32-bit key `k0` and a 32-bit multiplier `k1` are derived in `render`. The Rust encoder XOR-encodes each output byte with `((pc * k1) + k0 + proto_id * SOME_PRIME) & 0xFF` where `pc` is the byte's 1-based offset within the proto's bytecode. The Luau VM has a `_byte(code, pc, proto_id)` helper applying the same transform; `read_u16`/`read_i16` are rewritten to use `_byte` instead of `_sbyte` directly.
- The transformations are independent: opcode inlining works without bytecode XOR, and vice versa. Both ship in this plan because they're each small individually but together they unambiguously break the round-2 deobfuscator's static disassembly path.

**Tech Stack:** Same as Plans 1-9.

---

## Implementation notes

**Why inline rather than rename.** Plan 9 mangled `OP_LoadConst` → `_xa`. The opcode-constants block became 35 lines of `local _xx = N`. Even though the names are opaque, the *structure* is unmistakable. Inlining replaces the indirection entirely: there's no `local` at all; the dispatcher arms read `op == 14`. A reader sees ~35 numeric comparisons against small literals in a single `if/elseif` chain — but they no longer get the *set* of opcodes listed up front, and they can't grep the file to find the opcode universe.

**Keystream design.** For each byte at position `pc` (1-based) inside proto `proto_id`'s code string:
- Rust side, applied to plaintext bytes:
  ```rust
  fn keystream_byte(pc: usize, proto_id: u32, k0: u32, k1: u32) -> u8 {
      let mixed = (pc as u32)
          .wrapping_mul(k1)
          .wrapping_add(k0)
          .wrapping_add(proto_id.wrapping_mul(2_654_435_761));  // Knuth's multiplicative hash prime
      (mixed & 0xFF) as u8
  }
  ```
  At encode time: `ciphertext[i] = plaintext[i] ^ keystream_byte(i + 1, proto_id, k0, k1)` (the `i + 1` shifts to 1-based since the VM reads `_sbyte(code, pc)` with 1-based `pc`).
- Luau side, applied in `_byte`:
  ```luau
  local function _byte(code, pc, pid)
      local b = _sbyte(code, pc)
      local mixed = ((pc * _k1) + _k0 + (pid * 2654435761)) % 4294967296
      return _xor(b, mixed % 256)
  end
  ```
  Where `_k0` and `_k1` are rendered as plain numeric constants near the top of the template.

The `_k0` / `_k1` constants are in plaintext, but their NAMES get mangled by the Plan-9 mangler. A deobfuscator can extract the numeric values — that's fine; the keystream isn't trying to hide its formula, just to make the bytes opaque to direct extraction. Anyone who lifts `_byte` and reruns it against `CODE[i]` recovers the plaintext bytes. But (a) they have to do that work; (b) it's no longer a one-liner `string.byte` call; (c) once they have the bytes, they STILL face Plan 9's mangled-name dispatcher with no opcode table.

**Why this isn't "encryption."** It's an XOR keystream — by design, the deobfuscator can recover plaintext if they reverse-engineer `_byte`. The point is to eliminate the `\NNN`-extractable structural signal, not to hide the bytecode forever. The actual obfuscation strength against semantic recovery comes from later plans (control-flow flattening, expression mangling, junk insertion).

**Determinism + variance.** Same seed → same `k0`, `k1` → same ciphertext bytes. Different seed → different keystream → completely different bytecode bytes. The existing variance tests (`different_seeds_produce_different_outputs`) already cover this; no new test needed for variance per se, but a corpus run with several seeds is the acceptance gate.

**Template structural change.** The template's existing dispatcher looks like:
```luau
{% for opname, num in opcodes %}local OP_{{ opname }} = {{ num }}
{% endfor %}
...
local function vm_call(...)
    ...
    if op == OP_LoadNil then
        ...
    elseif op == OP_LoadConst then
        ...
```

After Plan 10:
```luau
-- (no opcode declarations)
local function vm_call(...)
    ...
    if op == {{ ops.LoadNil }} then
        ...
    elseif op == {{ ops.LoadConst }} then
        ...
```

`ops` is a minijinja dict; `{{ ops.LoadNil }}` renders as the integer byte assigned to LoadNil by the opmap. ~35 occurrences of `OP_X` in the template body need updating.

**Operand-read sites.** The template reads bytecode via two helpers — `read_u16` and `read_i16` — and directly via `_sbyte(code, pc)` at the dispatch (`local op = _sbyte(code, pc)`). All three call sites need to thread `proto_id` and call `_byte` instead of `_sbyte`:
- Rewrite `_sbyte(code, pc)` → `_byte(code, pc, proto_id)` at the dispatch.
- Rewrite `read_u16(code, pos)` body to call `_byte(code, pos, proto_id)` twice.
- Same for `read_i16`.
- Update every call site to pass `proto_id`.

`read_u16` and `read_i16` thus gain a third argument. There are many call sites — ~40+ — but they're all uniform substitutions.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  encode.rs              # MODIFY: take k0, k1, proto_id, XOR bytes during encode
  render.rs              # MODIFY: derive k0, k1; build ops dict; pass to encoder + template
crates/luau-runtime/assets/
  vm.luau.j2             # MODIFY: remove opcode-constants block;
                         #         use {{ ops.X }} in dispatcher;
                         #         add _byte helper;
                         #         rewrite read_u16/read_i16 to use _byte;
                         #         thread proto_id through call sites
crates/luau-emit/src/
  mangle.rs              # MODIFY: remove OP_* names from MANGLE_TARGETS
                         #         (they no longer appear as identifiers)
crates/luau-obf/src/
  lib.rs                 # MODIFY: add tests that the OP_-named locals are absent AND
                         #         that string.byte of the bytecode no longer matches
                         #         a small known set
README.md                # MODIFY: status line
```

---

## Task 1: Inline opcode bytes — render context + template

**Files:**
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Build the `ops` dict in render**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, in the `render` function body, AFTER the existing `opcodes` computation, add:

```rust
    let ops: std::collections::HashMap<String, u8> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();
```

Add `ops` to the template context:

```rust
    tmpl.render(minijinja::context! {
        opcodes => opcodes,
        ops => ops,
        consts => consts,
        codes => codes,
        meta => meta,
        key_a => key_a_lit,
        key_b => key_b_lit,
    })
```

Keep the existing `opcodes` context for now (we'll remove it once the template no longer iterates it).

- [ ] **Step 2: Remove the opcode-constants block from the template**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find:

```luau
-- Opcode constants
{% for opname, num in opcodes %}local OP_{{ opname }} = {{ num }}
{% endfor %}
```

Delete the entire block (both the comment and the `{% for %}`). The comment will be stripped by Plan 9's comment-stripper anyway, but deleting it explicitly is cleaner.

- [ ] **Step 3: Replace every `OP_X` reference in the dispatcher with `{{ ops.X }}`**

The dispatcher has ~35 `elseif op == OP_X then` lines. Each needs to become `elseif op == {{ ops.X }} then`. Use a careful find-and-replace. The complete list to replace (these are the only places `OP_*` appears in the template):

- `OP_LoadNil` → `{{ ops.LoadNil }}`
- `OP_LoadTrue` → `{{ ops.LoadTrue }}`
- `OP_LoadFalse` → `{{ ops.LoadFalse }}`
- `OP_LoadConst` → `{{ ops.LoadConst }}`
- `OP_Move` → `{{ ops.Move }}`
- `OP_Add` → `{{ ops.Add }}`
- `OP_Sub` → `{{ ops.Sub }}`
- `OP_Mul` → `{{ ops.Mul }}`
- `OP_Div` → `{{ ops.Div }}`
- `OP_Mod` → `{{ ops.Mod }}`
- `OP_Pow` → `{{ ops.Pow }}`
- `OP_Concat` → `{{ ops.Concat }}`
- `OP_Lt` → `{{ ops.Lt }}`
- `OP_Le` → `{{ ops.Le }}`
- `OP_Eq` → `{{ ops.Eq }}`
- `OP_Not` → `{{ ops.Not }}`
- `OP_Neg` → `{{ ops.Neg }}`
- `OP_Len` → `{{ ops.Len }}`
- `OP_GetGlobal` → `{{ ops.GetGlobal }}`
- `OP_SetGlobal` → `{{ ops.SetGlobal }}`
- `OP_Call` → `{{ ops.Call }}`
- `OP_Return` → `{{ ops.Return }}`
- `OP_Jmp` → `{{ ops.Jmp }}`
- `OP_JmpIfTrue` → `{{ ops.JmpIfTrue }}`
- `OP_JmpIfFalse` → `{{ ops.JmpIfFalse }}`
- `OP_Closure` → `{{ ops.Closure }}`
- `OP_NewTable` → `{{ ops.NewTable }}`
- `OP_GetTable` → `{{ ops.GetTable }}`
- `OP_SetTable` → `{{ ops.SetTable }}`
- `OP_GetUpval` → `{{ ops.GetUpval }}`
- `OP_SetUpval` → `{{ ops.SetUpval }}`
- `OP_CallVar` → `{{ ops.CallVar }}`
- `OP_BuildResults` → `{{ ops.BuildResults }}`
- `OP_Vararg` → `{{ ops.Vararg }}`
- `OP_ReturnMulti` → `{{ ops.ReturnMulti }}`

Verify: `grep -c 'OP_' crates/luau-runtime/assets/vm.luau.j2` should return 0 after this step.

- [ ] **Step 4: Remove `OP_*` from `MANGLE_TARGETS`**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/mangle.rs`, find the section in `MANGLE_TARGETS`:

```rust
    // Opcode constants.
    "OP_LoadNil", "OP_LoadTrue", "OP_LoadFalse", "OP_LoadConst", "OP_Move",
    "OP_Add", "OP_Sub", ...
    "OP_CallVar", "OP_BuildResults", "OP_Vararg", "OP_ReturnMulti",
```

Delete all `OP_*` entries. The mangler no longer needs to handle them because they're not in the rendered output anymore.

- [ ] **Step 5: Run the full workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all 23 corpus programs still pass. The output now has no opcode-constants block; opcode bytes are inlined as numeric literals in the dispatcher arms.

If any test fails — most likely cause is a missed `OP_X` in step 3, which would render as the literal text `OP_LoadConst` and Luau would treat it as a global lookup yielding nil, breaking the dispatcher. Search the output for `OP_` after a fresh build; should be 0.

- [ ] **Step 6: Spot-check the output**

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p luau-obf-cli --release -- runtime-tests/corpus/arithmetic.luau -o /tmp/spotcheck.luau --seed 0000000000000000000000000000000000000000000000000000000000000000 --quiet
grep -c 'OP_' /tmp/spotcheck.luau                # must be 0
grep -cE 'elseif [a-z_]+ == [0-9]+ then' /tmp/spotcheck.luau   # should be 34 (one less than 35, because the first arm is `if` not `elseif`)
head -50 /tmp/spotcheck.luau                       # should NOT have a long flat block of `local _xx = N`
```

- [ ] **Step 7: Commit**

```bash
git add crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-emit/src/mangle.rs
git commit -m "feat(emit): inline opcode bytes in dispatcher (eliminate opcode-constants block)"
```

---

## Task 2: Bytecode XOR encoding — Rust encoder side

**Files:**
- Modify: `crates/luau-emit/src/encode.rs`
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Add the keystream helper to `encode.rs`**

Near the top of `/Volumes/External Drive/luau-obf/crates/luau-emit/src/encode.rs`, add:

```rust
/// Compute the keystream byte for a single bytecode position.
/// pc is 1-based (matches the VM's `_sbyte(code, pc)` semantics).
pub(crate) fn keystream_byte(pc: u32, proto_id: u32, k0: u32, k1: u32) -> u8 {
    let mixed = pc
        .wrapping_mul(k1)
        .wrapping_add(k0)
        .wrapping_add(proto_id.wrapping_mul(2_654_435_761));
    (mixed & 0xFF) as u8
}
```

(`2_654_435_761` is Knuth's multiplicative-hash constant — golden-ratio prime. It's intentionally hard-coded so the VM-side decoder uses the same number.)

- [ ] **Step 2: Modify `encode_function` to take `proto_id`, `k0`, `k1` and XOR the output**

Change the signature and body of `encode_function`:

```rust
pub fn encode_function(
    f: &LirFunction,
    opmap: &OpMap,
    proto_id: u32,
    k0: u32,
    k1: u32,
) -> Vec<u8> {
    // ... existing body unchanged through the loop that builds `out` ...
    // At the end, XOR every byte by its keystream:
    for (i, byte) in out.iter_mut().enumerate() {
        let pc = (i + 1) as u32;  // 1-based
        *byte ^= keystream_byte(pc, proto_id, k0, k1);
    }
    out
}
```

(Apply the XOR loop just before `out` is returned.)

- [ ] **Step 3: Update `encode.rs`'s own unit tests for the new signature**

The existing `encodes_single_return_instruction` test calls `encode_function(&f, &opmap)`. Change it to:

```rust
        let bytes = encode_function(&f, &opmap, 0, 0, 0);
```

Passing `k0=0, k1=0` means the keystream is `(pc * 0 + 0 + 0 * P) & 0xFF` = 0, so no XOR applied — preserves the test's existing byte-level assertions.

- [ ] **Step 4: Derive k0, k1 in render and thread to encode_function**

In `crates/luau-emit/src/render.rs`, before the `codes` computation, add:

```rust
    use rand::RngCore;
    let mut k_buf = [0u8; 8];
    rng.fill_bytes(&mut k_buf);
    let k0 = u32::from_le_bytes([k_buf[0], k_buf[1], k_buf[2], k_buf[3]]);
    let k1 = u32::from_le_bytes([k_buf[4], k_buf[5], k_buf[6], k_buf[7]]) | 1;
    // k1 forced odd so the keystream's `pc * k1` term varies with pc on a byte basis.
```

Update the `codes` computation to pass these through:

```rust
    let codes: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let bytes = encode_function(f, opmap, i as u32, k0, k1);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();
```

Add `k0` and `k1` to the template context (rendered as decimal integers):

```rust
    tmpl.render(minijinja::context! {
        opcodes => opcodes,
        ops => ops,
        consts => consts,
        codes => codes,
        meta => meta,
        key_a => key_a_lit,
        key_b => key_b_lit,
        k0 => k0,
        k1 => k1,
    })
```

- [ ] **Step 5: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every unit test passes BUT the corpus differential tests now FAIL — the bytecode bytes are XOR-encoded but the VM still reads them plain. That's correct; Task 3 wires up the VM side.

If the encode unit tests fail, the XOR path has a bug — STOP and report.

If the corpus tests fail (which they should), DO NOT commit. Proceed to Task 3.

---

## Task 3: Bytecode XOR decoding — VM template

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Add `_k0`, `_k1` constants near the top**

Find the existing `_KA` / `_KB` block in `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`. After it (before the `_decrypt` function), add:

```luau
local _k0 = {{ k0 }}
local _k1 = {{ k1 }}

local function _byte(code, pc, pid)
    local b = _sbyte(code, pc)
    local mixed = ((pc * _k1) + _k0 + (pid * 2654435761)) % 4294967296
    return _xor(b, mixed % 256)
end
```

- [ ] **Step 2: Rewrite `read_u16` and `read_i16` to use `_byte` and accept `pid`**

Replace the existing `read_u16` and `read_i16` definitions:

```luau
-- Read a little-endian u16 at byte offset `pos` (1-based) of `code`.
local function read_u16(code, pos, pid)
    local lo = _byte(code, pos, pid)
    local hi = _byte(code, pos + 1, pid)
    return lo + hi * 256
end

-- Read signed i16 at 1-based offset.
local function read_i16(code, pos, pid)
    local v = read_u16(code, pos, pid)
    if v >= 32768 then v = v - 65536 end
    return v
end
```

Then add `_byte` to MANGLE_TARGETS (Plan 9's allow list).

- [ ] **Step 3: Update every call site that reads from `code`**

This is the bulk of the work. Inside `vm_call`, every:
- `_sbyte(code, pc)` → `_byte(code, pc, proto_id)`
- `read_u16(code, pc)` → `read_u16(code, pc, proto_id)`
- `read_i16(code, pc)` → `read_i16(code, pc, proto_id)`

Use a careful sweep. The key sites:

```luau
        local op = _sbyte(code, pc)
```
becomes:
```luau
        local op = _byte(code, pc, proto_id)
```

And every `read_u16(code, pc); pc = pc + 2` in handler bodies adds `, proto_id` after `pc`:

```luau
            local a = read_u16(code, pc, proto_id); pc = pc + 2
```

There are ~50+ such call sites. Mechanical but exhaustive. Verify with grep: after the sweep, `grep -E '_sbyte\(code|read_u16\(code, [a-z]+\)|read_i16\(code, [a-z]+\)' crates/luau-runtime/assets/vm.luau.j2` should return 0 (no old-form calls remain).

Watch out: there's also one occurrence of `_sbyte(code, pc)` inside the `Closure` handler for reading the upvalue `kind` byte:

```luau
                local kind = _sbyte(code, pc); pc = pc + 1
```

That becomes:

```luau
                local kind = _byte(code, pc, proto_id); pc = pc + 1
```

- [ ] **Step 4: Update `MANGLE_TARGETS` to include `_byte`, `_k0`, `_k1`**

In `crates/luau-emit/src/mangle.rs`, add to the `MANGLE_TARGETS` list:

```rust
    "_byte", "_k0", "_k1",
```

(Probably in the "Encryption" section alongside `_KA`, `_KB`.)

Also add `pid` (the third parameter of `_byte`, `read_u16`, `read_i16`) — but the existing list already has many similar names. Verify by checking: a call like `read_u16(code, pc, proto_id)` would not be affected by mangling of `pid` since the argument name is local to `read_u16`'s body. So adding `pid` is only needed if it appears as a NAME inside the body — yes, it's used as `mixed = ((pc * _k1) + _k0 + (pid * 2654435761)) % 4294967296`. Add `"pid"` to MANGLE_TARGETS.

- [ ] **Step 5: Run the full workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every corpus differential test passes. The bytecode bytes are now XOR-encoded by the Rust encoder, and `_byte` undoes the XOR on each read inside the VM.

If a test fails:
- **Likely cause #1:** mismatch between Rust's `(pc * k1).wrapping_add(k0).wrapping_add(pid * 2654435761) & 0xFF` and Luau's `((pc * _k1) + _k0 + (pid * 2654435761)) % 4294967296`. Verify: Rust does u32 wrapping arithmetic; Luau does double-precision arithmetic mod 2^32. For typical 16-bit pc values and 32-bit keys, the result should be IDENTICAL because the intermediate values stay well under 2^53 (Luau's exact-integer ceiling).
- **Likely cause #2:** missed a `_sbyte(code, ...)` or a `read_u16(code, ...)` call. Search for any remaining 2-arg form.
- **Likely cause #3:** the Closure handler's `kind` byte read is one byte wide and increments `pc` by 1, not 2. Make sure that site uses `_byte(code, pc, proto_id)` (the `pid` arg).

- [ ] **Step 6: Spot-check**

After tests pass:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p luau-obf-cli --release -- runtime-tests/corpus/arithmetic.luau -o /tmp/spotcheck.luau --seed 0000000000000000000000000000000000000000000000000000000000000000 --quiet
# Extract one of the CODE entries and confirm it's no longer a recognizable
# sequence of small opcode bytes. Pull a code literal:
grep -oE '"\\[0-9]+(\\[0-9]+)*[^"]*"' /tmp/spotcheck.luau | head -3
# The output should look like random bytes, NOT a uniform distribution of values 1-35.
```

There's no automated test here for "the bytecode looks random" because that's subjective; the differential corpus passing is the correctness gate, and the structural change is observable on inspection.

- [ ] **Step 7: Commit (Tasks 2 + 3 combined)**

The intermediate state (Task 2 only) was broken; Task 3 closes the loop.

```bash
git add crates/luau-emit/src/encode.rs crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-emit/src/mangle.rs
git commit -m "feat(obf): per-build XOR keystream encoding for bytecode bytes"
```

---

## Task 4: Tests — opcode set is no longer enumerable

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add post-Plan-10 acceptance tests**

Append to the `tests` mod in `/Volumes/External Drive/luau-obf/crates/luau-obf/src/lib.rs`:

```rust
    #[test]
    fn output_has_no_opcode_constants_block() {
        // Plan 10: the opcode table is inlined, so the output should NOT contain
        // 35 consecutive `local _xx = N` declarations of small integers.
        let r = obfuscate("print(1 + 2)", Options { seed: Some([100u8; 32]) }).unwrap();
        // Look for the canonical signature: many short `local NAME = SMALLNUM\n` lines.
        // Heuristic: count lines matching `^local _\w+ = \d+\s*$` (just an integer literal,
        // no operator). After Plan 10 there should be at most a handful (k0, k1, _KA-element
        // declarations could match) — well below 35.
        let lines_matching: usize = r.output.lines().filter(|l| {
            let trimmed = l.trim();
            // local _xx = NNN  (no operators on the RHS)
            if !trimmed.starts_with("local ") { return false; }
            let rest = trimmed.trim_start_matches("local ");
            let mut parts = rest.splitn(2, '=');
            let (lhs, rhs) = match (parts.next(), parts.next()) {
                (Some(l), Some(r)) => (l.trim(), r.trim()),
                _ => return false,
            };
            if !lhs.starts_with('_') { return false; }
            // RHS is a bare nonnegative integer.
            rhs.chars().all(|c| c.is_ascii_digit())
        }).count();
        assert!(lines_matching < 10,
            "expected fewer than 10 `local _x = <int>` lines, found {}", lines_matching);
    }

    #[test]
    fn bytecode_bytes_are_not_small_opcode_set() {
        // Plan 10: bytecode bytes are XOR-encoded, so the BYTE DISTRIBUTION inside
        // any CODE entry should NOT be tightly clustered in 1..=35.
        let r = obfuscate(
            "local function f(a, b) return a + b end print(f(3, 4))",
            Options { seed: Some([200u8; 32]) }
        ).unwrap();
        // Find the first CODE entry — look for a long `"\NNN…"` string literal.
        // Extract the first string literal that has more than 10 `\NNN` escapes.
        let mut bytes_found: Vec<u8> = Vec::new();
        for line in r.output.lines() {
            if let Some(q1) = line.find('"') {
                if let Some(q2_rel) = line[q1+1..].rfind('"') {
                    let payload = &line[q1+1..q1+1+q2_rel];
                    // Parse the \NNN escapes.
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut chars = payload.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '\\' {
                            let mut digits = String::new();
                            for _ in 0..3 {
                                if let Some(&dc) = chars.peek() {
                                    if dc.is_ascii_digit() { digits.push(dc); chars.next(); }
                                    else { break; }
                                }
                            }
                            if let Ok(n) = digits.parse::<u32>() {
                                if n <= 255 { bytes.push(n as u8); }
                            }
                        } else if c.is_ascii() {
                            bytes.push(c as u8);
                        }
                    }
                    if bytes.len() > 30 {
                        bytes_found = bytes;
                        break;
                    }
                }
            }
        }
        assert!(!bytes_found.is_empty(), "could not find a bytecode literal");
        // If the bytes were plaintext opcodes, every byte would be in 1..=35
        // (with operands as u16 LE pairs interleaved). After XOR encoding, the
        // bytes are pseudo-random and should hit values outside 0..50 frequently.
        let above_50: usize = bytes_found.iter().filter(|&&b| b > 50).count();
        let ratio = above_50 as f64 / bytes_found.len() as f64;
        assert!(ratio > 0.3,
            "fewer than 30% of bytecode bytes are above 50 ({} of {}); \
             suggests XOR encoding isn't applied", above_50, bytes_found.len());
    }
```

- [ ] **Step 2: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: both new tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: post-Plan-10 acceptance — no opcode block, XOR'd bytecode"
```

---

## Task 5: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 10 — opcode bytes are inlined directly into the dispatcher
(the 35-line opcode-constants block is gone) and bytecode bytes themselves are
XOR-encoded per build against a position-mixed keystream. Static analysis of
the obfuscated output no longer sees a labeled VM, no longer sees an opcode
table, and `string.byte` over a `CODE[i]` entry returns pseudo-random bytes
rather than the underlying opcode stream. Full corpus passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 10 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs from Plans 1–6 pass differential testing.
- [ ] `output_has_no_opcode_constants_block` passes.
- [ ] `bytecode_bytes_are_not_small_opcode_set` passes.
- [ ] Spot-check: `grep -c 'OP_' /tmp/spotcheck.luau` → 0; `grep -cE 'elseif [a-z_]+ == [0-9]+ then' /tmp/spotcheck.luau` is 34 or so.
- [ ] Same-seed determinism preserved.
- [ ] Different-seed variance preserved.
- [ ] All Plan 8 + Plan 9 tests still pass (encrypted constants, mangled identifiers, stripped comments, per-proto salt).
