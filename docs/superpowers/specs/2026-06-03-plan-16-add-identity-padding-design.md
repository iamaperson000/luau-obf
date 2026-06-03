# Plan 16 — Add Identity-Padding (Design)

**Date:** 2026-06-03
**Status:** Approved for implementation planning

## Goal

Land the second MIR-level obfuscation pass: `AddIdentityPadding`. For each
`BinOp::Add` instruction in the program, with 30% probability per instance
(seed-derived), rewrite

```
dst = Add(a, b)
```

as the four-instruction sequence

```
kc   = LoadConst(<fresh integer k in [1, 65535]>)
tmpL = Add(a, kc)
tmpR = Sub(b, kc)
dst  = Add(tmpL, tmpR)
```

Plan 15 broke the 1:1 mapping of `Sub`-in-bytecode ↔ `-`-in-source. Plan 16
does the same for `Add`. Together they ensure neither additive operator
maps cleanly to a single instruction in the obfuscated bytecode, and the
choice of which Adds are real vs. padded varies per build.

## Non-goals

- Type-aware rewriting. The rewrite is applied indiscriminately to any
  `BinOp::Add`. For numeric operands the identity is exact (modulo FP
  associativity — see below); for metatable operands behavior may diverge.
  Same pragma as Plan 15.
- Bit-exact IEEE 754 semantics for all operand ranges. With integer k in
  [1, 65535] and operands inside the integer range, the rewrite is exact.
  Pathological floats are out of scope.
- Iterating the pass to a fixed point. Single forward walk per function;
  Adds the pass synthesises are not re-rewritten in the same pass.

## Architecture

**New file:** `crates/luau-passes/src/add_padding.rs` exposing
`pub struct AddIdentityPadding;` with `impl Pass`.

**Modified file:** `crates/luau-passes/src/lib.rs`:
- `pub mod add_padding;`
- `default_plan()` gains a third pass after `ExpressionMangle`:

  ```rust
  pub fn default_plan() -> PassPlan {
      PassPlan::new()
          .push(Box::new(identity::Identity))
          .push(Box::new(expr_mangle::ExpressionMangle))
          .push(Box::new(add_padding::AddIdentityPadding))
  }
  ```

**Acceptance tests** live in `crates/luau-obf/src/lib.rs` alongside Plan 15's,
using `obfuscate(src, Options{seed})` and `luau` differential execution.

**No changes** to HIR / MIR types / LIR / encoder / VM template / constants
pipeline. `Add`, `Sub`, `LoadConst` are existing opcodes; `Constant::Number`
is the existing constant type.

## Pass mechanics

Per `MirFunction`:

1. Take a local copy of `f.n_locals` and `f.consts.len()` to track allocations.
2. For each `BasicBlock`, walk `instrs` with an index `i` (the list mutates,
   so a `while i < len()` loop is required).
3. For each `Instr::BinOp { op: Add, .. }`, draw `rng.gen::<u8>() % 100`.
   If `< 30`, rewrite; else `i += 1` and continue.
4. To rewrite:
   - Extract `(dst, a, b)` from the original Add.
   - Draw `k = rng.gen_range(1u32..=65535) as f64`.
   - Append `Constant::Number(k)` to `f.consts`; record its `ConstId`.
   - Allocate three fresh VLocals (`kc`, `tmpL`, `tmpR`) by bumping the
     local `n_locals` counter.
   - Replace `block.instrs[i]` with `Instr::LoadConst { dst: kc, src: <ConstId> }`.
   - Insert at `i+1`: `Instr::BinOp { dst: tmpL, op: Add, lhs: a, rhs: kc.into() }`.
   - Insert at `i+2`: `Instr::BinOp { dst: tmpR, op: Sub, lhs: b, rhs: kc.into() }`.
   - Insert at `i+3`: `Instr::BinOp { dst, op: Add, lhs: tmpL.into(), rhs: tmpR.into() }`.
   - `i += 4` (skip past all four new instructions).
5. After the function's blocks are processed, write `n_locals` back to
   `f.n_locals`.

The same Rust borrow-checker pattern as Plan 15 — copy `n_locals` to a local,
mutate `f.blocks`, write back — applies.

## Floating-point semantics

`(a + k) + (b - k) == a + b` is exact in IEEE 754 when no intermediate value
loses precision. With integer `k ∈ [1, 65535]` and operands in the integer
range (which covers all 23 corpus programs), this holds. For wildly large
floats — e.g. `a = 1e20, b = -1e20, k = 1` — `(1e20 + 1) + (-1e20 - 1) == 0`
exactly, no drift. For programs that lie outside that range, the rewrite
introduces sub-ULP drift that Luau's default `%g` print formatting hides.

Differential testing the existing corpus is the truth: if any program fails,
narrow the rewrite criteria. We don't anticipate failures on the current
corpus.

## RNG consumption

The pass consumes one `u8` (probability draw) per Add visited, plus one
`u32` (k draw) per Add actually rewritten. Same seed → same decisions →
same output. Determinism is the same property Plan 15 guarantees.

## Interaction with previous plans

- **Plan 15 (Sub→Neg+Add).** Plan 15 runs first and may have produced
  additional `Add` instructions from original `Sub`s. Plan 16 sees those
  Adds and may pad them — compounding obfuscation on original Subs.
- **Plans 11/12 (constant encryption / uniform blobs).** New `Constant::Number`
  entries flow through the same encryption pipeline. Each rewrite adds one
  encrypted constant blob to the output. No special handling needed.
- **Plans 13/14 (per-proto opcode permutation, stage-0 bootstrap).** Pass
  output is opaque to these; they operate on already-emitted bytecode.

## Testing strategy

### Unit tests (in `add_padding.rs`)

1. `rewrites_some_adds_to_padded_form`: program with ≥10 Adds. After pass,
   count of Adds increases (each rewrite turns 1 Add into 3), count of Subs
   increases (each rewrite adds 1), count of numeric `Constant::Number`s in
   `f.consts` grows by the number of rewrites.
2. `deterministic_for_same_seed`: two runs with seed `[42; 32]` produce
   identical n_locals, instruction sequences, and consts.
3. `programs_with_no_add_are_unchanged`: a program containing only Sub /
   Mul leaves n_locals and consts at their original values.
4. `rewrite_growth_invariants`: for a known seed that rewrites at least one
   Add, n_locals grows by exactly 3 × rewrite count, instr count grows by
   exactly 3 × rewrite count, consts grow by exactly rewrite count.

### Acceptance tests (in `crates/luau-obf/src/lib.rs`)

1. `add_padding_changes_output`: a 5-Add program obfuscated under three
   different seeds yields three distinct outputs.
2. `add_padding_preserves_runtime_semantics`: `print(100 + 200 + 300)` →
   obfuscated → executed via `luau` CLI → stdout contains "600".

### Existing tests

All 23 corpus differential tests must continue to pass — `+` is exercised
heavily across `arithmetic.luau`, `numeric_for.luau`, `counter.luau`,
`compound_assign.luau`, etc.

## File structure summary

```
crates/luau-passes/src/
  add_padding.rs       # CREATE: AddIdentityPadding pass + unit tests
  lib.rs               # MODIFY: pub mod, default_plan adds the pass
crates/luau-obf/src/
  lib.rs               # MODIFY: two new acceptance tests
README.md              # MODIFY: status line
```

## Open questions

None. All design decisions are locked.
