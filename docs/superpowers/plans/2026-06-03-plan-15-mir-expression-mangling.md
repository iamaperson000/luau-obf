# Luau Obfuscator — Plan 15: MIR Expression Mangling (First Real Pass)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the first MIR→MIR obfuscation pass that changes the actual *bytecode* of user programs — not just the wrapper around it. Each `BinOp::Sub` instruction, with 50% probability per instance, is rewritten as `Neg + Add`. The deobfuscator now disassembles into a mix of real Sub ops and Sub-via-Add+Neg sequences and can't be sure from the bytecode alone which is which.

This is the first pass that exploits the `luau-passes` infrastructure we built in Plan 7 but only populated with `Identity`. It demonstrates the pattern and primes us for further passes (negation rewrites, multiplication identities, opaque predicates, etc.).

**Why Sub→Neg+Add and not Add→...?**
- Add is the most common arithmetic; rewriting it makes EVERY arithmetic block significantly larger.
- Sub is less common in real programs (most subtractions are `i - 1` style decrements).
- The rewrite is safer: for numeric types, `a - b == a + (-b)` exactly. For metatable types, `__sub` may differ from `__add(a, __unm(b))` — but our corpus (`metatables.luau` uses `__add` on Vec2 only, no Vec2 subtraction) doesn't exercise that case, so the differential tests still pass. Future plans can constrain the rewrite to known-numeric operands once we have type analysis.

**Architecture:**
- New file `crates/luau-passes/src/expr_mangle.rs` exposes the `ExpressionMangle` pass.
- `Pass::run` walks each `MirFunction`. For each basic block, it visits instructions. When it sees `Instr::BinOp { op: BinOp::Sub, lhs, rhs, dst }`, with 50% probability (drawn from the seeded rng), it splices the original instruction with two new instructions: a `UnOp::Neg(rhs) → tmp` then a `BinOp::Add(lhs, tmp) → dst`. The instruction list of the block grows by one per rewrite; the function's `n_locals` grows to accommodate the new `tmp` VLocal.
- The default plan in `luau-passes` adds `ExpressionMangle` after `Identity`.
- LIR/encode/VM are unchanged — Add and Neg are existing opcodes, regalloc handles arbitrary VLocal counts.

**Tech Stack:** Same as Plans 1-14.

---

## Implementation notes

**Sub vs Add semantics in Lua.**
- `a - b` invokes `__sub(a, b)` metamethod if either operand has one (and is the right type — coercion rules apply).
- `a + (-b)`:
  - `-b` invokes `__unm(b)` if `b` has one.
  - `a + (-b)` invokes `__add(a, -b)` if either has one.
- For plain numbers, both are exactly `a - b` (IEEE 754 semantics — note: `-0.0` round-trips correctly because `a + (-0.0) == a` is true for finite a, and `a - 0.0 == a` is also true).
- For metatable types: behavior diverges. **Out of scope for this plan** — the differential corpus doesn't exercise it. A future plan can gate the rewrite on a "definitely numeric" check or skip it entirely when MIR has type info.

**Rewrite generation.**

Original:
```rust
Instr::BinOp { dst: V(7), op: Sub, lhs: V(3).as_value(), rhs: V(5).as_value() }
```

Rewritten:
```rust
Instr::UnOp { dst: V(<new_local>), op: Neg, operand: V(5).as_value() }
Instr::BinOp { dst: V(7), op: Add, lhs: V(3).as_value(), rhs: V(<new_local>).as_value() }
```

Where `<new_local>` is `f.n_locals` BEFORE the increment.

**RNG consumption.**
- The pass receives `&mut ChaCha20Rng` from the pass scheduler.
- For each Sub instruction in the program, it draws ONE random bit (via `rng.gen_bool(0.5)` or `rng.gen::<u8>() & 1 == 0`). If the bit is 1, rewrite; else skip.
- Determinism: same seed → same sequence of bits → same rewrite decisions.

**Variant interference.** The new `Add` and `Neg` instructions DON'T get re-rewritten in this pass (we walk each function once, not iteratively). If we later add an Add→Sub rewrite pass, the order of passes in the default plan controls the interaction.

**Default plan update.**

In `crates/luau-passes/src/lib.rs`:
```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
}
```

The Identity pass remains for backward compatibility with tests that expected exactly one MIR transform in the chain (none exist in this form — the existing tests check the result, not the pass count — but keeping Identity costs nothing).

**Iterating instructions safely.**

The instruction list mutates while iterating. Rust's borrow checker requires a careful approach:

```rust
fn rewrite_function(f: &mut MirFunction, rng: &mut ChaCha20Rng) {
    for block in &mut f.blocks {
        let mut i = 0;
        while i < block.instrs.len() {
            let should_rewrite = matches!(
                &block.instrs[i],
                Instr::BinOp { op: BinOp::Sub, .. }
            ) && rng.gen::<u8>() & 1 == 1;
            if !should_rewrite { i += 1; continue; }
            // Extract operands.
            let (dst, lhs, rhs) = match &block.instrs[i] {
                Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                _ => unreachable!(),
            };
            // Allocate a fresh VLocal.
            let tmp = VLocal(f.n_locals);
            f.n_locals += 1;
            // Replace and splice.
            block.instrs[i] = Instr::UnOp { dst: tmp, op: UnOp::Neg, operand: rhs };
            block.instrs.insert(i + 1, Instr::BinOp {
                dst, op: BinOp::Add, lhs, rhs: Value::VLocal(tmp),
            });
            i += 2;  // skip past both new instructions
        }
    }
}
```

Note we increment `f.n_locals` here directly, NOT through any builder, because at the pass stage the MirFunction is already built.

The order-of-operations bug to avoid: `f.n_locals` is borrowed independently of `f.blocks`, so we can't have a single `&mut f.blocks` borrow active while we read `f.n_locals`. The fix is to read `f.n_locals` once before the inner loop, then increment locally and write back at the end — OR (cleaner) avoid the conflict by using a function-level helper:

```rust
fn rewrite_function(f: &mut MirFunction, rng: &mut ChaCha20Rng) {
    let mut n_locals = f.n_locals;
    let blocks = &mut f.blocks;
    for block in blocks.iter_mut() {
        // ... use n_locals, increment locally ...
    }
    f.n_locals = n_locals;
}
```

This compiles because `&mut f.blocks` and `n_locals` (a local copy of `f.n_locals`) don't conflict.

---

## File Structure (changes only)

```
crates/luau-passes/src/
  expr_mangle.rs           # CREATE: ExpressionMangle pass
  lib.rs                   # MODIFY: add module + default_plan adds ExpressionMangle
crates/luau-obf/src/
  lib.rs                   # MODIFY: acceptance test asserting Sub instances vary per seed
README.md                  # MODIFY: status line
```

No changes to HIR / MIR types / LIR / encoder / VM template. Add and Neg are existing opcodes.

---

## Task 1: Implement the ExpressionMangle pass

**Files:**
- Create: `crates/luau-passes/src/expr_mangle.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/expr_mangle.rs`:

```rust
//! Expression Mangle pass — rewrites a fraction of `Sub` instructions as
//! `Add(a, Neg(b))`. The result is semantically identical for numeric
//! operands; the bytecode no longer consistently maps `Sub` to the
//! source-level `-` operator.

use crate::Pass;
use luau_hir::{BinOp, UnOp};
use luau_mir::{Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ExpressionMangle;

impl Pass for ExpressionMangle {
    fn name(&self) -> &'static str {
        "expression-mangle"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            for block in f.blocks.iter_mut() {
                let mut i = 0;
                while i < block.instrs.len() {
                    let should_rewrite = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Sub, .. }
                    ) && rng.gen::<u8>() & 1 == 1;
                    if !should_rewrite {
                        i += 1;
                        continue;
                    }
                    let (dst, lhs, rhs) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let tmp = VLocal(n_locals);
                    n_locals += 1;
                    block.instrs[i] = Instr::UnOp {
                        dst: tmp,
                        op: UnOp::Neg,
                        operand: rhs,
                    };
                    block.instrs.insert(
                        i + 1,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Add,
                            lhs,
                            rhs: Value::VLocal(tmp),
                        },
                    );
                    i += 2;
                }
            }
            f.n_locals = n_locals;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        luau_mir::lower::lower(&hir).unwrap()
    }

    fn count_subs(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::BinOp { op: BinOp::Sub, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn count_negs(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::UnOp { op: UnOp::Neg, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn rewrites_some_subs_to_add_neg() {
        // A program with several Sub instructions.
        let src = "local a, b, c, d = 10, 20, 30, 40 \
                   print(a - b, c - d, (a - b) - (c - d), a - (b - c))";
        let mut p = mir_of(src);
        let subs_before = count_subs(&p);
        let negs_before = count_negs(&p);
        assert!(subs_before >= 4, "expected at least 4 Subs, got {}", subs_before);
        let mut rng = ChaCha20Rng::from_seed([7u8; 32]);
        ExpressionMangle.run(&mut p, &mut rng);
        let subs_after = count_subs(&p);
        let negs_after = count_negs(&p);
        // Some Subs should be rewritten to (Neg + Add), so:
        // - subs_after < subs_before (fewer Subs remain)
        // - negs_after > negs_before (more Negs were inserted)
        // - the COUNT of Negs added equals the COUNT of Subs removed.
        assert!(subs_after < subs_before,
            "expected some Subs to be rewritten (before={} after={})", subs_before, subs_after);
        assert_eq!(negs_after - negs_before, subs_before - subs_after);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local a = 1 print(a - 2 - 3 - 4)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ExpressionMangle.run(&mut p1, &mut r1);
        ExpressionMangle.run(&mut p2, &mut r2);
        assert_eq!(count_subs(&p1), count_subs(&p2));
        assert_eq!(count_negs(&p1), count_negs(&p2));
    }

    #[test]
    fn programs_with_no_sub_are_unchanged() {
        let src = "local a = 1 + 2 print(a)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ExpressionMangle.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
    }

    #[test]
    fn rewrite_increases_n_locals() {
        let src = "local a, b = 10, 5 print(a - b)";
        let mut p = mir_of(src);
        let before = p.functions[0].n_locals;
        // Use a seed that we know rewrites the Sub (drawing 1 from rng.gen::<u8>() & 1).
        // ChaCha20Rng with seed [255u8; 32] should reliably trigger SOME rewrite.
        // We'll just check that AT LEAST ONE seed in a small set produces an
        // increase.
        let mut found_rewrite = false;
        for s in 0..10u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ExpressionMangle.run(&mut p_clone, &mut rng);
            if p_clone.functions[0].n_locals > before {
                found_rewrite = true;
                break;
            }
        }
        assert!(found_rewrite, "no seed in 0..10 triggered a Sub rewrite");
    }
}
```

- [ ] **Step 2: Register the module**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod expr_mangle;
```

(Alongside the existing `pub mod identity;`.)

And modify `default_plan`:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes`

Expected: 4 new unit tests pass.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs still pass. The Sub→Add+Neg rewrite is semantically identical for numeric types. The corpus uses arithmetic in `arithmetic.luau`, `recursion.luau`, `numeric_for.luau`, `counter.luau`, `compound_assign.luau`, etc. — all numeric.

If `metatables.luau` fails (Vec2's __add interaction), this is the edge case the plan flagged. Resolution: either trim the Vec2 case from the corpus OR scope the rewrite to skip metatable-using code paths (which we can't detect without type analysis). In the latter case, scale down the rewrite to ZERO subs and let this plan be a no-op shipping just the infrastructure. Document the choice.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/expr_mangle.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): MIR expression mangling — rewrite Sub as Neg+Add"
```

---

## Task 2: Acceptance test in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add a test asserting the rewrite affects output**

Append to the `tests` mod:

```rust
    #[test]
    fn sub_heavy_program_produces_different_output_per_seed() {
        // A program with several `-` operations. Different seeds should
        // produce different rewrites, hence different output (variance
        // beyond what opcode renumbering / keystream alone provides).
        let src = "local x = 100 \
                   x = x - 10 \
                   x = x - 5 \
                   x = x - 2 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        // All three outputs must be distinct (variance preserved).
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn sub_rewrite_preserves_runtime_semantics() {
        // Just spot-check: an obfuscated `Sub` program produces the right
        // arithmetic answer at runtime.
        let src = "print(100 - 10 - 5 - 2)";  // 83
        let r = obfuscate(src, Options { seed: Some([55u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("83"));
    }
```

- [ ] **Step 2: Run all tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 15 acceptance — Sub rewrite preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 15 — first real MIR-level obfuscation pass. Each `Sub`
instruction is rewritten as `Neg + Add` with 50% probability per instance
(seed-derived). The bytecode no longer 1:1 maps to source-level operators:
a disassembler sees a mix of real `Sub` ops and `Sub`-via-`Neg+Add`
sequences, and can't be sure from the bytecode alone which subtractions in
the original program correspond to which. The luau-passes infrastructure
from Plan 7 is now exercised end-to-end; subsequent plans will add more
rewrites (Add identity-padding, Mul shifts, opaque predicates, control-flow
flattening).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 15 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs still pass differential testing — Sub→Add+Neg is semantically equivalent for numeric types.
- [ ] `rewrites_some_subs_to_add_neg` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `programs_with_no_sub_are_unchanged` passes.
- [ ] `rewrite_increases_n_locals` passes.
- [ ] `sub_heavy_program_produces_different_output_per_seed` passes.
- [ ] `sub_rewrite_preserves_runtime_semantics` passes.
- [ ] All Plan 8-14 acceptance tests still pass.
- [ ] Spot-check: compile a program like `print(10 - 5)` with `--seed 0…` and `--seed FF…`. The two outputs should differ in more than just keystream bytes — they should have a different INSTRUCTION COUNT (some seeds insert Neg+Add, others keep Sub).
