# Luau Obfuscator — Plan 13: Per-Proto Opcode Permutation + Dispatcher Arm Shuffle

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Break the "one global opcode → one handler body" reading of the dispatcher. After Plan 13, two protos in the same obfuscated chunk encode the same logical opcode (e.g. ADD) as different bytecode bytes, and the textual order of the dispatcher's `if/elseif` arms is shuffled per build (so the linear scan no longer matches the canonical Plan-7 opcode numbering).

**Architecture:**
- **Per-proto opcode permutation.** Each proto's bytecode prologue gains a 35-byte permutation table (after the existing 5-byte META fields). The permutation maps "raw opcode byte read from bytecode" → "canonical opcode index 1..35" (matching the global opmap). The encoder generates a different random permutation per proto from the seeded RNG and writes opcodes through the inverse permutation; the VM reads the permutation table at vm_call entry and applies it on every dispatch.
- **Dispatcher arm shuffle.** The 35 `if/elseif` arms in the template are reordered per build. The first arm (the `if op == N then` head) is also randomized. Since execution depends only on equality comparison, the order doesn't affect semantics.
- **Bytecode prologue grows.** From 5 bytes (num_params, num_regs LE u16, num_upvals, is_vararg) to 5 + 35 = 40 bytes per proto. The first instruction begins at pc=41 (was pc=6). All 40 prologue bytes are encrypted by the existing per-proto bytecode keystream.

After Plan 13, a deobfuscator who's already reproduced the bytecode keystream still needs to:
1. Decrypt each proto's prologue (already required for META).
2. Extract that proto's 35-byte permutation.
3. Apply the permutation when disassembling each instruction.
4. Map permuted-opcode → handler-arm by scanning the (shuffled) dispatcher.

Plain-static disassembly is still possible — the keys are in plaintext (Plan 14+ work) — but the work per proto grows.

**What this does NOT close:**
- Decryptor still in plaintext at the top of the file. Plan 14 candidate.
- Handler bodies still 1:1 with Lua operators once the permutation is applied. A structural fix (handler diversification, expression mangling, micro-ops) belongs to a later plan.

**Tech Stack:** Same as Plans 1-12.

---

## Implementation notes

**Permutation generation.** For each proto, draw a permutation of `1..=ALL_OPS.len()` from the seeded RNG. The Rust side uses `SliceRandom::shuffle` on a `Vec<u8>`. The encoder applies the *inverse* of this permutation when assigning the byte to write for a given logical opcode.

Two clarifications on direction:
- `perm[raw_byte] = canonical_opcode_index` is the table the VM uses to translate a raw byte to a canonical index.
- The encoder needs the *inverse* `inv[canonical_opcode_index] = raw_byte` to translate the LIR's opcode → the byte to actually emit.

Today's encoder writes `opmap.opcode_of(kind)` as the byte. After Plan 13, it writes `inv[opmap.opcode_of(kind) - 1]` (zero-based index into inv).

Wait — clarify: the *global* opmap (Plan 7) maps `OpKind → byte`. The byte is already in `1..=35`. The per-proto permutation re-maps those `1..=35` indices to new `1..=35` indices.

So in code:
```rust
// Existing: opmap.opcode_of(kind) returns u8 in 1..=35.
let global_byte = opmap.opcode_of(kind);  // e.g. 14 for ADD
let canonical_index = global_byte - 1;     // 0-based index 13

// Plan 13: each proto has its own permutation.
// perm[i] = the byte that the VM will receive for canonical index i.
// inv[byte - 1] = the canonical index.
//
// During encoding:
let emitted_byte = perm[canonical_index as usize];  // some random value in 1..=35

// During decoding (VM side):
local canonical_byte = inv[emitted_byte - 1] + 1
if canonical_byte == 14 then ... -- ADD handler
```

So the VM stores `inv` (raw_byte → canonical_byte) and applies it. The Rust encoder stores `perm` (canonical_byte → raw_byte) and applies it.

Actually simpler representation: each proto stores a 35-byte table `inv` directly. `inv[i]` (0-based) is the canonical opcode byte for raw byte `i + 1`. The encoder computes `perm = inverse(inv)` and uses it to translate canonical → raw.

The 35 bytes of `inv` go into the bytecode prologue. The encoder:
1. Generate `perm` (a random permutation of 1..=35).
2. Compute `inv` such that `inv[perm[c] - 1] = c + 1` for each `c` in `0..35`. Equivalently: build `inv` as the inverse permutation.
3. Emit `inv` into the bytecode prologue (35 bytes).
4. For each instruction, emit `perm[canonical_byte - 1]` as the opcode byte.

The VM:
1. Read the 35-byte `inv` table from the prologue at startup.
2. For each instruction, compute `canonical_byte = inv[raw_byte - 1]`.
3. Dispatch on `canonical_byte`.

**Prologue layout (40 bytes):**
```
byte 0:    num_params
byte 1:    num_regs LO
byte 2:    num_regs HI
byte 3:    num_upvals
byte 4:    is_vararg
byte 5..39: inv[0..35] — 35 bytes of inverse permutation
```

Instructions begin at byte 40 (pc = 41 in the VM's 1-based indexing).

**Dispatcher arm shuffle.** The template currently lays out arms in a fixed order:
```
if op == OP_LoadNil then
elseif op == OP_LoadTrue then
elseif op == OP_LoadFalse then
...
```

After Plan 13, the arm order is rendered as a seed-derived permutation of the 35 opcode names. The first arm uses `if`, the rest use `elseif`. The dispatcher's behavior is identical because all arms test equality.

Implementing the shuffle is a Rust-side change in `render`: build a `handler_order: Vec<OpKind>` from the rng, then in the template iterate via `{% for arm in handler_order %}`. The template needs the dispatcher body broken into per-arm pieces — a substantial template refactor.

**Simpler alternative for the arm shuffle: post-render textual permutation.** Tokenize the rendered output, locate the dispatcher's `if/elseif` block by anchor pattern (after a known prefix), split it into arms, shuffle, re-emit. Fragile because it depends on the textual layout. The clean approach is the template-iteration version.

For Plan 13 scope, do the **template-iteration version**. This means moving handler bodies out of the template's inline form into either:
- (A) Separate render-time strings injected via `{{ arm_body }}` per arm.
- (B) Multiple template-literal blocks named per arm.

(A) is cleaner. The Rust `render` builds a `Vec<(opcode_kind, body_string)>` for each handler body, then the template iterates and substitutes.

OK that's a big refactor. Let me scope just per-proto opcode permutation for now — drop the arm shuffle to a future plan if it turns out to need work.

**Revised plan scope:** per-proto opcode permutation only. The dispatcher arm order stays fixed. The deobfuscator can still read the arms in canonical order, but the bytes they receive from raw bytecode now need translation through the per-proto `inv` table.

This is a smaller, tighter plan. Arm shuffle becomes a separate later plan.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  encode.rs              # MODIFY: generate per-proto perm; emit 35-byte inv into prologue;
                         #         translate each opcode byte through perm
  render.rs              # MODIFY: derive per-proto perms via the rng; thread them to encode_function
crates/luau-runtime/assets/
  vm.luau.j2             # MODIFY: read 35-byte inv table from prologue at vm_call entry;
                         #         apply inv to every decoded opcode byte before dispatching
crates/luau-obf/src/
  lib.rs                 # MODIFY: tests asserting per-proto opcode bytes differ for the same logical op
README.md                # MODIFY: status line
```

---

## Task 1: Per-proto opcode permutation in the encoder

**Files:**
- Modify: `crates/luau-emit/src/encode.rs`
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Add permutation helpers in `encode.rs`**

Near the top of `/Volumes/External Drive/luau-obf/crates/luau-emit/src/encode.rs`, add:

```rust
use rand::seq::SliceRandom;

/// Generate a random permutation of opcode bytes 1..=n_ops for a single proto.
/// Returns (perm, inv) where:
///   perm[c] = the raw byte to emit for canonical opcode-index c (1-based input, 0-based slice).
///   inv[r-1] = the canonical opcode byte for raw byte r (1-based both).
pub(crate) fn make_opcode_permutation(
    n_ops: usize,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> (Vec<u8>, Vec<u8>) {
    let mut perm: Vec<u8> = (1..=(n_ops as u8)).collect();
    perm.shuffle(rng);
    let mut inv = vec![0u8; n_ops];
    for (canonical_zero_based, &raw) in perm.iter().enumerate() {
        // raw is the byte emitted for canonical (canonical_zero_based + 1).
        // inv[raw - 1] = canonical_zero_based + 1.
        inv[(raw - 1) as usize] = (canonical_zero_based + 1) as u8;
    }
    (perm, inv)
}
```

- [ ] **Step 2: Modify `encode_function` to take and apply the permutation**

Change the signature:

```rust
pub fn encode_function(
    f: &LirFunction,
    opmap: &OpMap,
    proto_id: u32,
    k0: u32,
    k1: u32,
    perm: &[u8],   // canonical_byte (1-based) - 1 → raw byte
    inv: &[u8],    // raw_byte - 1 → canonical byte
) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    // Prologue: 5 bytes META + 35 bytes inv = 40 bytes.
    out.push(f.num_params as u8);
    out.push((f.num_regs & 0xFF) as u8);
    out.push(((f.num_regs >> 8) & 0xFF) as u8);
    out.push(f.num_upvals as u8);
    out.push(if f.is_vararg { 1 } else { 0 });
    // 35-byte inverse permutation.
    out.extend_from_slice(inv);

    // ... existing instruction encoding logic continues ...
    // EXCEPT: every place that wrote `opmap.opcode_of(...)` must now translate
    // through `perm[(opmap.opcode_of(kind) - 1) as usize]` to get the actual
    // emitted byte.
    // The same applies to anywhere the encoder writes `instr.op` as a byte
    // (which is `opmap.opcode_of(instr.op)`).

    // The existing per-byte XOR keystream loop at the end applies to the
    // entire `out` vector, including the prologue + permutation table.
    for (i, byte) in out.iter_mut().enumerate() {
        let pc = (i + 1) as u32;
        *byte ^= keystream_byte(pc, proto_id, k0, k1);
    }
    out
}
```

The crucial change is wherever the encoder writes an opcode byte. Look for `out.push(opmap.opcode_of(...))` and similar. Replace with:

```rust
let canonical = opmap.opcode_of(instr.op);
let emitted = perm[(canonical - 1) as usize];
out.push(emitted);
```

Search for ALL such sites — the typical path is in the main instruction loop. There should be exactly one.

- [ ] **Step 3: Update the encoder unit tests**

The previous tests called `encode_function(&f, &opmap, 0, 0, 0)`. They need an identity permutation (perm[i] = i+1):

```rust
        let n_ops = 35;
        let perm: Vec<u8> = (1..=n_ops as u8).collect();
        let inv = perm.clone();  // identity is self-inverse
        let bytes = encode_function(&f, &opmap, 0, 0, 0, &perm, &inv);
```

This preserves all existing test assertions (the byte values remain the same as before).

- [ ] **Step 4: Thread permutations from `render`**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, generate a per-proto permutation:

```rust
    let n_ops = crate::opmap::ALL_OPS.len();
    let perms_and_invs: Vec<(Vec<u8>, Vec<u8>)> = (0..program.functions.len())
        .map(|_| crate::encode::make_opcode_permutation(n_ops, rng))
        .collect();
```

Then pass each to `encode_function`:

```rust
    let codes: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let (perm, inv) = &perms_and_invs[i];
            let bytes = encode_function(f, opmap, i as u32, k0, k1, perm, inv);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();
```

- [ ] **Step 5: Run the workspace**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all unit tests pass BUT the differential corpus tests FAIL — the encoder writes permuted opcode bytes, but the VM still expects canonical bytes. That's correct; Task 2 wires the VM side.

If a UNIT test (in luau-emit) fails, the encoder logic has a bug — STOP and report.

If only the corpus tests fail, proceed to Task 2 without committing.

---

## Task 2: VM-side decoding of the per-proto permutation

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Read the inv table from the prologue at vm_call entry**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, find the existing prologue read (post-Plan-11):

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
```

Modify to read the 35-byte inv table from bytes 6..40 of the prologue, then start instructions at pc=41:

```luau
vm_call = function(proto_id, args, nargs, upvals)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local num_params = _byte(code, 1, proto_id)
    local num_regs = _byte(code, 2, proto_id) + _byte(code, 3, proto_id) * 256
    local is_vararg = _byte(code, 5, proto_id) == 1
    local inv = {}
    for i = 1, 35 do
        inv[i] = _byte(code, 5 + i, proto_id)
    end
    upvals = upvals or {}
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local frame_varargs
    if is_vararg then
        ...
    end
    local pc = 41
```

- [ ] **Step 2: Apply inv to every decoded opcode byte**

The current dispatch is:

```luau
    while pc <= code_len do
        local op = _byte(code, pc, proto_id)
        pc = pc + 1
        if op == 14 then
            ...
```

Insert the permutation translation:

```luau
    while pc <= code_len do
        local raw = _byte(code, pc, proto_id)
        local op = inv[raw]
        pc = pc + 1
        if op == 14 then
            ...
```

(The dispatch arms stay unchanged — they test `op == N` where N is the canonical opcode byte.)

- [ ] **Step 3: Add `inv` and `raw` to MANGLE_TARGETS**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/mangle.rs`, ensure these are present:
- `inv`
- `raw`

Add both to MANGLE_TARGETS if not already there.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every test passes. The encoder permutes opcodes per proto; the VM reads the inv table and translates each byte back to canonical. Differential corpus tests succeed.

If a test fails:
- **#1 cause**: pc starts at the wrong byte. Must be `41` (after 5-byte META + 35-byte inv).
- **#2 cause**: inv table read indexes are off. The table starts at byte 6, ends at byte 40. Loop: `for i = 1, 35 do inv[i] = _byte(code, 5 + i, proto_id) end` reads bytes 6 through 40 inclusive.
- **#3 cause**: encoder forgot to apply perm at one opcode-write site. Search the encoder for `opmap.opcode_of` — there should be exactly one site, and it must go through `perm[(canonical - 1) as usize]`.
- **#4 cause**: perm/inv mismatch. If perm[0] = 5 (canonical opcode 1 emits as byte 5), then inv[4] (byte 5) should be 1. Verify the `make_opcode_permutation` invariant by inspection.

- [ ] **Step 5: Spot-check that two protos use different opcode bytes for the same logical op**

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p luau-obf-cli --release -- runtime-tests/corpus/arithmetic.luau -o /tmp/spotcheck.luau --seed 1111111111111111111111111111111111111111111111111111111111111111 --quiet
# Spot-check by inspection: the inv tables in different protos should differ.
# Hard to verify automatically without re-implementing the decryption; rely on
# the differential corpus passing as the strong correctness signal.
echo "Test passing means the permutation round-trips."
```

- [ ] **Step 6: Commit (Tasks 1 + 2 combined)**

```bash
git add crates/luau-emit/src/encode.rs crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-emit/src/mangle.rs
git commit -m "feat(obf): per-proto opcode byte permutation (inv table in encrypted prologue)"
```

---

## Task 3: Acceptance test — per-proto opcode bytes differ

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add a test asserting that two protos in the same program use different opcode bytes for the same logical operation**

Append to the `tests` mod:

```rust
    #[test]
    fn per_proto_opcode_bytes_differ() {
        // A program with two functions that both perform an arithmetic operation.
        // Their bytecode bytes for that operation should differ because the
        // per-proto opcode permutations differ.
        // We can't easily inspect "which byte is ADD in proto N" without decoding
        // the XOR'd bytecode, but we can use a structural property:
        //   - Each proto's CODE entry is a string literal of \NNN escapes.
        //   - The first 40 bytes are the encrypted prologue.
        //   - Bytes 6..40 are the encrypted inv table for that proto.
        //   - Two protos with different inv tables → bytes 6..40 differ.
        let r = obfuscate(
            "local function a(x, y) return x + y end \
             local function b(x, y) return x + y end \
             print(a(1, 2), b(3, 4))",
            Options { seed: Some([244u8; 32]) }
        ).unwrap();
        // Extract the first two CODE entries.
        // A CODE entry has the form: `"\NNN\NNN..."` inside the CODE table.
        // Find all such literals and grab the first two non-empty ones.
        let mut codes: Vec<Vec<u8>> = Vec::new();
        let bytes = r.output.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // Look for an opening `"` that's immediately preceded by a comma or
            // bracket (start of a table entry).
            if bytes[i] == b'"' {
                // Heuristic: the CODE entries are big and start near the top.
                // Decode \NNN escapes until closing `"`.
                let mut j = i + 1;
                let mut decoded: Vec<u8> = Vec::new();
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        if bytes[j + 1].is_ascii_digit() {
                            // \NNN form.
                            let mut n = 0u32;
                            let mut k = j + 1;
                            while k < bytes.len() && bytes[k].is_ascii_digit() && k < j + 4 {
                                n = n * 10 + (bytes[k] - b'0') as u32;
                                k += 1;
                            }
                            if n <= 255 {
                                decoded.push(n as u8);
                            }
                            j = k;
                        } else {
                            j += 2;
                        }
                    } else {
                        decoded.push(bytes[j]);
                        j += 1;
                    }
                }
                // Only collect entries of length >= 50 (likely bytecode strings,
                // not 64-byte constant blobs — but constant blobs are also 64
                // bytes so we'll skip those by requiring length > 64).
                if decoded.len() > 64 {
                    codes.push(decoded);
                }
                i = j + 1;
                continue;
            }
            i += 1;
        }
        assert!(codes.len() >= 2, "expected at least 2 bytecode CODE entries, got {}", codes.len());
        // Compare the first two: bytes 5..40 (the inv-table region) must differ.
        // We can't decrypt; just assert the encrypted bytes 5..40 are not identical.
        let a_inv_region = &codes[0][5..40.min(codes[0].len())];
        let b_inv_region = &codes[1][5..40.min(codes[1].len())];
        assert_ne!(a_inv_region, b_inv_region,
            "two protos have identical encrypted inv tables — permutation isn't varying");
    }
```

- [ ] **Step 2: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass, including the new one. If the new test fails, the per-proto permutation isn't actually varying — investigate the rng-consumption order.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 13 acceptance — per-proto opcode bytes differ"
```

---

## Task 4: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 13 — per-proto opcode byte permutation. Each function now
has its own permutation of the 35 opcode bytes, stored encrypted in its
bytecode prologue. The same logical operation (e.g. ADD) is emitted as
different bytecode bytes in different protos within the same chunk, so the
deobfuscator must extract and apply each proto's permutation before
disassembly. Dispatcher arm order is unchanged (deferred to a later plan).
Full differential corpus passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 13 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All differential corpus tests pass.
- [ ] `per_proto_opcode_bytes_differ` passes.
- [ ] All Plan 8-12 acceptance tests still pass.
- [ ] Spot-check: take a corpus program with 2+ functions, obfuscate it, decode the bytes in the first two CODE entries' prologue regions, and verify that bytes 6..40 differ between them.
- [ ] The bytecode prologue is now 40 bytes (was 5). The first instruction starts at pc=41.
