# Luau Obfuscator — Plan 25: Junk Sub Chain Insertion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Goal:** Final pass in the obfuscation pipeline. For each `BasicBlock` with 20% probability, insert at a seed-determined position a 4-instruction dead-store chain using `Sub`:

```
δ1_vl = LoadConst(δ1)             -- seed-derived integer in [2, 65535]
δ2_vl = LoadConst(δ2)             -- different seed-derived integer
tmp1  = BinOp::Sub(δ1_vl, δ2_vl)   -- δ1 - δ2
tmp2  = BinOp::Sub(tmp1, δ1_vl)    -- (δ1 - δ2) - δ1 = -δ2
```

Symmetric to Plan 21's junk-arithmetic injection but uses `Sub` instead of `Add + Mul`. The two passes together produce diverse dead-store shapes that a static analyzer must recognize independently.

**Composition.**
- Runs at default-plan position 11 (after `SyntheticMove`, before `BranchPolarityFlip`). All earlier passes have done their work; Plan 25 adds final junk shape variety.
- The new Subs may be candidates for Plan 15's Sub→Neg+Add rewrite — but Plan 15 ran much earlier in the pipeline, so these Subs won't be re-rewritten by Plan 15.
- The new LoadConsts may be candidates for Plan 19's constant decomposition — but Plan 19 also ran earlier. No further mangling.
- Branch-polarity flip (after Plan 25) doesn't touch instructions, only terminators — unaffected.

**Why this is safe:**
- Fresh VLocals, never read elsewhere.
- LoadConst Number is bit-exact.
- Sub of two finite small-magnitude numbers is exact and never overflows.

**Architecture:**
- New file `crates/luau-passes/src/junk_sub.rs` exposes `pub struct JunkSubChain;`.
- `default_plan` adds it at position 11.

**Tech Stack:** Same as Plans 1-24.

---

## Implementation notes

**RNG consumption.** One u8 per block (always drawn). If selected: 1 u32 for position + 2 u32s for δ values = 3 u32s per injection.

**Insertion position.** Same pattern as Plan 21: `pos = u32 % (block.instrs.len() + 1)`.

**δ range.** `[2, 65535]` — exclude 0 (would make `Sub` trivial) and 1 (potential identity).

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
        .push(Box::new(goto_trampoline::GotoTrampoline))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(comparison_commute::ComparisonCommute))
        .push(Box::new(synth_move::SyntheticMove))
        .push(Box::new(junk_sub::JunkSubChain))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  junk_sub.rs              # CREATE: JunkSubChain pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

---

## Task 1: Implement the JunkSubChain pass

**Files:**
- Create: `crates/luau-passes/src/junk_sub.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/junk_sub.rs`:

```rust
//! Junk-Sub-Chain pass — for each `BasicBlock`, with 20% probability,
//! inserts a 4-instruction dead-store chain at a seed-determined position:
//!
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Sub(δ1_vl, δ2_vl)
//!   tmp2  = Sub(tmp1, δ1_vl)
//!
//! Symmetric to JunkArithmetic but uses Sub. Adds dead-store pattern variety.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct JunkSubChain;

const INJECT_PERCENT: u8 = 20;
const DELTA_MIN: u32 = 2;
const DELTA_MAX: u32 = 65535;

impl Pass for JunkSubChain {
    fn name(&self) -> &'static str {
        "junk-sub-chain"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let roll = rng.gen::<u8>() % 100;
                if roll >= INJECT_PERCENT {
                    continue;
                }
                let pos_pick = rng.gen::<u32>();
                let pos = (pos_pick as usize) % (block.instrs.len() + 1);

                let delta1 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                let delta2 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;

                let c1_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta1));
                let c2_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta2));

                let d1_vl = VLocal(n_locals);
                let d2_vl = VLocal(n_locals + 1);
                let tmp1 = VLocal(n_locals + 2);
                let tmp2 = VLocal(n_locals + 3);
                n_locals += 4;

                let junk: [Instr; 4] = [
                    Instr::LoadConst { dst: d1_vl, src: c1_id },
                    Instr::LoadConst { dst: d2_vl, src: c2_id },
                    Instr::BinOp {
                        dst: tmp1,
                        op: BinOp::Sub,
                        lhs: Value::VLocal(d1_vl),
                        rhs: Value::VLocal(d2_vl),
                    },
                    Instr::BinOp {
                        dst: tmp2,
                        op: BinOp::Sub,
                        lhs: Value::VLocal(tmp1),
                        rhs: Value::VLocal(d1_vl),
                    },
                ];
                for instr in junk.iter().rev().cloned() {
                    block.instrs.insert(pos, instr);
                }
                let _ = tmp2;
            }
            f.n_locals = n_locals;
            f.consts = consts;
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

    fn total_instrs(p: &MirProgram) -> usize {
        p.functions.iter().flat_map(|f| &f.blocks).map(|b| b.instrs.len()).sum()
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

    #[test]
    fn injects_some_sub_chains() {
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            for j = 1, 5 do s = s + j end
            for k = 1, 5 do s = s + k end
            print(s)
        ";
        let p = mir_of(src);
        let instrs_before = total_instrs(&p);
        let subs_before = count_subs(&p);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkSubChain.run(&mut p_clone, &mut rng);
            let instrs_after = total_instrs(&p_clone);
            let subs_after = count_subs(&p_clone);
            // Each injection: +4 instrs, +2 Subs.
            let instr_growth = instrs_after as i64 - instrs_before as i64;
            let sub_growth = subs_after as i64 - subs_before as i64;
            assert_eq!(
                instr_growth, 2 * sub_growth,
                "seed {}: instr growth ({}) != 2 * sub growth ({})",
                s, instr_growth, sub_growth
            );
            if sub_growth > 0 {
                saw_injection = true;
            }
        }
        assert!(saw_injection, "no seed 0..5 injected any sub chain");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local x = 1 if x then x = x + 1 end print(x)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        JunkSubChain.run(&mut p1, &mut r1);
        JunkSubChain.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                    s.push_str(&format!("term: {:?}\n", b.terminator));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn terminators_and_block_count_unchanged() {
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline = mir_of(src);
            let baseline_block_count: usize = baseline.functions.iter().map(|f| f.blocks.len()).sum();
            let baseline_terms: Vec<_> = baseline.functions.iter().flat_map(|f| &f.blocks).map(|b| format!("{:?}", b.terminator)).collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkSubChain.run(&mut p, &mut rng);
            let after_block_count: usize = p.functions.iter().map(|f| f.blocks.len()).sum();
            let after_terms: Vec<_> = p.functions.iter().flat_map(|f| &f.blocks).map(|b| format!("{:?}", b.terminator)).collect();
            assert_eq!(baseline_block_count, after_block_count, "seed {}: block count changed", seed_byte);
            assert_eq!(baseline_terms, after_terms, "seed {}: terminator changed", seed_byte);
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`:

```rust
pub mod junk_sub;
```

And update `default_plan`:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(opaque_predicate::OpaqueTruePredicate))
        .push(Box::new(junk_arith::JunkArithmetic))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(goto_trampoline::GotoTrampoline))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(comparison_commute::ComparisonCommute))
        .push(Box::new(synth_move::SyntheticMove))
        .push(Box::new(junk_sub::JunkSubChain))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

`PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 3 new tests pass.

- [ ] **Step 4: Run the workspace + corpus**

`PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: 23 corpus programs pass under 8-seed harness.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/junk_sub.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): junk Sub-chain dead-store injection"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `junk_sub_changes_output`**

```rust
    #[test]
    fn junk_sub_changes_output() {
        let src = "
            local function pow2(n)
                local r = 1
                for _ = 1, n do r = r * 2 end
                return r
            end
            print(pow2(10))
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `junk_sub_preserves_semantics`**

```rust
    #[test]
    fn junk_sub_preserves_semantics() {
        let src = "
            local function pow2(n)
                local r = 1
                for _ = 1, n do r = r * 2 end
                return r
            end
            print(pow2(10))
        "; // 1024
        for seed_byte in [17u8, 95, 175, 255] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("1024"),
                "seed {}: expected '1024', got: {}", seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

`PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 25 acceptance — junk Sub chain preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

```
**Status:** Plan 25 — junk Sub-chain insertion (final pass in the
Plans-16-25 mangling family). Each MIR basic block has a 20% chance of
receiving a 4-instruction synthetic Sub chain at a seed-random position.
Symmetric to Plan 21's Add/Mul chain. Combined with Plans 15-24, the
obfuscated bytecode contains stochastic dead-store payload (Move, Add,
Mul, Sub), opaque-true branches with junk blocks, Goto trampolines,
operand-padded arithmetic, commuted comparisons, polarity-flipped
branches, and decomposed constants — all per-build-seeded.
```

Commit: `git commit -m "docs: mark Plan 25 status"`

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] 8-seed corpus harness passes.
- [ ] `injects_some_sub_chains` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `terminators_and_block_count_unchanged` across 20 seeds.
- [ ] `junk_sub_changes_output` passes.
- [ ] `junk_sub_preserves_semantics` passes.
