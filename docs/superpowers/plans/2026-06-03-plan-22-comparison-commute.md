# Luau Obfuscator — Plan 22: Comparison Operator Commute

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Goal:** For each `BinOp` comparison instruction (`Lt`, `Le`, `Gt`, `Ge`, `Eq`, `Ne`), with 40% probability, commute operands and flip the operator. This produces bit-exact-equivalent semantics for ALL Luau values (including NaN), unlike Plans 16/17 which have operand-type constraints.

Commutations:
- `Lt(a, b)` → `Gt(b, a)` and vice versa.
- `Le(a, b)` → `Ge(b, a)` and vice versa.
- `Eq(a, b)` → `Eq(b, a)` (commute only — symmetric op).
- `Ne(a, b)` → `Ne(b, a)` (commute only).

After Plan 22, the obfuscated bytecode no longer maps comparison operands to source order, and `<` may appear as `>` (or `<=` as `>=`) in disassembly.

**Why this is safe:**
- `a < b` invokes `__lt` metamethod (or numeric/string less-than). `b > a` invokes the same comparison machinery (Luau spec: `a > b` is defined as `b < a`). Identical semantics for all types.
- `a <= b` ↔ `b >= a`: same reasoning. `__le` metamethod is invoked symmetrically by both forms.
- `a == b` ↔ `b == a` for all types (Eq is commutative in Lua; `__eq` is invoked on either operand symmetrically).
- `a ~= b` ↔ `b ~= a` (Ne is commutative).
- **NaN safety:** `NaN < x` is `false`, `x > NaN` is also `false` (Luau aligns with IEEE 754). Same answer regardless of commute direction.

**Architecture:**
- New file `crates/luau-passes/src/comparison_commute.rs` exposes `pub struct ComparisonCommute;`.
- Pass runs at default-plan position 7 (after `MulOperandScatter`, before `BranchPolarityFlip`). The comparison ops live in BinOp instructions throughout the program; this pass touches them after all arithmetic mangling has stabilized.
- No new types. Single-instruction in-place rewrite.

**Tech Stack:** Same as Plans 1-21.

---

## Implementation notes

**Rewrite logic.** For each `Instr::BinOp { dst, op, lhs, rhs }` where op is a comparison, with 40% probability:
- New op:
  - `Lt` ↔ `Gt`
  - `Le` ↔ `Ge`
  - `Eq` stays `Eq`
  - `Ne` stays `Ne`
- New lhs/rhs: swap them.

```rust
Instr::BinOp { dst, op: <new_op>, lhs: rhs, rhs: lhs }
```

**RNG consumption.** One `u8` per comparison BinOp visited. No further draws.

**Walk semantics.** Single linear scan per block. No new instructions inserted; the existing BinOp is mutated in place. `i += 1` always.

**Default plan update.**

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(opaque_predicate::OpaqueTruePredicate))
        .push(Box::new(junk_arith::JunkArithmetic))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(comparison_commute::ComparisonCommute))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  comparison_commute.rs    # CREATE: ComparisonCommute pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass at position 7
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

---

## Task 1: Implement the ComparisonCommute pass

**Files:**
- Create: `crates/luau-passes/src/comparison_commute.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/comparison_commute.rs`:

```rust
//! Comparison-Commute pass — rewrites a fraction of comparison BinOps by
//! swapping operands (and flipping the operator for non-symmetric ops):
//!
//!   Lt(a, b) → Gt(b, a)        Eq(a, b) → Eq(b, a)
//!   Gt(a, b) → Lt(b, a)        Ne(a, b) → Ne(b, a)
//!   Le(a, b) → Ge(b, a)
//!   Ge(a, b) → Le(b, a)
//!
//! Semantically identical for ALL Luau values (including NaN and metatable
//! `__lt` / `__le` / `__eq`). No operand-type gate required.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{Instr, MirProgram};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ComparisonCommute;

const COMMUTE_PERCENT: u8 = 40;

fn flip_op(op: BinOp) -> Option<BinOp> {
    match op {
        BinOp::Lt => Some(BinOp::Gt),
        BinOp::Gt => Some(BinOp::Lt),
        BinOp::Le => Some(BinOp::Ge),
        BinOp::Ge => Some(BinOp::Le),
        BinOp::Eq => Some(BinOp::Eq),
        BinOp::Ne => Some(BinOp::Ne),
        _ => None,
    }
}

impl Pass for ComparisonCommute {
    fn name(&self) -> &'static str {
        "comparison-commute"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            for block in f.blocks.iter_mut() {
                for i in block.instrs.iter_mut() {
                    let (dst, op, lhs, rhs) = match i {
                        Instr::BinOp { dst, op, lhs, rhs } => (*dst, *op, *lhs, *rhs),
                        _ => continue,
                    };
                    let new_op = match flip_op(op) {
                        Some(o) => o,
                        None => continue,
                    };
                    let roll = rng.gen::<u8>() % 100;
                    if roll >= COMMUTE_PERCENT {
                        continue;
                    }
                    *i = Instr::BinOp {
                        dst,
                        op: new_op,
                        lhs: rhs,
                        rhs: lhs,
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::MirProgram;
    use rand::SeedableRng;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        luau_mir::lower::lower(&hir).unwrap()
    }

    fn count_op(p: &MirProgram, target: BinOp) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if let Instr::BinOp { op, .. } = i {
                        if *op == target {
                            n += 1;
                        }
                    }
                }
            }
        }
        n
    }

    #[test]
    fn commutes_some_lt_to_gt() {
        // A program with several `<` comparisons.
        let src = "
            local a, b, c, d = 1, 2, 3, 4
            print(a < b, b < c, c < d, a < c, b < d)
        ";
        let p = mir_of(src);
        let lt_before = count_op(&p, BinOp::Lt);
        let gt_before = count_op(&p, BinOp::Gt);
        assert!(lt_before >= 5, "expected ≥5 Lt, got {}", lt_before);

        let mut saw_commute = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ComparisonCommute.run(&mut p_clone, &mut rng);
            let lt_after = count_op(&p_clone, BinOp::Lt);
            let gt_after = count_op(&p_clone, BinOp::Gt);
            // Every Lt commuted becomes Gt: Lt count drops by N, Gt count rises by N.
            let commuted = lt_before as i64 - lt_after as i64;
            let gained = gt_after as i64 - gt_before as i64;
            assert_eq!(
                commuted, gained,
                "seed {}: Lt-loss ({}) != Gt-gain ({})",
                s, commuted, gained
            );
            if commuted > 0 {
                saw_commute = true;
            }
        }
        assert!(saw_commute, "no seed 0..5 commuted any Lt");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local a, b = 1, 2
            print(a < b, a == b, a >= b)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ComparisonCommute.run(&mut p1, &mut r1);
        ComparisonCommute.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn programs_with_no_comparisons_are_unchanged() {
        let src = "local x = 1 + 2 * 3 print(x)";
        let mut p = mir_of(src);
        let nlocals_before = p.functions[0].n_locals;
        let dump_before = format!("{:?}", p.functions[0].blocks);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ComparisonCommute.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, nlocals_before);
        let dump_after = format!("{:?}", p.functions[0].blocks);
        assert_eq!(dump_before, dump_after, "non-comparison program changed");
    }

    #[test]
    fn instruction_count_is_unchanged() {
        // The pass mutates in place; never inserts or removes instructions.
        let src = "
            local a, b = 1, 2
            if a < b then print('lt') end
            if a == b then print('eq') end
            if a <= b then print('le') end
            print(a > b, a >= b, a ~= b)
        ";
        let baseline = mir_of(src);
        let baseline_count: usize = baseline
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .map(|b| b.instrs.len())
            .sum();
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ComparisonCommute.run(&mut p, &mut rng);
            let after_count: usize = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| b.instrs.len())
                .sum();
            assert_eq!(after_count, baseline_count, "seed {}: instr count changed", s);
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`:

```rust
pub mod comparison_commute;
```

And update `default_plan` so `ComparisonCommute` runs at position 7 (after `MulOperandScatter`, before `BranchPolarityFlip`):

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(opaque_predicate::OpaqueTruePredicate))
        .push(Box::new(junk_arith::JunkArithmetic))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(comparison_commute::ComparisonCommute))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new tests pass on top of existing.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed multi-seed harness.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/comparison_commute.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): comparison-operator commute — Lt↔Gt, Le↔Ge, Eq, Ne"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `comparison_commute_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn comparison_commute_changes_output() {
        let src = "
            local function classify(n)
                if n < 0 then return 'neg' end
                if n == 0 then return 'zero' end
                if n > 0 then return 'pos' end
            end
            print(classify(-5), classify(0), classify(5))
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `comparison_commute_preserves_semantics`**

```rust
    #[test]
    fn comparison_commute_preserves_semantics() {
        let src = "
            local function classify(n)
                if n < 0 then return 'neg' end
                if n == 0 then return 'zero' end
                return 'pos'
            end
            print(classify(-5), classify(0), classify(5))
        ";
        for seed_byte in [14u8, 70, 150, 230] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("neg") && stdout.contains("zero") && stdout.contains("pos"),
                "seed {}: expected neg/zero/pos, got: {}", seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 22 acceptance — comparison commute preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 22 — comparison-operator commute. Each comparison BinOp
(<, <=, >, >=, ==, ~=) has a 40% chance of being commuted: operands
swapped and the operator flipped to the symmetric form. Bit-exact for
all Luau values including NaN and metatable __lt/__le/__eq. Subsequent
plans wrap return paths (Plan 23) and enrich junk blocks (Plan 24).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 22 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed multi-seed harness.
- [ ] `commutes_some_lt_to_gt` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `programs_with_no_comparisons_are_unchanged` passes.
- [ ] `instruction_count_is_unchanged` passes across 20 seeds.
- [ ] `comparison_commute_changes_output` passes.
- [ ] `comparison_commute_preserves_semantics` passes.
