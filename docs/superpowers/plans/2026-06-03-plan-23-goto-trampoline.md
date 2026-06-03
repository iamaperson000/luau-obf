# Luau Obfuscator — Plan 23: Goto Trampoline Insertion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Goal:** Inflate the MIR CFG by inserting empty trampoline blocks on `Goto` edges. For each `Terminator::Goto(target)` with 30% probability:

1. Create a new empty `BasicBlock` `inter` with `terminator = Goto(target)` and no instructions.
2. Rewrite the original block's terminator to `Goto(inter.id)`.

Same runtime path (a → inter → target), but the CFG has more blocks and the original edge now hops through an intermediate. After Plan 18 (branch polarity flip) and Plan 20 (opaque predicates) — which may further wrap these new `Goto`s — the trampolines become hops through opaque-wrapped intermediates.

**Why this is safe:**
- A `Goto(target)` followed by another `Goto(target)` is exactly equivalent to a single `Goto(target)`. No semantics change.
- Trampoline blocks have zero instructions, so no register allocation issues.

**Architecture:**
- New file `crates/luau-passes/src/goto_trampoline.rs` exposes `pub struct GotoTrampoline;`.
- Pass runs at default-plan position 6 (between `AddIdentityPadding` and `MulOperandScatter` — doesn't matter much, just needs to be after CFG-shape passes that read terminators).
- New BlockIds allocated as `max(existing) + 1`, incremented per insertion.

**Tech Stack:** Same as Plans 1-22.

---

## Implementation notes

**Walk semantics.** For each function, iterate blocks. Snapshot the indices of blocks with `Goto` terminators BEFORE adding new blocks (so we don't process newly-inserted trampolines). For each snapshot index, draw a u8; if rolled, create a trampoline block and rewire.

**Block ID allocation.** Maintain a `next_block_id` counter initialized to `max(f.blocks.iter().map(|b| b.id.0)) + 1`. Increment per insertion.

**RNG consumption.** One u8 per `Goto` terminator in the function.

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  goto_trampoline.rs       # CREATE: GotoTrampoline pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

---

## Task 1: Implement the GotoTrampoline pass

**Files:**
- Create: `crates/luau-passes/src/goto_trampoline.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/goto_trampoline.rs`:

```rust
//! Goto-Trampoline pass — for each `Terminator::Goto(target)`, with 30%
//! probability, insert an empty intermediate `BasicBlock` whose terminator
//! is `Goto(target)`, then rewrite the original Goto to point at the
//! intermediate. CFG inflates without semantic change.

use crate::Pass;
use luau_mir::{BasicBlock, BlockId, MirProgram, Terminator};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct GotoTrampoline;

const TRAMPOLINE_PERCENT: u8 = 30;

impl Pass for GotoTrampoline {
    fn name(&self) -> &'static str {
        "goto-trampoline"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            if f.blocks.is_empty() {
                continue;
            }
            // Snapshot block indices that have Goto terminators.
            let goto_indices: Vec<usize> = f
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(idx, b)| match b.terminator {
                    Terminator::Goto(_) => Some(idx),
                    _ => None,
                })
                .collect();
            let mut next_block_id_value = f
                .blocks
                .iter()
                .map(|b| b.id.0)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
            let mut new_blocks: Vec<BasicBlock> = Vec::new();
            for idx in goto_indices {
                let roll = rng.gen::<u8>() % 100;
                if roll >= TRAMPOLINE_PERCENT {
                    continue;
                }
                let original_target = match f.blocks[idx].terminator {
                    Terminator::Goto(t) => t,
                    _ => continue,
                };
                let new_id = BlockId(next_block_id_value);
                next_block_id_value += 1;
                let trampoline = BasicBlock {
                    id: new_id,
                    instrs: Vec::new(),
                    terminator: Terminator::Goto(original_target),
                };
                new_blocks.push(trampoline);
                f.blocks[idx].terminator = Terminator::Goto(new_id);
            }
            f.blocks.extend(new_blocks);
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

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
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

    #[test]
    fn inserts_some_trampolines() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let gotos_before = count_gotos(&p);
        let blocks_before = total_blocks(&p);
        assert!(gotos_before >= 1, "expected ≥1 Goto, got {}", gotos_before);

        let mut saw_inflation = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            GotoTrampoline.run(&mut p_clone, &mut rng);
            let blocks_after = total_blocks(&p_clone);
            let gotos_after = count_gotos(&p_clone);
            // Each insertion: +1 block, +1 Goto (the trampoline's own).
            // The original Goto is REROUTED, not removed, so it still counts.
            let added_blocks = blocks_after as i64 - blocks_before as i64;
            let added_gotos = gotos_after as i64 - gotos_before as i64;
            assert_eq!(
                added_blocks, added_gotos,
                "seed {}: block growth ({}) != Goto growth ({})",
                s, added_blocks, added_gotos
            );
            if added_blocks > 0 {
                saw_inflation = true;
            }
        }
        assert!(saw_inflation, "no seed 0..5 inserted any trampoline");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local s = 0
            for i = 1, 3 do s = s + i end
            print(s)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        GotoTrampoline.run(&mut p1, &mut r1);
        GotoTrampoline.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    s.push_str(&format!("block {:?}: ", b.id));
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}, ", i));
                    }
                    s.push_str(&format!("term: {:?}\n", b.terminator));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn programs_with_no_gotos_are_unchanged() {
        let src = "print('hi')";
        let mut p = mir_of(src);
        let blocks_before = total_blocks(&p);
        for s in 0..20u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            GotoTrampoline.run(&mut p_clone, &mut rng);
            assert_eq!(total_blocks(&p_clone), blocks_before, "seed {}: blocks grew", s);
        }
        let _ = p;
    }

    #[test]
    fn each_trampoline_block_has_no_instrs() {
        // After a run, every newly-added block should be empty (only a Goto terminator).
        let src = "
            local s = 0
            for i = 1, 10 do s = s + i end
            for j = 1, 10 do s = s + j end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline_block_ids: std::collections::HashSet<u32> = mir_of(src)
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| b.id.0)
                .collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            GotoTrampoline.run(&mut p, &mut rng);
            for f in &p.functions {
                for b in &f.blocks {
                    if !baseline_block_ids.contains(&b.id.0) {
                        // It's a new trampoline. Must have empty instrs and Goto terminator.
                        assert!(b.instrs.is_empty(),
                            "seed {}: new block {:?} has {} instrs",
                            seed_byte, b.id, b.instrs.len());
                        assert!(matches!(b.terminator, Terminator::Goto(_)),
                            "seed {}: new block {:?} has non-Goto terminator",
                            seed_byte, b.id);
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`:

```rust
pub mod goto_trampoline;
```

And update `default_plan` to insert at position 6 (between `AddIdentityPadding` and `MulOperandScatter`):

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new tests pass.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed multi-seed harness. Empty trampoline blocks routed via Goto are semantics-preserving.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/goto_trampoline.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): Goto trampoline insertion — inflate CFG with empty intermediates"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `goto_trampoline_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn goto_trampoline_changes_output() {
        let src = "
            local total = 0
            for i = 1, 10 do
                for j = 1, 10 do
                    total = total + i * j
                end
            end
            print(total)
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `goto_trampoline_preserves_semantics`**

```rust
    #[test]
    fn goto_trampoline_preserves_semantics() {
        let src = "
            local total = 0
            for i = 1, 10 do
                for j = 1, 10 do
                    total = total + i * j
                end
            end
            print(total)
        "; // sum_{i=1..10} sum_{j=1..10} i*j = (sum i)^2 = 55^2 = 3025
        for seed_byte in [15u8, 80, 160, 250] {
            let r = obfuscate(src, Options { seed: Some([seed_byte; 32]) }).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("obf.luau");
            std::fs::write(&path, &r.output).unwrap();
            let out = std::process::Command::new("luau").arg(&path).output().unwrap();
            assert!(out.status.success(),
                "seed {}: luau exited {:?}; stderr: {}",
                seed_byte, out.status, String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("3025"),
                "seed {}: expected '3025', got: {}",
                seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 23 acceptance — goto trampoline preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 23 — Goto trampoline insertion. Each `Goto(target)`
terminator has a 30% chance of being rerouted through an empty
intermediate block whose terminator points to the original target. The
CFG gains one block and one indirect hop per insertion, while runtime
behavior is unchanged. Combined with Plan 20's opaque-true wrapping,
trampolines may themselves be wrapped — turning a single `Goto` into a
two-hop opaque-branch chain.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 23 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass the 8-seed harness.
- [ ] `inserts_some_trampolines` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `programs_with_no_gotos_are_unchanged` passes across 20 seeds.
- [ ] `each_trampoline_block_has_no_instrs` passes across 20 seeds.
- [ ] `goto_trampoline_changes_output` passes.
- [ ] `goto_trampoline_preserves_semantics` passes.
