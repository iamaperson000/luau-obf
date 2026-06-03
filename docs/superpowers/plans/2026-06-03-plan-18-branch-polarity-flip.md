# Luau Obfuscator — Plan 18: Branch Polarity Flip (CFG-level)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Land the first **CFG-level** obfuscation pass. For each conditional branch terminator in the MIR (`Terminator::Branch { cond, then_block, else_block }`) and with 30% probability per branch, rewrite as:

```
... (existing instructions in block) ...
not_cond = UnOp::Not(cond)     -- NEW: appended to block.instrs
terminator = Branch { cond: not_cond, then: else_block, else: then_block }
```

This breaks the 1:1 correspondence between source-level `if cond then A else B` and bytecode-level "branch-on-cond to A else B": for ~30% of conditionals per build, the branch target order is flipped and a `Not` is interposed. A static analyzer can't tell from the bytecode alone whether a given branch's "then" path corresponds to the source's "then" path without simulating the `Not` opcode.

This pass establishes the pattern for subsequent CFG passes (Plan 19 — opaque predicates; Plan 21 — bogus block insertion).

**Why this is safe:**
- `UnOp::Not` on **any** Luau value returns a boolean (`not nil` and `not false` are `true`; everything else is `false`). It does NOT invoke a metamethod — Lua/Luau has no `__not`. So there's no operand-type hazard like Plans 16/17 had with `__add` / `__mul`.
- Swapping then/else after inverting the condition is exactly equivalent in Lua semantics — `if not c then B else A` ≡ `if c then A else B` for any `c`, including `nil`, `false`, and tables.

**Architecture:**
- New file `crates/luau-passes/src/branch_polarity.rs` exposes `pub struct BranchPolarityFlip;` with `impl Pass`.
- `default_plan` adds `BranchPolarityFlip` after `MulOperandScatter`.
- No new types, no shared analyzer needed (this pass doesn't gate on operand type — `Not` is total).

**Tech Stack:** Same as Plans 1-17.

---

## Implementation notes

**Walk semantics.** For each `MirFunction`, for each `BasicBlock`, look at the terminator. If it's a `Branch`, draw one `u8` from rng. If `roll < 30`:
- Allocate a fresh VLocal `not_cond = VLocal(f.n_locals)`.
- Bump `f.n_locals += 1`.
- Append `Instr::UnOp { dst: not_cond, op: UnOp::Not, operand: cond }` to `block.instrs`.
- Replace the terminator with `Branch { cond: Value::VLocal(not_cond), then_block: old_else, else_block: old_then }`.

Single pass per block — the flipped terminator is not re-flipped.

**RNG consumption.** One `u8` per Branch terminator visited. Other terminators (`Goto`, `Return`, `ReturnMulti`) are not affected.

**Composition with prior passes.** This pass runs LAST in the default plan, after all arithmetic mangling. The arithmetic passes don't insert or modify terminators, so there's no interaction.

**Default plan update.**

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
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
  branch_polarity.rs       # CREATE: BranchPolarityFlip pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

No changes to MIR types, LIR, encoder, VM template. `UnOp::Not` is an existing opcode.

---

## Task 1: Implement the BranchPolarityFlip pass

**Files:**
- Create: `crates/luau-passes/src/branch_polarity.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/branch_polarity.rs`:

```rust
//! Branch-Polarity-Flip pass — for each conditional `Branch` terminator,
//! with seed-derived 30% probability, append a `Not(cond)` instruction
//! and swap the then/else successors. Bit-equivalent under Luau semantics
//! (`not` is total — no metamethod). The CFG structure no longer mirrors
//! source `if`/`else` layout.

use crate::Pass;
use luau_hir::UnOp;
use luau_mir::{Instr, MirProgram, Terminator, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct BranchPolarityFlip;

const FLIP_PERCENT: u8 = 30;

impl Pass for BranchPolarityFlip {
    fn name(&self) -> &'static str {
        "branch-polarity-flip"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            for block in f.blocks.iter_mut() {
                let (cond, then_block, else_block) = match &block.terminator {
                    Terminator::Branch { cond, then_block, else_block } => {
                        (*cond, *then_block, *else_block)
                    }
                    _ => continue,
                };
                let roll = rng.gen::<u8>() % 100;
                if roll >= FLIP_PERCENT {
                    continue;
                }
                let not_cond = VLocal(n_locals);
                n_locals += 1;
                block.instrs.push(Instr::UnOp {
                    dst: not_cond,
                    op: UnOp::Not,
                    operand: cond,
                });
                block.terminator = Terminator::Branch {
                    cond: Value::VLocal(not_cond),
                    then_block: else_block,
                    else_block: then_block,
                };
            }
            f.n_locals = n_locals;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::BinOp;
    use luau_mir::MirProgram;
    use rand::SeedableRng;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        luau_mir::lower::lower(&hir).unwrap()
    }

    fn count_nots(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::UnOp { op: UnOp::Not, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
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

    #[test]
    fn flips_some_branches() {
        // Several `if` statements so the 30% flip rate hits something for
        // at least one seed in a small set.
        let src = "
            local x = 1
            if x == 1 then print('a') else print('b') end
            if x > 0 then print('c') else print('d') end
            if x < 10 then print('e') else print('f') end
            if x ~= 0 then print('g') else print('h') end
            if x >= 1 then print('i') else print('j') end
        ";
        let p = mir_of(src);
        let nots_before = count_nots(&p);
        let branches_before = count_branches(&p);
        assert!(branches_before >= 5, "expected ≥5 Branches, got {}", branches_before);

        let mut saw_flip = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            BranchPolarityFlip.run(&mut p_clone, &mut rng);
            // Branch count NEVER changes — we only swap then/else.
            assert_eq!(count_branches(&p_clone), branches_before);
            // Each flip adds exactly 1 UnOp::Not.
            if count_nots(&p_clone) > nots_before {
                saw_flip = true;
            }
        }
        assert!(saw_flip, "no seed in 0..5 flipped any branch");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local x = 1 if x > 0 then print('a') else print('b') end";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        BranchPolarityFlip.run(&mut p1, &mut r1);
        BranchPolarityFlip.run(&mut p2, &mut r2);
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
    fn programs_with_no_branches_are_unchanged() {
        let src = "local a, b = 10, 3 print(a + b)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let nots_before = count_nots(&p);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        BranchPolarityFlip.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
        assert_eq!(count_nots(&p), nots_before);
    }

    #[test]
    fn flip_swaps_then_else_targets() {
        // Single branch program — find a seed that flips it and verify
        // then/else are swapped vs unflipped.
        let src = "local x = 1 if x then print('y') else print('n') end";
        let unflipped = mir_of(src);
        let unflipped_branch = unflipped
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .find_map(|b| match b.terminator {
                Terminator::Branch { then_block, else_block, .. } => Some((then_block, else_block)),
                _ => None,
            })
            .expect("expected at least one Branch in unflipped program");

        let mut found_flip = false;
        for s in 0..30u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            BranchPolarityFlip.run(&mut p, &mut rng);
            let flipped_branch = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .find_map(|b| match b.terminator {
                    Terminator::Branch { then_block, else_block, .. } => Some((then_block, else_block)),
                    _ => None,
                })
                .unwrap();
            if flipped_branch == (unflipped_branch.1, unflipped_branch.0) {
                // Then/else are swapped — confirm a Not was appended.
                let nots_total: usize = p
                    .functions
                    .iter()
                    .flat_map(|f| &f.blocks)
                    .map(|b| b.instrs.iter().filter(|i| matches!(i, Instr::UnOp { op: UnOp::Not, .. })).count())
                    .sum();
                assert!(nots_total >= 1, "seed {} flipped branch but no Not appended", s);
                found_flip = true;
                break;
            }
        }
        assert!(found_flip, "no seed 0..30 flipped the lone branch");
        // BinOp import was used implicitly in mir_of via `if x then`; quiet unused warning.
        let _: BinOp = BinOp::Add;
    }
}
```

- [ ] **Step 2: Register and add to default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod branch_polarity;
```

And update `default_plan`:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
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

EXPECTED: all 23 corpus programs still pass under the 8-seed multi-seed harness. Branch flipping is exact under Luau semantics.

If any corpus program fails: investigate the specific failure. Branch flipping has NO semantic risk by design — `not` is total, and swapping then/else after inverting cond is exactly equivalent. A failure would indicate a real bug in the implementation (e.g. wrong cond passed to Not, wrong block IDs swapped).

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/branch_polarity.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): CFG branch-polarity flip — invert cond + swap then/else"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `branch_flip_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn branch_flip_changes_output() {
        let src = "
            local function check(x)
                if x > 0 then return 'pos' end
                if x < 0 then return 'neg' end
                return 'zero'
            end
            print(check(1), check(-1), check(0))
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `branch_flip_preserves_semantics`**

```rust
    #[test]
    fn branch_flip_preserves_semantics() {
        // A branching program with a deterministic output. Verify the
        // obfuscated chunk produces the same lines as plain luau.
        let src = "
            local total = 0
            for i = 1, 10 do
                if i % 2 == 0 then
                    total = total + i
                else
                    total = total - i
                end
            end
            print(total)
        ";
        let r = obfuscate(src, Options { seed: Some([99u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(),
            "luau exited {:?}; stderr: {}",
            out.status, String::from_utf8_lossy(&out.stderr));
        // sum of evens 2..10 = 30; sum of odds 1..9 = 25; total = 30-25 = 5.
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("5"), "expected '5' in stdout, got: {}", stdout);
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 18 acceptance — branch flip preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 18 — first CFG-level obfuscation pass. Each conditional
`Branch` terminator in the MIR is flipped with 30% probability: a `Not(cond)`
is appended to the predecessor block and the then/else successors are
swapped. Combined with Plans 15-17's arithmetic mangling, the obfuscated
bytecode now diverges from source structure at both the expression and
control-flow levels — a static analyzer must simulate every `Not` opcode
just to recover the source-level `if`/`else` polarity. Next plans add
opaque predicates and junk blocks, then full control-flow flattening.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 18 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed multi-seed differential harness.
- [ ] `flips_some_branches` passes.
- [ ] `deterministic_for_same_seed` passes (strong: full instr + terminator Debug-dump compare).
- [ ] `programs_with_no_branches_are_unchanged` passes.
- [ ] `flip_swaps_then_else_targets` passes.
- [ ] `branch_flip_changes_output` passes.
- [ ] `branch_flip_preserves_semantics` passes.
- [ ] Spot-check: obfuscate an if/else-heavy program under two seeds; the bytecode size should differ (some seeds inject Nots, others don't).
