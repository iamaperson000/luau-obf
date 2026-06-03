# Luau Obfuscator — Plan 21: Junk Arithmetic Injection

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Goal:** Inflate basic blocks with **dead-store** arithmetic that a static analyzer must liveness-prove before removing. For each `BasicBlock`, with 30% probability, insert a 4-instruction junk chain at a seed-determined position in `block.instrs`:

```
δ1_vl = LoadConst(δ1)            -- seed-derived integer
δ2_vl = LoadConst(δ2)            -- different seed-derived integer
tmp1  = BinOp::Add(δ1_vl, δ2_vl)
tmp2  = BinOp::Mul(tmp1, δ1_vl)
```

The four fresh VLocals (`δ1_vl`, `δ2_vl`, `tmp1`, `tmp2`) are **never read** by any subsequent instruction — they're dead stores. The arithmetic looks meaningful (LoadConsts + Add + Mul) but has no observable effect.

After Plan 21, every block has a ~30% chance of containing extra synthetic computation. Combined with downstream passes:
- Plan 16 (AddIdentityPadding) may scatter the junk Add into `(a+k)+(b-k)` form.
- Plan 17 (MulOperandScatter) may scatter the junk Mul into `(x+k)*y - k*y` form.
- Plan 18 (BranchPolarityFlip) is unaffected (no branches touched).

A single 4-instruction junk chain can compile to 10+ bytecode instructions after later passes.

**Architecture:**
- New file `crates/luau-passes/src/junk_arith.rs` exposes `pub struct JunkArithmetic;`.
- Pass runs at default-plan position 5 (after `OpaqueTruePredicate`, before `AddIdentityPadding`).
- No new types. No special semantics — dead stores are just dead stores.

**Why this is safe:**
- The fresh VLocals are written once (no aliasing).
- The arithmetic operates on fresh LoadConsts of finite small-magnitude numbers — no FP drift, no metatable hazard.
- Inserting instructions in the middle of a block doesn't change the block's terminator or successors; CFG is invariant.

**Tech Stack:** Same as Plans 1-20.

---

## Implementation notes

**Insertion position.** For each block selected for injection, draw a `u32` and reduce modulo `block.instrs.len() + 1` to pick an insertion point in `[0, len]`. The 4 new instructions are inserted contiguously at that index. This randomizes where in the block the junk lands (start, middle, just before terminator).

**Why "modulo len+1" not "len"?** We want the insertion index to include "before instruction 0" (position 0) and "after the last instruction" (position len). That's len+1 valid positions.

**Edge case: empty block.** A block with zero instructions and a terminator (e.g. just a `Goto` or `Return`). `len = 0`, so insertion position is forced to 0. The 4 instructions are prepended. Safe.

**RNG consumption.**
- One `u8` per `BasicBlock` (always drawn, for deterministic advancement).
- For each block selected: one `u32` (position picker), then four `u32`s (the four δ values: two for LoadConst, two for arithmetic — wait, only two LoadConsts, so two δ values; the arithmetic uses VLocals, not new constants. So: 1 u32 for position + 2 u32s for δs = 3 u32s per injection).

Hmm — actually: each injection needs TWO seed-derived integer values (δ1 and δ2). Let me state: 1 `u32` for position, 1 `u32` for δ1, 1 `u32` for δ2 = 3 `u32`s per injection.

**δ value range.** `δ ∈ [2, 65535]` (skip 1 to avoid trivially-foldable `Add(x, 1) = x+1` patterns; skip 0 to avoid identity).

**Walk semantics.** Iterate functions, then blocks (snapshot block indices before mutation since we don't mutate `f.blocks` itself, only `block.instrs`). For each block, decide and inject. The inserted instructions don't trigger further injections in the same pass (we don't re-scan the modified block).

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  junk_arith.rs            # CREATE: JunkArithmetic pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass at position 5
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

---

## Task 1: Implement the JunkArithmetic pass

**Files:**
- Create: `crates/luau-passes/src/junk_arith.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/junk_arith.rs`:

```rust
//! Junk-Arithmetic pass — for each `BasicBlock`, with 30% probability,
//! inserts a 4-instruction dead-store chain at a seed-determined position:
//!
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Add(δ1_vl, δ2_vl)
//!   tmp2  = Mul(tmp1, δ1_vl)
//!
//! The four fresh VLocals are dead (never read). Plans 16/17 may further
//! scatter the inserted Add and Mul.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct JunkArithmetic;

const INJECT_PERCENT: u8 = 30;
const DELTA_MIN: u32 = 2;
const DELTA_MAX: u32 = 65535;

impl Pass for JunkArithmetic {
    fn name(&self) -> &'static str {
        "junk-arithmetic"
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
                // Pick insertion position uniformly in [0, instrs.len()].
                let pos_pick = rng.gen::<u32>();
                let pos = (pos_pick as usize) % (block.instrs.len() + 1);

                // Pick two δ values.
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
                        op: BinOp::Add,
                        lhs: Value::VLocal(d1_vl),
                        rhs: Value::VLocal(d2_vl),
                    },
                    Instr::BinOp {
                        dst: tmp2,
                        op: BinOp::Mul,
                        lhs: Value::VLocal(tmp1),
                        rhs: Value::VLocal(d1_vl),
                    },
                ];
                // Insert in reverse so each `insert` lands at the same index.
                for instr in junk.iter().rev().cloned() {
                    block.instrs.insert(pos, instr);
                }
                // Suppress unused warning for tmp2 — it's intentionally dead.
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
        p.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .map(|b| b.instrs.len())
            .sum()
    }

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
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
    fn injects_some_junk() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let blocks = total_blocks(&p);
        assert!(blocks >= 3, "expected ≥3 blocks, got {}", blocks);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let instrs_before = total_instrs(&p_clone);
            let consts_before = count_number_consts(&p_clone);
            let nlocals_before = p_clone.functions[0].n_locals;
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkArithmetic.run(&mut p_clone, &mut rng);
            let instrs_growth = total_instrs(&p_clone) - instrs_before;
            let consts_growth = count_number_consts(&p_clone) - consts_before;
            let nlocals_growth = (p_clone.functions[0].n_locals - nlocals_before) as usize;
            // Each injection: +4 instrs, +2 consts, +4 vlocals (in any function).
            // Per-function bookkeeping is tracked by us, but the totals across all
            // functions should preserve the ratio.
            // Compute total nlocals across all functions:
            let total_nlocals_growth: usize = p_clone
                .functions
                .iter()
                .zip(mir_of(src).functions.iter())
                .map(|(after, before)| (after.n_locals - before.n_locals) as usize)
                .sum();
            let total_consts_growth: usize = p_clone
                .functions
                .iter()
                .zip(mir_of(src).functions.iter())
                .map(|(after, before)| {
                    let a = after.consts.iter().filter(|c| matches!(c, Constant::Number(_))).count();
                    let b = before.consts.iter().filter(|c| matches!(c, Constant::Number(_))).count();
                    a - b
                })
                .sum();
            assert_eq!(
                instrs_growth, 4 * (total_consts_growth / 2),
                "seed {}: instr growth ({}) not 4x injection count ({})",
                s, instrs_growth, total_consts_growth / 2
            );
            assert_eq!(
                total_nlocals_growth, 2 * total_consts_growth,
                "seed {}: nlocals growth ({}) not 2x const growth ({})",
                s, total_nlocals_growth, total_consts_growth
            );
            if instrs_growth > 0 {
                saw_injection = true;
            }
            let _ = consts_growth; // already covered by total_consts_growth
            let _ = nlocals_growth;
        }
        assert!(saw_injection, "no seed 0..5 injected any junk");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local x = 1
            if x then x = x + 1 else x = x - 1 end
            print(x)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        JunkArithmetic.run(&mut p1, &mut r1);
        JunkArithmetic.run(&mut p2, &mut r2);
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
    fn terminators_are_unchanged() {
        // The pass only inserts instructions; never modifies terminators.
        let src = "
            local s = 0
            for i = 1, 3 do
                s = s + i
                if i == 2 then break end
            end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline = mir_of(src);
            let baseline_terms: Vec<_> = baseline
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| format!("{:?}", b.terminator))
                .collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p, &mut rng);
            let after_terms: Vec<_> = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| format!("{:?}", b.terminator))
                .collect();
            assert_eq!(
                baseline_terms, after_terms,
                "seed {}: terminator changed", seed_byte
            );
        }
    }

    #[test]
    fn block_count_is_unchanged() {
        // The pass never adds or removes blocks.
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline_blocks = total_blocks(&mir_of(src));
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p, &mut rng);
            assert_eq!(total_blocks(&p), baseline_blocks, "seed {}: block count changed", seed_byte);
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod junk_arith;
```

And update `default_plan` to insert `JunkArithmetic` at position 5 (after `OpaqueTruePredicate`, before `AddIdentityPadding`):

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new tests pass plus all existing.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed multi-seed harness. Dead-store insertion never changes semantics — the inserted VLocals are not read.

If any corpus fails: the only plausible cause is that the inserted Add/Mul interacts with later passes in a way that changes program behavior. Investigate.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/junk_arith.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): junk arithmetic dead-store injection in MIR blocks"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `junk_arith_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn junk_arith_changes_output() {
        let src = "
            local function fact(n)
                if n <= 1 then return 1 end
                return n * fact(n - 1)
            end
            print(fact(5))
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `junk_arith_preserves_semantics`**

```rust
    #[test]
    fn junk_arith_preserves_semantics() {
        let src = "
            local function fact(n)
                if n <= 1 then return 1 end
                return n * fact(n - 1)
            end
            print(fact(5))
        "; // 120
        for seed_byte in [13u8, 67, 144, 222] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("120"),
                "seed {}: expected '120', got: {}", seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 21 acceptance — junk arith preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 21 — junk arithmetic dead-store injection. Each MIR
basic block has a 30% chance of receiving a 4-instruction synthetic
arithmetic chain (LoadConst, LoadConst, Add, Mul) at a seed-determined
position. The four fresh VLocals are never read — a static analyzer must
do liveness analysis to identify them as dead. Combined with Plans 16-17,
the inserted Add and Mul are themselves scattered into operand-padded
forms, so a single junk chain compiles to 10+ bytecode instructions.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 21 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed multi-seed harness.
- [ ] `injects_some_junk` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `terminators_are_unchanged` passes across 20 seeds.
- [ ] `block_count_is_unchanged` passes across 20 seeds.
- [ ] `junk_arith_changes_output` passes.
- [ ] `junk_arith_preserves_semantics` passes (4 seeds, all produce "120").
