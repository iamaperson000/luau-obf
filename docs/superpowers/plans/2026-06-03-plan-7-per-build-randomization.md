# Luau Obfuscator — Plan 7: Per-Build Randomization (Opcode Renumbering + Const Pool Shuffling)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the first two *real* per-build obfuscation transforms — seeded opcode renumbering and constant-pool shuffling — so that the same source compiled with two different seeds produces two materially different obfuscated outputs. Both transforms preserve semantics: every existing differential corpus test must still pass under every seed.

**Architecture:**
- The seed flows through `obfuscate()` → `ChaCha20Rng` already. What's missing: `OpMap::new(seed)` ignores its seed (deterministic identity mapping), and no LIR-level pass touches the constant pool. This plan fills both gaps.
- **Opcode renumbering:** `OpMap::new` is rewritten to consume the RNG and produce a permutation of opcode bytes. The render code already writes `OP_<Name> = <opmap.opcode_of(name)>` constants into the VM template, and `encode` reads from `opmap.opcode_of` for the bytecode bytes — both call sites pick up the new mapping automatically.
- **Constant-pool shuffling:** A new module `crates/luau-lir/src/shuffle.rs` exposes `shuffle_constants(program: &mut LirProgram, rng: &mut ChaCha20Rng)`. For each function it computes a permutation `σ`, reorders `f.consts` according to `σ⁻¹`, and rewrites every `Operand::Const(ConstIdx(i))` in `f.instrs` to `Operand::Const(ConstIdx(σ(i)))`. Same encoder, same VM, no opcode change — just different bytes.
- **Plumbing:** The emit signature changes from `emit(program, seed: [u8; 32])` to `emit(program, rng: &mut ChaCha20Rng)`. The facade threads one RNG through MIR passes → LIR shuffle → emit so consumption order is deterministic for a given seed. The CLI is unchanged.

**Tech Stack:** Same as Plans 1-6, plus `rand::seq::SliceRandom` for in-place permutation.

**Why these two passes first, vs others from the design spec:**
- Both are small (<150 lines each), self-contained, and need no IR additions.
- Both materially change the output shape (opcode bytes are different per build; constants are in different positions in the pool).
- Neither risks correctness — the transforms are pure relabellings: a permutation and its inverse cancel by construction.
- Together they exercise the seed-to-output plumbing end-to-end, which sets up future passes (expression mangling, constant lifting, etc.) to drop in without re-paving the road.

---

## Implementation notes

**Opcode permutation algorithm.** `ALL_OPS` is a `&[OpKind]` of length 35 (one entry per opcode kind). The opcode byte for each kind is 1..=35 today. The new `OpMap::new`:

```rust
pub fn new(seed: &[u8; 32]) -> Self {
    let mut rng = ChaCha20Rng::from_seed(*seed);
    let mut bytes: Vec<u8> = (1..=ALL_OPS.len() as u8).collect();
    bytes.shuffle(&mut rng);
    let mut forward = [0u8; 35];
    for (i, b) in bytes.into_iter().enumerate() {
        forward[i] = b;
    }
    OpMap { forward }
}
```

Note: the function still takes the seed (not a `&mut ChaCha20Rng`) so it can be called independently by tests. The facade owns the rng and re-seeds emit consistently — see plumbing changes below.

**Why opcode bytes start at 1 (not 0).** `0` is reserved as a "halt" sentinel by `vm.luau.j2`'s main dispatcher: `read_u16` of the bytecode at offsets past EOF returns `nil`, but a byte of `0` would be a valid opcode. Today's `LoadNil = 1, LoadTrue = 2, …` already avoids `0`. The shuffle preserves this — `(1..=35)` produces bytes 1 through 35 inclusive.

**Constant shuffle correctness.** For each function `f`:
1. Generate a permutation `σ: 0..N → 0..N` where `N = f.consts.len()`.
2. Build a new consts vec: `new[σ(i)] = f.consts[i]` for each `i`.
   Equivalently: `new = (0..N).map(|j| f.consts[σ_inv(j)]).collect()`.
3. For every `Operand::Const(ConstIdx(i))` in `f.instrs`, rewrite to `Operand::Const(ConstIdx(σ(i)))`.

The invariant: `new[σ(i)] == old[i]`, and any rewritten reference now points at `new[σ(i)]` which equals `old[i]`. Loop tests should observe identical runtime behavior.

**Where Operand::Const appears.** Search the LIR encoder for `Operand::Const` — it's emitted by `MInstr::LoadConst`, `MInstr::GetGlobal`, and `MInstr::SetGlobal` lowering. The `encode.rs` switch over operands just writes the index as a u16 regardless. So the rewrite is a one-pass walk over `f.instrs` operating on the `operands: Vec<Operand>` field.

**Same-seed determinism.** The existing `deterministic_with_fixed_seed` test in `crates/luau-obf/src/lib.rs` already asserts that two calls with the same seed produce identical output. With the new transforms, this remains true because the rng state evolves deterministically.

**Different-seed variance.** Add a new test: `obfuscate(X, seed_A).output != obfuscate(X, seed_B).output` for any X with at least one opcode or constant. Today this would FAIL (outputs are identical regardless of seed); after Plan 7 it will pass.

**Plumbing change details.** Today:
```
obfuscate(): rng = ChaCha20Rng::from_seed(seed); passes.run(mir, &mut rng); ... emit(lir, seed)
                                                                                       ^^^^^
emit() internally re-seeds: ChaCha20Rng::from_seed(seed) again — two independent streams.
```

After:
```
obfuscate(): rng = ChaCha20Rng::from_seed(seed); passes.run(mir, &mut rng);
             shuffle_constants(&mut lir, &mut rng); emit(lir, &mut rng)
                                                              ^^^^^^^^
emit() consumes from the shared stream.
OpMap::new still takes &[u8; 32] but the facade derives a sub-seed: rng.fill_bytes(&mut sub); OpMap::new(&sub)
```

Why derive a sub-seed for OpMap instead of passing `&mut rng`? Because OpMap is constructed in `emit()` lib code AND in unit tests that don't have a full rng. Keeping the `[u8; 32]` signature makes test setup trivial. The facade does the derivation in one line: `let mut opmap_seed = [0u8; 32]; rng.fill_bytes(&mut opmap_seed); ... OpMap::new(&opmap_seed)`.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  lib.rs              # MODIFY: emit(program, rng) — was emit(program, seed)
  opmap.rs            # MODIFY: OpMap::new permutes via the seed
  render.rs           # MODIFY: signature accepts &mut rng (was already taking it but unused)
crates/luau-lir/src/
  lib.rs              # MODIFY: pub mod shuffle
  shuffle.rs          # CREATE: shuffle_constants(program, rng)
crates/luau-obf/src/
  lib.rs              # MODIFY: thread one rng through everything; add variance test
crates/luau-obf-cli/tests/
  cli.rs              # MODIFY: add variance test (different seeds → different output)
README.md             # MODIFY: status line
```

---

## Task 1: Refactor emit to take a shared RNG (preparatory)

**Files:**
- Modify: `crates/luau-emit/src/lib.rs`
- Modify: `crates/luau-emit/src/opmap.rs` (signature stays the same — no work in this task)
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Change emit's signature**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/lib.rs`, replace:

```rust
pub fn emit(program: &LirProgram, seed: [u8; 32]) -> Result<String, EmitError> {
    let opmap = opmap::OpMap::new(&seed);
    let mut rng = ChaCha20Rng::from_seed(seed);
    render::render(program, &opmap, &mut rng)
}
```

with:

```rust
pub fn emit(program: &LirProgram, rng: &mut ChaCha20Rng) -> Result<String, EmitError> {
    let mut opmap_seed = [0u8; 32];
    use rand::RngCore;
    rng.fill_bytes(&mut opmap_seed);
    let opmap = opmap::OpMap::new(&opmap_seed);
    render::render(program, &opmap, rng)
}
```

Delete the now-unused `use rand::SeedableRng;` if the file no longer references it.

- [ ] **Step 2: Update the emit unit tests**

The two tests in the same file currently call `emit(&lir, [0u8; 32])`. Change them to:

```rust
    fn compile_to_luau(src: &str) -> String {
        let ast = luau_parse::parse(src).unwrap();
        let hir = hir_lower(&ast).unwrap();
        let mir = mir_lower(&hir).unwrap();
        let lir = lir_lower(&mir).unwrap();
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        emit(&lir, &mut rng).unwrap()
    }
```

Add `use rand::SeedableRng;` to the test mod if needed.

- [ ] **Step 3: Update the facade to pass the shared rng to emit**

In `/Volumes/External Drive/luau-obf/crates/luau-obf/src/lib.rs`, replace the body of `obfuscate`:

```rust
pub fn obfuscate(source: &str, opts: Options) -> Result<ObfuscateResult, Error> {
    let seed = opts.seed.unwrap_or_else(random_seed);
    let mut rng = ChaCha20Rng::from_seed(seed);

    let ast = luau_parse::parse(source)?;
    let hir = luau_hir::lower::lower(&ast)?;
    let mut mir = luau_mir::lower::lower(&hir)?;
    let plan = luau_passes::default_plan();
    plan.run(&mut mir, &mut rng);
    let lir = luau_lir::lower::lower(&mir)?;
    let output = luau_emit::emit(&lir, &mut rng)?;
    Ok(ObfuscateResult { output, seed_used: seed })
}
```

- [ ] **Step 4: Verify the workspace still builds and all tests still pass**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass. The output is byte-identical to before because:
- `OpMap::new` still ignores its seed (we haven't changed it yet — that's Task 2).
- No new randomization yet.

If any test fails, do NOT commit. Report BLOCKED.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-emit/src/lib.rs crates/luau-obf/src/lib.rs
git commit -m "refactor(emit): take &mut ChaCha20Rng instead of seed bytes"
```

---

## Task 2: Seeded opcode renumbering

**Files:**
- Modify: `crates/luau-emit/src/opmap.rs`
- Modify: `crates/luau-emit/Cargo.toml` (only if `rand` / `rand_chacha` aren't already deps)

- [ ] **Step 1: Confirm `rand` and `rand_chacha` are available to `luau-emit`**

Check `/Volumes/External Drive/luau-obf/crates/luau-emit/Cargo.toml`. If both `rand` and `rand_chacha` are not already listed under `[dependencies]`, add them. Use the version already used elsewhere in the workspace (check the workspace `Cargo.toml` or `luau-obf/Cargo.toml`).

- [ ] **Step 2: Rewrite `OpMap::new`**

Replace the body of `OpMap::new` in `/Volumes/External Drive/luau-obf/crates/luau-emit/src/opmap.rs`:

```rust
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

impl OpMap {
    pub fn new(seed: &[u8; 32]) -> Self {
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let mut bytes: Vec<u8> = (1..=ALL_OPS.len() as u8).collect();
        bytes.shuffle(&mut rng);
        let mut forward = [0u8; 35];
        for (i, b) in bytes.into_iter().enumerate() {
            forward[i] = b;
        }
        OpMap { forward }
    }
    // ... existing opcode_of / assigned_pairs methods unchanged ...
}
```

Drop the previous body. Keep `opcode_of` and `assigned_pairs` exactly as they are.

(The `use` lines go at the top of the file.)

- [ ] **Step 3: Add tests**

Append to `opmap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_mapping() {
        let m1 = OpMap::new(&[7u8; 32]);
        let m2 = OpMap::new(&[7u8; 32]);
        for k in ALL_OPS {
            assert_eq!(m1.opcode_of(*k), m2.opcode_of(*k));
        }
    }

    #[test]
    fn different_seeds_produce_different_mappings() {
        let m1 = OpMap::new(&[1u8; 32]);
        let m2 = OpMap::new(&[2u8; 32]);
        // At least one opcode must differ between the two mappings.
        let any_diff = ALL_OPS.iter().any(|k| m1.opcode_of(*k) != m2.opcode_of(*k));
        assert!(any_diff, "different seeds produced identical opmap");
    }

    #[test]
    fn opcodes_are_a_permutation_of_1_to_n() {
        let m = OpMap::new(&[42u8; 32]);
        let mut bytes: Vec<u8> = ALL_OPS.iter().map(|k| m.opcode_of(*k)).collect();
        bytes.sort_unstable();
        let expected: Vec<u8> = (1..=ALL_OPS.len() as u8).collect();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn opcode_zero_is_never_assigned() {
        // 0 is reserved as a halt sentinel.
        let m = OpMap::new(&[99u8; 32]);
        for k in ALL_OPS {
            assert_ne!(m.opcode_of(*k), 0);
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

This is the moment of truth: every existing test that compiles a Luau program through the pipeline now goes through a permuted opmap. The bytecode bytes will be different, BUT semantically equivalent (because the VM template gets the matching `local OP_X = <byte>` definitions). Every corpus differential test should still pass.

If a corpus test fails: read its diff. The most likely cause is a missing opcode in `ALL_OPS` or a hard-coded opcode byte somewhere. Search the codebase for any literal `1`, `2`, etc. compared against an opcode (`if op == 1`) — that would be a latent bug exposed by the shuffle. Report BLOCKED with the specifics.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-emit/src/opmap.rs crates/luau-emit/Cargo.toml
git commit -m "feat(emit): seeded per-build opcode renumbering"
```

---

## Task 3: Constant-pool shuffling

**Files:**
- Create: `crates/luau-lir/src/shuffle.rs`
- Modify: `crates/luau-lir/src/lib.rs`
- Modify: `crates/luau-lir/Cargo.toml` (only if `rand` / `rand_chacha` aren't already deps)
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Confirm `rand` and `rand_chacha` are available to `luau-lir`**

Check `/Volumes/External Drive/luau-obf/crates/luau-lir/Cargo.toml`. Add `rand` and `rand_chacha` deps if missing.

- [ ] **Step 2: Create `crates/luau-lir/src/shuffle.rs`**

```rust
//! LIR-level constant-pool shuffling.
//!
//! For each function, computes a permutation σ over its constant pool indices,
//! reorders the consts vec accordingly, and rewrites every `Operand::Const`
//! reference to point at the new index. Pure relabelling — the program's
//! runtime behavior is identical, but the bytes in the constant pool and the
//! ConstIdx operands embedded in the bytecode are different.

use crate::{ConstIdx, LirProgram, Operand};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha20Rng;

pub fn shuffle_constants(program: &mut LirProgram, rng: &mut ChaCha20Rng) {
    for f in &mut program.functions {
        let n = f.consts.len();
        if n <= 1 {
            continue;
        }
        // sigma[i] = new position of what was at old position i.
        let mut sigma: Vec<usize> = (0..n).collect();
        sigma.shuffle(rng);
        // Reorder consts: new[sigma[i]] = old[i].
        let mut new_consts = vec![f.consts[0].clone(); n];
        for (old_i, c) in f.consts.iter().enumerate() {
            new_consts[sigma[old_i]] = c.clone();
        }
        f.consts = new_consts;
        // Rewrite every Operand::Const(ConstIdx(i)) → ConstIdx(sigma[i]).
        for instr in &mut f.instrs {
            for operand in &mut instr.operands {
                if let Operand::Const(ConstIdx(i)) = operand {
                    let old = *i as usize;
                    *i = sigma[old] as u16;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LirFunction, LirInstr, OpKind, ProtoId, Reg};
    use luau_mir::Constant;
    use rand::SeedableRng;

    fn make_program() -> LirProgram {
        // A single function with 3 constants and one LoadConst per constant.
        let f = LirFunction {
            id: ProtoId(0),
            num_params: 0,
            num_regs: 3,
            num_upvals: 0,
            is_vararg: false,
            consts: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(30.0),
            ],
            instrs: vec![
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(0)), Operand::Const(ConstIdx(0))] },
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(1)), Operand::Const(ConstIdx(1))] },
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(2)), Operand::Const(ConstIdx(2))] },
            ],
            label_positions: vec![],
            closure_upval_sources: vec![],
            build_results_values: vec![],
        };
        LirProgram { functions: vec![f] }
    }

    fn collect_loaded_pairs(p: &LirProgram) -> Vec<(u16, Constant)> {
        // For each LoadConst, return (dst_reg, the actual constant it loads).
        let f = &p.functions[0];
        let mut pairs = Vec::new();
        for i in &f.instrs {
            if i.op != OpKind::LoadConst {
                continue;
            }
            let Operand::Reg(Reg(r)) = i.operands[0] else { unreachable!() };
            let Operand::Const(ConstIdx(c)) = i.operands[1] else { unreachable!() };
            pairs.push((r, f.consts[c as usize].clone()));
        }
        pairs
    }

    #[test]
    fn shuffle_preserves_constant_per_register() {
        // For each register, the constant it loads must be the same before and after.
        let mut before = make_program();
        let pairs_before = collect_loaded_pairs(&before);
        let mut rng = ChaCha20Rng::from_seed([5u8; 32]);
        shuffle_constants(&mut before, &mut rng);
        let pairs_after = collect_loaded_pairs(&before);
        assert_eq!(pairs_before, pairs_after);
    }

    #[test]
    fn shuffle_actually_reorders_for_typical_seed() {
        let mut p = make_program();
        let consts_before = p.functions[0].consts.clone();
        let mut rng = ChaCha20Rng::from_seed([99u8; 32]);
        shuffle_constants(&mut p, &mut rng);
        // Very high probability the order changed (only 1/6 chance of identity for n=3).
        assert_ne!(p.functions[0].consts, consts_before);
    }

    #[test]
    fn shuffle_is_deterministic_for_same_seed() {
        let mut p1 = make_program();
        let mut p2 = make_program();
        let mut r1 = ChaCha20Rng::from_seed([3u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([3u8; 32]);
        shuffle_constants(&mut p1, &mut r1);
        shuffle_constants(&mut p2, &mut r2);
        assert_eq!(p1.functions[0].consts, p2.functions[0].consts);
    }
}
```

- [ ] **Step 3: Expose the module**

In `/Volumes/External Drive/luau-obf/crates/luau-lir/src/lib.rs`, add (alongside the existing `pub mod` declarations):

```rust
pub mod shuffle;
```

- [ ] **Step 4: Wire it into the facade**

In `/Volumes/External Drive/luau-obf/crates/luau-obf/src/lib.rs`, modify `obfuscate` to call `shuffle_constants` between LIR lowering and emit:

```rust
pub fn obfuscate(source: &str, opts: Options) -> Result<ObfuscateResult, Error> {
    let seed = opts.seed.unwrap_or_else(random_seed);
    let mut rng = ChaCha20Rng::from_seed(seed);

    let ast = luau_parse::parse(source)?;
    let hir = luau_hir::lower::lower(&ast)?;
    let mut mir = luau_mir::lower::lower(&hir)?;
    let plan = luau_passes::default_plan();
    plan.run(&mut mir, &mut rng);
    let mut lir = luau_lir::lower::lower(&mir)?;
    luau_lir::shuffle::shuffle_constants(&mut lir, &mut rng);
    let output = luau_emit::emit(&lir, &mut rng)?;
    Ok(ObfuscateResult { output, seed_used: seed })
}
```

- [ ] **Step 5: Run the full workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

The bytecode now has shuffled const indices AND permuted opcode bytes. Every corpus program must still produce identical stdout between plain and obfuscated runs — if any fails, the bug is in `shuffle_constants` (most likely: missed an `Operand::Const` site somewhere).

- [ ] **Step 6: Commit**

```bash
git add crates/luau-lir/src/shuffle.rs crates/luau-lir/src/lib.rs crates/luau-lir/Cargo.toml crates/luau-obf/src/lib.rs
git commit -m "feat(lir): seeded constant-pool shuffling"
```

---

## Task 4: Determinism + variance tests at the facade and CLI

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`
- Modify: `crates/luau-obf-cli/tests/cli.rs`

- [ ] **Step 1: Add a variance test in `crates/luau-obf/src/lib.rs`**

Append to the `tests` mod:

```rust
    #[test]
    fn different_seeds_produce_different_outputs() {
        let src = "local x = 1 + 2 print(x)";
        let a = obfuscate(src, Options { seed: Some([1u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([2u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }

    #[test]
    fn three_seeds_produce_three_distinct_outputs() {
        let src = "local function add(a, b) return a + b end print(add(3, 4))";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add a CLI variance test**

In `/Volumes/External Drive/luau-obf/crates/luau-obf-cli/tests/cli.rs`, append a test. Pattern it on the existing `cli_accepts_explicit_seed` test (which uses two equal seeds and asserts identical output). The new test uses two different seeds and asserts NON-identical output:

```rust
#[test]
fn cli_different_seeds_produce_different_outputs() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let out_a = dir.path().join("a.luau");
    let out_b = dir.path().join("b.luau");
    std::fs::write(&input, "print(1) print(2) print(3)").unwrap();
    let seed_a = "1111111111111111111111111111111111111111111111111111111111111111";
    let seed_b = "2222222222222222222222222222222222222222222222222222222222222222";
    let status_a = Command::new(env!("CARGO_BIN_EXE_luau-obf"))
        .arg(&input).arg("-o").arg(&out_a).arg("--seed").arg(seed_a).arg("--quiet")
        .status().unwrap();
    let status_b = Command::new(env!("CARGO_BIN_EXE_luau-obf"))
        .arg(&input).arg("-o").arg(&out_b).arg("--seed").arg(seed_b).arg("--quiet")
        .status().unwrap();
    assert!(status_a.success() && status_b.success());
    let a = std::fs::read_to_string(&out_a).unwrap();
    let b = std::fs::read_to_string(&out_b).unwrap();
    assert_ne!(a, b, "different seeds must produce different output");
}
```

If the existing test imports for `tempfile`, `Command`, or `env!` aren't already in scope, add the necessary `use` lines at the top of the file.

- [ ] **Step 3: Run all tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass, including the two new facade tests and the new CLI test. The CLI test exercises the full pipeline including the random-seed generation path.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs crates/luau-obf-cli/tests/cli.rs
git commit -m "test: per-build variance across seeds"
```

---

## Task 5: README status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 7 — first per-build obfuscation transforms. Same source under
two different `--seed` values now produces materially different obfuscated
output: opcode bytes are renumbered per build, and each function's constant
pool is shuffled per build. Semantics preserved; the full differential corpus
(Plans 1–6) passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 7 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All corpus programs from Plans 1–6 pass differential testing.
- [ ] `same_seed_produces_same_mapping` test passes.
- [ ] `different_seeds_produce_different_mappings` test passes.
- [ ] `different_seeds_produce_different_outputs` test passes.
- [ ] CLI test `cli_different_seeds_produce_different_outputs` passes.
- [ ] No opcode byte equals `0` in any permutation (the test asserts this for one seed; trust the algorithm for the rest).
- [ ] The previously-passing CLI determinism test (`cli_accepts_explicit_seed`) still passes — same seed → identical output.
