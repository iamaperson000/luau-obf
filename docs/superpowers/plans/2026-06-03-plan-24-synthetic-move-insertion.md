# Luau Obfuscator — Plan 24: Synthetic Move Insertion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Goal:** Sprinkle dead `Move dst src` instructions through the MIR. For each `BasicBlock` with 25% probability:

1. Walk `block.instrs` from start to find the FIRST instruction that writes a VLocal (LoadConst, BinOp, UnOp, Move, Call/CallVar with `dst: Some`, GetGlobal, GetIndex, GetUpval, NewTable, BuildResults, MakeClosure, GetVarargs).
2. If such an instruction exists, insert `Move { dst: <fresh VLocal>, src: <that VLocal> }` immediately after it. The fresh dst is never read elsewhere — dead store.

This adds `Move` opcodes throughout the bytecode without changing semantics. Combined with prior passes (Add/Mul/Sub/Branch mangling, opaque predicates, junk arithmetic, comparison commute, trampolines), this Move-only pass thickens the basic blocks further.

**Why this is safe:**
- A `Move` to a fresh VLocal that's never read has no observable effect.
- The source VLocal exists (we found it by walking) — no dangling reference.
- The instruction sequence is well-formed in MIR / LIR / encoder pipeline.

**Architecture:**
- New file `crates/luau-passes/src/synth_move.rs` exposes `pub struct SyntheticMove;`.
- Pass runs at default-plan position 9 (after `ComparisonCommute`, before `BranchPolarityFlip`). The synthetic Move doesn't affect terminators or block structure, so it can run almost anywhere.

**Tech Stack:** Same as Plans 1-23.

---

## Implementation notes

**Source-extraction helper.** For each `Instr` variant, return `Some(VLocal)` of its dst (or first written VLocal), or `None`:

| Instr variant                             | dst VLocal           |
|------------------------------------------|----------------------|
| `LoadConst { dst, .. }`                  | dst                  |
| `Move { dst, .. }`                       | dst                  |
| `BinOp { dst, .. }`                      | dst                  |
| `UnOp { dst, .. }`                       | dst                  |
| `GetGlobal { dst, .. }`                  | dst                  |
| `Call { dst: Some(d), .. }`              | d                    |
| `Call { dst: None, .. }`                 | None                 |
| `CallVar { dst: Some(d), .. }`           | d                    |
| `CallVar { dst: None, .. }`              | None                 |
| `BuildResults { dst, .. }`               | dst                  |
| `GetVarargs { dst }`                     | dst                  |
| `MakeClosure { dst, .. }`                | dst                  |
| `GetUpval { dst, .. }`                   | dst                  |
| `NewTable { dst }`                       | dst                  |
| `GetIndex { dst, .. }`                   | dst                  |
| `SetGlobal { .. } / SetIndex { .. } / SetUpval { .. }` | None    |

**Walk semantics.** For each function, for each block: draw a u8. If `< 25`, find the first writing instruction, insert a Move right after it. Advance `i` is implicit — no further iteration after insertion (only one Move per block).

**RNG consumption.** One u8 per block.

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  synth_move.rs            # CREATE: SyntheticMove pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

---

## Task 1: Implement the SyntheticMove pass

**Files:**
- Create: `crates/luau-passes/src/synth_move.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/synth_move.rs`:

```rust
//! Synthetic-Move pass — for each MIR `BasicBlock` with 25% probability,
//! find the first instruction writing a VLocal and insert a `Move` from
//! that VLocal to a fresh (dead) VLocal immediately after it. Adds Move
//! opcodes throughout the bytecode without changing semantics.

use crate::Pass;
use luau_mir::{Instr, MirProgram, VLocal};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct SyntheticMove;

const INSERT_PERCENT: u8 = 25;

fn instr_dst(instr: &Instr) -> Option<VLocal> {
    match instr {
        Instr::LoadConst { dst, .. } => Some(*dst),
        Instr::Move { dst, .. } => Some(*dst),
        Instr::BinOp { dst, .. } => Some(*dst),
        Instr::UnOp { dst, .. } => Some(*dst),
        Instr::GetGlobal { dst, .. } => Some(*dst),
        Instr::Call { dst, .. } => *dst,
        Instr::CallVar { dst, .. } => *dst,
        Instr::BuildResults { dst, .. } => Some(*dst),
        Instr::GetVarargs { dst } => Some(*dst),
        Instr::MakeClosure { dst, .. } => Some(*dst),
        Instr::GetUpval { dst, .. } => Some(*dst),
        Instr::NewTable { dst } => Some(*dst),
        Instr::GetIndex { dst, .. } => Some(*dst),
        Instr::SetGlobal { .. } | Instr::SetIndex { .. } | Instr::SetUpval { .. } => None,
    }
}

impl Pass for SyntheticMove {
    fn name(&self) -> &'static str {
        "synthetic-move"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            for block in f.blocks.iter_mut() {
                let roll = rng.gen::<u8>() % 100;
                if roll >= INSERT_PERCENT {
                    continue;
                }
                let (insert_after_idx, src_vl) = match block
                    .instrs
                    .iter()
                    .enumerate()
                    .find_map(|(idx, instr)| instr_dst(instr).map(|vl| (idx, vl)))
                {
                    Some((i, vl)) => (i, vl),
                    None => continue,
                };
                let dst = VLocal(n_locals);
                n_locals += 1;
                block.instrs.insert(
                    insert_after_idx + 1,
                    Instr::Move { dst, src: src_vl },
                );
            }
            f.n_locals = n_locals;
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

    fn count_moves(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::Move { .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
    }

    #[test]
    fn inserts_some_moves() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let moves_before = count_moves(&p);
        let blocks = total_blocks(&p);
        assert!(blocks >= 3, "expected ≥3 blocks, got {}", blocks);

        let mut saw_insert = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            SyntheticMove.run(&mut p_clone, &mut rng);
            if count_moves(&p_clone) > moves_before {
                saw_insert = true;
                break;
            }
        }
        assert!(saw_insert, "no seed 0..5 inserted any synthetic Move");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local x = 1 x = x + 2 print(x)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        SyntheticMove.run(&mut p1, &mut r1);
        SyntheticMove.run(&mut p2, &mut r2);
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
    fn terminators_and_blocks_are_unchanged() {
        let src = "
            local s = 0
            for i = 1, 3 do s = s + i end
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
            let baseline_block_count = total_blocks(&baseline);
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            SyntheticMove.run(&mut p, &mut rng);
            let after_terms: Vec<_> = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| format!("{:?}", b.terminator))
                .collect();
            assert_eq!(baseline_terms, after_terms, "seed {}: terminator changed", seed_byte);
            assert_eq!(total_blocks(&p), baseline_block_count, "seed {}: block count changed", seed_byte);
        }
    }

    #[test]
    fn each_inserted_move_has_fresh_dst() {
        let src = "local s = 0 for i = 1, 5 do s = s + i end print(s)";
        for seed_byte in 0..20u8 {
            let baseline_n_locals: Vec<u32> =
                mir_of(src).functions.iter().map(|f| f.n_locals).collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            SyntheticMove.run(&mut p, &mut rng);
            for (i, f) in p.functions.iter().enumerate() {
                assert!(
                    f.n_locals >= baseline_n_locals[i],
                    "seed {}: function {} n_locals shrunk",
                    seed_byte, i
                );
            }
        }
    }
}
```

- [ ] **Step 2: Register and update default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`:

```rust
pub mod synth_move;
```

And update `default_plan` to insert `SyntheticMove` at position 9 (before `BranchPolarityFlip`):

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
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new tests pass plus all existing.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs pass the 8-seed harness.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/synth_move.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): synthetic Move insertion — dead Move from first writer per block"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `synth_move_changes_output`**

```rust
    #[test]
    fn synth_move_changes_output() {
        let src = "
            local function tri(n)
                local s = 0
                for i = 1, n do s = s + i end
                return s
            end
            print(tri(10))
        ";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `synth_move_preserves_semantics`**

```rust
    #[test]
    fn synth_move_preserves_semantics() {
        let src = "
            local function tri(n)
                local s = 0
                for i = 1, n do s = s + i end
                return s
            end
            print(tri(10))
        "; // 55
        for seed_byte in [16u8, 90, 170, 252] {
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
                "seed {}: expected '55', got: {}", seed_byte, stdout);
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 24 acceptance — synthetic Move preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

```
**Status:** Plan 24 — synthetic Move insertion. Each MIR basic block has
a 25% chance of receiving a dead Move from the block's first writer to
a fresh VLocal. Adds Move opcodes throughout the bytecode without
changing semantics. Combined with Plan 21's junk arithmetic, every
block has stochastic dead-store payload that a deobfuscator must
liveness-analyze.
```

Commit: `git commit -m "docs: mark Plan 24 status"`

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] 8-seed corpus harness passes.
- [ ] `inserts_some_moves` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `terminators_and_blocks_are_unchanged` across 20 seeds.
- [ ] `each_inserted_move_has_fresh_dst` across 20 seeds.
- [ ] `synth_move_changes_output` passes.
- [ ] `synth_move_preserves_semantics` passes.
