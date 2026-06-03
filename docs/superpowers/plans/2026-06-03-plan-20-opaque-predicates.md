# Luau Obfuscator — Plan 20: Opaque-True Predicates (CFG injection)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Land a CFG-injection pass that turns unconditional `Goto(target)` terminators into `Branch { cond: opaque_true, then: target, else: junk }` terminators. The opaque condition is always true at runtime, but a static analyzer must constant-propagate `δ * δ > 0` (for nonzero δ) to prove it. The else target is a per-function junk block — reachable from the CFG but unexecutable at runtime, containing a few dead `LoadConst Number` instructions and a `Return(None)` terminator.

After Plan 20, the obfuscated CFG contains visible-but-unreachable code, and unconditional jumps look like conditional ones in disassembly.

**Architecture:**
- New file `crates/luau-passes/src/opaque_predicate.rs` exposes `pub struct OpaqueTruePredicate;` with `impl Pass`.
- Pass runs at default-plan position 4 (after `ExpressionMangle`, before `AddIdentityPadding`). This lets later passes mangle the opaque computation: Plan 17 may scatter the `δ*δ` Mul; Plan 18 may polarity-flip the new Branch.
- One junk block per affected function. Only created if at least one `Goto` in the function is selected for wrapping AND the function-level coin flips true.
- LIR / encode / VM unchanged — `LoadConst`, `BinOp::Mul`, `BinOp::Gt`, `Branch`, `Goto`, `Return` are existing constructs.

**Why this is safe:**
- For integer δ in `[1, 65535]`, `δ * δ ∈ [1, 4.29e9]`, well within f64 precision and never overflowing. `> 0` is always true.
- The junk block is unreachable at runtime; its instructions are valid and well-typed; if reached by adversarial means (it can't be via normal program flow), it returns immediately with no side effects.
- The Branch terminator preserves semantics — same target as the original Goto, plus an unreachable alternative path.

**Tech Stack:** Same as Plans 1-19.

---

## Implementation notes

**RNG consumption.**
- One `u8` per `MirFunction` (to decide whether to attempt opaque-wrapping in this function).
- If the function-level coin says yes: one `u8` per existing `Goto` terminator in the function (to decide whether to wrap that one).
- For each Goto actually wrapped: one `u32` (the δ value).
- For the junk block (created once per affected function): three `u8`s for three junk LoadConst δ values.

**Junk block content.** Three `Instr::LoadConst` of seed-derived `Constant::Number` values into fresh VLocals (each VLocal is dead — never read), followed by `Terminator::Return(None)`. The values are integers in `[2, 255]` to look plausible.

**Opaque-true computation appended to each wrapped block.** Four instructions before the new terminator:

```
δ_vl   = LoadConst(δ)           where δ ∈ [1, 65535]
mul_vl = BinOp::Mul(δ_vl, δ_vl)
zero_vl = LoadConst(0.0)
cond_vl = BinOp::Gt(mul_vl, zero_vl)
```

Then the terminator becomes `Branch { cond: Value::VLocal(cond_vl), then_block: original_target, else_block: junk_block_id }`.

**Walk semantics.** Single forward pass per function. For each block, look at its terminator; if `Goto(t)`, draw a u8 and decide whether to wrap. Skip non-Goto terminators.

**Block ID allocation.** New junk block gets `BlockId(max_existing_id + 1)`. The block IDs in MIR are stored in `BasicBlock.id` and referenced by `Terminator::Branch::then_block` / `else_block` / `Terminator::Goto`. No global registry — just pick max + 1.

**Default plan update.**

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(opaque_predicate::OpaqueTruePredicate))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  opaque_predicate.rs      # CREATE: OpaqueTruePredicate pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass at position 4
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

No MIR / LIR / encoder / VM changes.

---

## Task 1: Implement the OpaqueTruePredicate pass

**Files:**
- Create: `crates/luau-passes/src/opaque_predicate.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/opaque_predicate.rs`:

```rust
//! Opaque-True Predicate pass — wraps unconditional `Goto` terminators
//! with a `Branch { cond: opaque_true, then: target, else: junk }` whose
//! condition is `δ * δ > 0` for a seed-derived integer δ ∈ [1, 65535].
//! Always true at runtime; static analyzers must constant-propagate
//! through Mul + Gt to prove it.
//!
//! Per function: one junk block holding ~3 dead `LoadConst Number`
//! instructions and a `Return(None)` terminator. Only created when at
//! least one `Goto` in the function will be wrapped.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{
    BasicBlock, BlockId, ConstId, Constant, Instr, MirProgram, Terminator, VLocal, Value,
};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct OpaqueTruePredicate;

const FN_ENABLE_PERCENT: u8 = 50;
const GOTO_WRAP_PERCENT: u8 = 40;
const DELTA_MIN: u32 = 1;
const DELTA_MAX: u32 = 65535;
const JUNK_LOADCONST_COUNT: usize = 3;
const JUNK_DELTA_MIN: u32 = 2;
const JUNK_DELTA_MAX: u32 = 255;

impl Pass for OpaqueTruePredicate {
    fn name(&self) -> &'static str {
        "opaque-true-predicate"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            // One u8 per function to decide enablement (always draw for
            // deterministic RNG advancement).
            let fn_roll = rng.gen::<u8>() % 100;
            let fn_enable = fn_roll < FN_ENABLE_PERCENT;
            if !fn_enable {
                continue;
            }

            // Phase 1: decide which Gotos to wrap (always draw a u8 per Goto).
            let mut wrap_decisions: Vec<bool> = Vec::with_capacity(f.blocks.len());
            for block in &f.blocks {
                let is_goto = matches!(block.terminator, Terminator::Goto(_));
                if !is_goto {
                    wrap_decisions.push(false);
                    continue;
                }
                let roll = rng.gen::<u8>() % 100;
                wrap_decisions.push(roll < GOTO_WRAP_PERCENT);
            }

            if !wrap_decisions.iter().any(|&w| w) {
                continue;
            }

            // Phase 2: create the junk block.
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            let next_block_id_value = f
                .blocks
                .iter()
                .map(|b| b.id.0)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
            let junk_block_id = BlockId(next_block_id_value);
            let mut junk_instrs: Vec<Instr> = Vec::with_capacity(JUNK_LOADCONST_COUNT);
            for _ in 0..JUNK_LOADCONST_COUNT {
                let junk_delta = rng.gen_range(JUNK_DELTA_MIN..=JUNK_DELTA_MAX) as f64;
                let cid = ConstId(consts.len() as u32);
                consts.push(Constant::Number(junk_delta));
                let dst = VLocal(n_locals);
                n_locals += 1;
                junk_instrs.push(Instr::LoadConst { dst, src: cid });
            }
            let junk_block = BasicBlock {
                id: junk_block_id,
                instrs: junk_instrs,
                terminator: Terminator::Return(None),
            };

            // Phase 3: for each Goto selected, append opaque computation and
            // rewrite the terminator.
            for (idx, &wrap) in wrap_decisions.iter().enumerate() {
                if !wrap {
                    continue;
                }
                let target = match &f.blocks[idx].terminator {
                    Terminator::Goto(t) => *t,
                    _ => continue,
                };
                let delta = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                let delta_const_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta));
                let zero_const_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(0.0));
                let delta_vl = VLocal(n_locals);
                let mul_vl = VLocal(n_locals + 1);
                let zero_vl = VLocal(n_locals + 2);
                let cond_vl = VLocal(n_locals + 3);
                n_locals += 4;
                let block = &mut f.blocks[idx];
                block.instrs.push(Instr::LoadConst { dst: delta_vl, src: delta_const_id });
                block.instrs.push(Instr::BinOp {
                    dst: mul_vl,
                    op: BinOp::Mul,
                    lhs: Value::VLocal(delta_vl),
                    rhs: Value::VLocal(delta_vl),
                });
                block.instrs.push(Instr::LoadConst { dst: zero_vl, src: zero_const_id });
                block.instrs.push(Instr::BinOp {
                    dst: cond_vl,
                    op: BinOp::Gt,
                    lhs: Value::VLocal(mul_vl),
                    rhs: Value::VLocal(zero_vl),
                });
                block.terminator = Terminator::Branch {
                    cond: Value::VLocal(cond_vl),
                    then_block: target,
                    else_block: junk_block_id,
                };
            }

            f.blocks.push(junk_block);
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

    fn count_branches(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                if matches!(b.terminator, Terminator::Branch { .. }) {
                    n += 1;
                }
            }
        }
        n
    }

    fn count_gotos(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                if matches!(b.terminator, Terminator::Goto(_)) {
                    n += 1;
                }
            }
        }
        n
    }

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
    }

    #[test]
    fn wraps_some_gotos() {
        // A program with multiple blocks that reach via Goto. A while loop
        // or numeric for produces these.
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let gotos_before = count_gotos(&p);
        let branches_before = count_branches(&p);
        let blocks_before = total_blocks(&p);
        assert!(gotos_before >= 1, "expected ≥1 Goto, got {}", gotos_before);

        let mut saw_wrap = false;
        for s in 0..10u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            OpaqueTruePredicate.run(&mut p_clone, &mut rng);
            // Each wrap converts a Goto into a Branch and adds one block (the
            // junk block, shared per function).
            let gotos_after = count_gotos(&p_clone);
            let branches_after = count_branches(&p_clone);
            let blocks_after = total_blocks(&p_clone);
            if branches_after > branches_before {
                assert!(blocks_after > blocks_before, "wrap happened but no junk block added");
                assert!(gotos_after < gotos_before, "wrap happened but Goto count didn't drop");
                saw_wrap = true;
            }
        }
        assert!(saw_wrap, "no seed 0..10 wrapped any Goto");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local x = 0
            while x < 5 do
                x = x + 1
            end
            print(x)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        OpaqueTruePredicate.run(&mut p1, &mut r1);
        OpaqueTruePredicate.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                    s.push_str(&format!("term: {:?}\n", b.terminator));
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
    fn programs_with_no_gotos_are_unchanged() {
        // A program with only a top-level print — produces a single block
        // with Return(None), no Goto.
        let src = "print('hi')";
        let mut p = mir_of(src);
        let blocks_before = total_blocks(&p);
        let nlocals_before = p.functions[0].n_locals;
        for s in 0..20u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            OpaqueTruePredicate.run(&mut p_clone, &mut rng);
            assert_eq!(total_blocks(&p_clone), blocks_before, "seed {}: blocks changed", s);
            assert_eq!(
                p_clone.functions[0].n_locals,
                nlocals_before,
                "seed {}: nlocals changed",
                s
            );
        }
        let _ = p; // silence unused warning
    }

    #[test]
    fn each_wrapped_function_has_at_most_one_junk_block() {
        // After a run with several Gotos, all of them route to the SAME junk
        // block (the one created per function).
        let src = "
            local s = 0
            for i = 1, 10 do
                s = s + i
            end
            for j = 1, 10 do
                s = s + j
            end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            OpaqueTruePredicate.run(&mut p, &mut rng);
            for f in &p.functions {
                // Collect every else_block target where the else_block looks
                // like a junk block (i.e., a block whose terminator is
                // Return(None) and whose instrs are all LoadConst).
                let mut junk_targets: Vec<BlockId> = Vec::new();
                for b in &f.blocks {
                    if let Terminator::Branch { else_block, .. } = &b.terminator {
                        // Look up the else_block.
                        let referenced = f.blocks.iter().find(|bb| bb.id == *else_block);
                        if let Some(jb) = referenced {
                            let is_junk = matches!(jb.terminator, Terminator::Return(None))
                                && jb.instrs.iter().all(|i| matches!(i, Instr::LoadConst { .. }));
                            if is_junk && !junk_targets.contains(else_block) {
                                junk_targets.push(*else_block);
                            }
                        }
                    }
                }
                // At most one junk block per function.
                assert!(
                    junk_targets.len() <= 1,
                    "seed {}: function {:?} has {} junk targets — should be ≤1",
                    seed_byte,
                    f.id,
                    junk_targets.len()
                );
            }
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod opaque_predicate;
```

And update `default_plan` so `OpaqueTruePredicate` runs at position 4 (after `ExpressionMangle`, before `AddIdentityPadding`):

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(const_decompose::ConstantDecompose))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(opaque_predicate::OpaqueTruePredicate))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new tests pass on top of existing passes.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed multi-seed differential. The opaque-true predicate is `δ * δ > 0` for δ ∈ [1, 65535] — always true.

If any corpus fails: the most likely cause is the junk block being unreachable in the CFG but unexpectedly visited by the lowering / encoding pipeline. The LIR layer must accept blocks with `Return(None)` terminators that are reachable from a Branch's else_block. This should already work (Return is a standard terminator); if it doesn't, the bug is in LIR.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/opaque_predicate.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): opaque-true predicates — wrap Goto with Branch on (δ*δ > 0)"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `opaque_predicate_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn opaque_predicate_changes_output() {
        let src = "
            local sum = 0
            for i = 1, 5 do
                sum = sum + i
            end
            print(sum)
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `opaque_predicate_preserves_semantics`**

```rust
    #[test]
    fn opaque_predicate_preserves_semantics() {
        // Loop summation — verify every tested seed produces correct output.
        let src = "
            local product = 1
            for i = 1, 6 do
                product = product * i
            end
            print(product)
        "; // 6! = 720
        for seed_byte in [12u8, 50, 130, 240] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("720"),
                "seed {}: expected '720', got: {}",
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
git commit -m "test: Plan 20 acceptance — opaque predicates preserve semantics, vary per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 20 — opaque-true predicates. Each unconditional `Goto`
in the MIR is candidate for being wrapped in a `Branch { cond: δ*δ > 0,
then: target, else: junk_block }`, where δ is a seed-derived integer in
[1, 65535]. The else branch points to a per-function junk block holding
~3 dead `LoadConst` instructions and a `Return(None)`. At runtime the
opaque is always true; statically the analyzer must constant-propagate
the Mul + Gt + δ value to prove it. The junk block compiles to bytecode
but is never executed. Combined with Plans 17-18 (Mul scatter + branch
polarity), the opaque Mul and Branch are themselves further mangled.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 20 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed multi-seed harness.
- [ ] `wraps_some_gotos` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `programs_with_no_gotos_are_unchanged` passes.
- [ ] `each_wrapped_function_has_at_most_one_junk_block` passes across 20 seeds.
- [ ] `opaque_predicate_changes_output` passes.
- [ ] `opaque_predicate_preserves_semantics` passes (4 seeds, all produce "720").
- [ ] Spot-check: obfuscate a loop-heavy program under two seeds; output size should differ (some seeds inject opaque-true wrappers, others don't).
