# Luau Obfuscator — Plan 19: Constant Decomposition (LoadConst Rewriting)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Land a fourth MIR pass that mangles **constants themselves** before any arithmetic passes get to see them. For each `Instr::LoadConst { dst, src }` where `consts[src]` is a finite `Constant::Number(n)` with `|n| < 2^31`, with 30% probability:

```
dst = LoadConst(n)
```

becomes

```
tmpA = LoadConst(n + δ)
tmpB = LoadConst(δ)
dst  = Sub(tmpA, tmpB)
```

where `δ` is a seed-derived integer in `[1, 65535]`. The number `n` no longer appears anywhere in the constant pool. After Plans 11 and 12 (uniform encrypted blobs), the two new constants are indistinguishable from any other; a static analyzer sees a `Sub` of two encrypted blobs and cannot guess what the original literal was without simulating the subtraction.

**Why this is safe:**
- For finite `n` with `|n| < 2^31` and integer `δ ∈ [1, 65535]`, `n + δ` is exactly representable in f64 and so is `δ`. The subtraction `(n + δ) − δ` is bit-exact in IEEE 754 (Sterbenz's lemma — operands within a factor of 2, but more strongly: `(n + δ) − δ` is exact whenever `(n + δ)` doesn't lose precision, which it doesn't for `|n + δ| < 2^53`).
- The new constants flow through Plan 11/12's encryption pipeline like any other `Constant::Number`. No special handling required downstream.
- The rewrite runs BEFORE the arithmetic-mangling passes (Plans 15-17), so the new `Sub` instruction it inserts becomes a candidate for `Sub → Neg + Add` (Plan 15) and subsequently for `Add` operand-padding (Plan 16). One original constant can end up as a multi-instruction synthetic chain.

**Architecture:**
- New file `crates/luau-passes/src/const_decompose.rs` exposes `pub struct ConstantDecompose;` with `impl Pass`.
- `default_plan` inserts `ConstantDecompose` at position 2 (right after `Identity`, before `ExpressionMangle`).
- No new types; reuses `Constant::Number`, `Instr::LoadConst`, `Instr::BinOp { op: Sub }`. No shared analyzer needed (operand types are guaranteed numeric — both operands are fresh `LoadConst Number`).

**Tech Stack:** Same as Plans 1-18.

---

## Implementation notes

**Range check.** Only rewrite when `consts[src]` is `Constant::Number(n)` AND `n.is_finite()` AND `n.abs() < (1u32 << 31) as f64`. Skip non-finite (NaN, ±Inf) and large-magnitude numbers to keep the subtraction bit-exact.

**RNG consumption.** One `u8` per LoadConst visited that COULD be rewritten (i.e. passes the range check). One `u32` (δ draw) per LoadConst actually rewritten. LoadConsts of non-numeric or out-of-range constants don't draw any RNG — they're skipped before the roll.

Wait — that introduces non-determinism risk if the const pool changes across runs. Actually no: the const pool is part of the program's state, and the order of LoadConst instructions is fixed by MIR lowering. Two runs with the same seed visit the same LoadConsts in the same order, and the range check (purely on the constant's value) yields the same answer. So determinism is preserved.

But to be safe and aligned with prior passes' style, **always roll the u8** (whether or not the range check passes). Skip if either the roll says no OR the range check fails. This way the RNG sequence is purely a function of the number of `LoadConst` instructions, independent of the constants' values.

**Walk semantics.** Single forward pass per block. The two new LoadConsts and one new Sub are skipped via `i += 3`. The new VLocals are added beyond `f.n_locals`.

**Effects on the numeric set analyzer (Plans 16/17).** Plan 19 runs BEFORE Plans 16/17, so it doesn't interact with their per-block dataflow. From Plan 16/17's perspective, the new Sub instruction's operands are two fresh `LoadConst Number` instructions, so the operands are provably numeric and the new Sub's dst is provably numeric. No problem.

**Default plan update.**

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  const_decompose.rs       # CREATE: ConstantDecompose pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass at position 2
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

No changes to MIR types, LIR, encoder, VM template.

---

## Task 1: Implement the ConstantDecompose pass

**Files:**
- Create: `crates/luau-passes/src/const_decompose.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/const_decompose.rs`:

```rust
//! Constant-Decomposition pass — rewrites a fraction of `LoadConst` of
//! finite, small-magnitude `Constant::Number(n)` instructions as
//! `LoadConst(n + δ); LoadConst(δ); Sub`, where δ is a seed-derived
//! integer in [1, 65535]. The original literal disappears from the
//! constant pool, replaced by two seed-derived constants whose runtime
//! difference reproduces it. Bit-exact for finite `|n| < 2^31`.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ConstantDecompose;

const REWRITE_PERCENT: u8 = 30;
const DELTA_MIN: u32 = 1;
const DELTA_MAX: u32 = 65535;
const ABS_MAX_N: f64 = (1u64 << 31) as f64; // 2^31

fn n_safe_to_decompose(n: f64) -> bool {
    n.is_finite() && n.abs() < ABS_MAX_N
}

impl Pass for ConstantDecompose {
    fn name(&self) -> &'static str {
        "constant-decompose"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_load = matches!(&block.instrs[i], Instr::LoadConst { .. });
                    if !is_load {
                        i += 1;
                        continue;
                    }
                    // Always draw the u8 for deterministic RNG advancement.
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, src_id) = match &block.instrs[i] {
                        Instr::LoadConst { dst, src } => (*dst, *src),
                        _ => unreachable!(),
                    };
                    let n = match consts.get(src_id.0 as usize) {
                        Some(Constant::Number(n)) => *n,
                        _ => {
                            i += 1;
                            continue;
                        }
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    if !want_rewrite || !n_safe_to_decompose(n) {
                        i += 1;
                        continue;
                    }
                    let delta = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                    let k1 = n + delta;
                    let k2 = delta;
                    let c1_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k1));
                    let c2_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k2));
                    let tmp_a = VLocal(n_locals);
                    let tmp_b = VLocal(n_locals + 1);
                    n_locals += 2;
                    block.instrs[i] = Instr::LoadConst { dst: tmp_a, src: c1_id };
                    block.instrs.insert(
                        i + 1,
                        Instr::LoadConst { dst: tmp_b, src: c2_id },
                    );
                    block.instrs.insert(
                        i + 2,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Sub,
                            lhs: Value::VLocal(tmp_a),
                            rhs: Value::VLocal(tmp_b),
                        },
                    );
                    i += 3;
                }
            }
            f.n_locals = n_locals;
            f.consts = consts;
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

    fn count_loadconsts(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::LoadConst { .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn count_number_consts(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for c in &f.consts {
                if matches!(c, Constant::Number(_)) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn rewrites_some_numeric_loadconsts() {
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a, b, c, d, e, 6, 7, 8, 9, 10, 11, 12)";
        let p = mir_of(src);
        let lc_before = count_loadconsts(&p);
        let subs_before = count_subs(&p);
        let nums_before = count_number_consts(&p);
        assert!(lc_before >= 10, "expected ≥10 LoadConsts, got {}", lc_before);

        let mut saw_rewrite = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ConstantDecompose.run(&mut p_clone, &mut rng);
            let lc_after = count_loadconsts(&p_clone);
            let subs_after = count_subs(&p_clone);
            let nums_after = count_number_consts(&p_clone);
            // Each rewrite: -1 LoadConst replaced + 2 new = net +1 LoadConst,
            //               +1 new Sub, +2 new Number consts.
            let lc_growth = lc_after as i64 - lc_before as i64;
            let subs_growth = subs_after as i64 - subs_before as i64;
            let nums_growth = nums_after as i64 - nums_before as i64;
            assert_eq!(lc_growth, subs_growth, "seed {}: lc growth ({}) != subs growth ({})", s, lc_growth, subs_growth);
            assert_eq!(nums_growth, 2 * subs_growth, "seed {}: nums growth ({}) != 2 * subs ({})", s, nums_growth, subs_growth);
            if subs_growth > 0 {
                saw_rewrite = true;
            }
        }
        assert!(saw_rewrite, "no seed in 0..5 rewrote a LoadConst");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "print(1, 2, 3, 4, 5, 6, 7, 8)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ConstantDecompose.run(&mut p1, &mut r1);
        ConstantDecompose.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                }
                for c in &f.consts {
                    s.push_str(&format!("c: {:?}\n", c));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn skips_non_numeric_constants() {
        let src = "print('hi', true, nil)";
        let mut p = mir_of(src);
        let subs_before = count_subs(&p);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ConstantDecompose.run(&mut p, &mut rng);
        assert_eq!(count_subs(&p), subs_before, "non-numeric LoadConsts should not be decomposed");
    }

    #[test]
    fn skips_out_of_range_numbers() {
        // 2^32 is finite but outside our safety range.
        let big_n: f64 = (1u64 << 32) as f64;
        let src = format!("print({})", big_n);
        let mut p = mir_of(&src);
        let subs_before = count_subs(&p);
        // Run with a seed that would otherwise decompose small numbers.
        for s in 0..20u8 {
            let mut p_clone = mir_of(&src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ConstantDecompose.run(&mut p_clone, &mut rng);
            assert_eq!(
                count_subs(&p_clone),
                subs_before,
                "seed {}: out-of-range number was decomposed",
                s
            );
        }
    }

    #[test]
    fn programs_with_no_loadconst_are_unchanged() {
        // A program with no constants at all is rare; use one with only string
        // constants — Number ones are absent.
        let src = "local function f() end f()";
        let mut p = mir_of(src);
        let nlocals_before = p.functions[0].n_locals;
        let subs_before = count_subs(&p);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ConstantDecompose.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, nlocals_before);
        assert_eq!(count_subs(&p), subs_before);
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod const_decompose;
```

And update `default_plan` so `ConstantDecompose` runs at position 2, **after** `Identity` and **before** `ExpressionMangle`:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 5 new tests pass plus all existing tests.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed multi-seed differential. If `arithmetic.luau`, `numeric_for.luau`, `counter.luau` (constant-heavy) fail, the FP cancellation has lost precision somewhere — debug by inspecting which seed and which constant.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/const_decompose.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): MIR constant decomposition — LoadConst N → Sub(k+δ, δ)"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `const_decompose_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn const_decompose_changes_output() {
        let src = "print(123, 456, 789, 1000, 2025)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `const_decompose_preserves_semantics`**

```rust
    #[test]
    fn const_decompose_preserves_semantics() {
        // Sum of small constants — verify the output is exact across seeds.
        let src = "print(1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10)";  // 55
        for seed_byte in [11u8, 47, 99, 200] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("55"),
                "seed {}: expected '55' in stdout, got: {}",
                seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 19 acceptance — constant decompose preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 19 — constant decomposition. Each finite numeric `LoadConst`
in small-magnitude range is rewritten with 30% probability as
`LoadConst(n + δ); LoadConst(δ); Sub`, with δ a seed-derived integer in
[1, 65535]. The original literal is gone from the encrypted constant pool;
two seed-derived blobs replace it. Composes with Plans 15-17: the inserted
Sub becomes a candidate for `Sub → Neg + Add` rewriting, then for Add
operand-padding. One source-level constant can compile to a multi-instruction
synthetic chain per build.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 19 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed multi-seed harness.
- [ ] `rewrites_some_numeric_loadconsts` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `skips_non_numeric_constants` passes.
- [ ] `skips_out_of_range_numbers` passes.
- [ ] `programs_with_no_loadconst_are_unchanged` passes.
- [ ] `const_decompose_changes_output` passes.
- [ ] `const_decompose_preserves_semantics` passes (4 seeds, all produce "55").
- [ ] Spot-check: obfuscate `print(42)` under two different seeds; the output size should differ (some seeds decompose 42, others don't).
