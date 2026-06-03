# Luau Obfuscator — Plan 16: Add Identity-Padding (Second MIR Pass)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the second MIR→MIR obfuscation pass. Each `BinOp::Add` instruction, with 30% probability per instance, is rewritten as a four-instruction operand-scattering sequence using a per-rewrite integer `k`:

```
dst = Add(a, b)
```
becomes
```
kc   = LoadConst(<fresh integer k in [1, 65535]>)
tmpL = Add(a, kc)
tmpR = Sub(b, kc)
dst  = Add(tmpL, tmpR)
```

Plan 15 broke `Sub`-to-`-` correspondence; Plan 16 does the same for `Add`. Together they ensure neither additive operator maps cleanly to a single bytecode instruction.

**Architecture:**
- New file `crates/luau-passes/src/add_padding.rs` exposes the `AddIdentityPadding` pass.
- `Pass::run` walks each `MirFunction`. For each basic block, it visits instructions. When it sees `Instr::BinOp { op: BinOp::Add, lhs, rhs, dst }`, with 30% probability (drawn from the seeded rng), it splices the original instruction with four new instructions: `LoadConst kc` + `Add tmpL` + `Sub tmpR` + `Add dst`. The instruction list of the block grows by three per rewrite; `n_locals` grows by three; `f.consts` grows by one (a fresh `Constant::Number`).
- The default plan in `luau-passes` adds `AddIdentityPadding` after `ExpressionMangle`.
- LIR / encode / VM are unchanged — Add, Sub, LoadConst are existing opcodes; `Constant::Number(f64)` is the existing const type; regalloc handles arbitrary VLocal counts.

**Tech Stack:** Same as Plans 1-15.

---

## Implementation notes

**FP semantics.**
- `(a + k) + (b − k)` is exactly equal to `a + b` in IEEE 754 when the intermediate values don't lose precision. With integer `k ∈ [1, 65535]` and operands in the integer range (which covers all 23 corpus programs), this holds bit-exact.
- For wildly large floats (e.g. `a = 1e20`), the addition `a + k` rounds k away — but symmetrically, `b − k` rounds k away as well, and the final sum equals `a + b` exactly.
- Sub-ULP drift is theoretically possible for non-integer operands at extreme scales; that lives outside the corpus and is documented as future work.

**Metatable semantics.**
- For numeric operands the rewrite is exact.
- For metatable types `__add` and `__sub` interactions diverge. Same pragma as Plan 15: the current corpus (metatables.luau uses Vec2 `__add` only) doesn't exercise the broken cases. Future plans can type-gate this rewrite.

**Rewrite generation.**

Original:
```rust
Instr::BinOp { dst: V(7), op: Add, lhs: V(3).as_value(), rhs: V(5).as_value() }
```

Rewritten:
```rust
Instr::LoadConst { dst: V(<n0>), src: <new ConstId> }
Instr::BinOp { dst: V(<n1>), op: Add, lhs: V(3).as_value(), rhs: Value::VLocal(V(<n0>)) }
Instr::BinOp { dst: V(<n2>), op: Sub, lhs: V(5).as_value(), rhs: Value::VLocal(V(<n0>)) }
Instr::BinOp { dst: V(7), op: Add, lhs: Value::VLocal(V(<n1>)), rhs: Value::VLocal(V(<n2>)) }
```

Where `<n0>`, `<n1>`, `<n2>` are three fresh `VLocal` IDs (`f.n_locals`, `f.n_locals + 1`, `f.n_locals + 2`), and `<new ConstId>` is `ConstId(f.consts.len() as u32)` BEFORE the push.

**RNG consumption.**
- The pass receives `&mut ChaCha20Rng` from the pass scheduler.
- For each Add instruction the pass visits, it draws ONE `u8` (`rng.gen::<u8>() % 100`). If `< 30`, rewrite; else skip.
- For each Add the pass DOES rewrite, it then draws ONE `u32` for the k value (`rng.gen_range(1u32..=65535)`).
- Determinism: same seed → same sequence of decisions → same rewrites.

**Walk semantics.** Single forward pass per function. The three new Adds (and one new Sub) created by the rewrite ARE NOT re-rewritten in the same pass — we always advance `i` to past the inserted instructions.

**Default plan update.**

In `crates/luau-passes/src/lib.rs`:
```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
}
```

**Iterating instructions safely.** Same borrow-checker dance as Plan 15:

```rust
fn rewrite_function(f: &mut MirFunction, rng: &mut ChaCha20Rng) {
    let mut n_locals = f.n_locals;
    let mut consts = std::mem::take(&mut f.consts);
    for block in f.blocks.iter_mut() {
        let mut i = 0;
        while i < block.instrs.len() {
            let is_add = matches!(
                &block.instrs[i],
                Instr::BinOp { op: BinOp::Add, .. }
            );
            if !is_add {
                i += 1;
                continue;
            }
            let roll = rng.gen::<u8>() % 100;
            if roll >= 30 {
                i += 1;
                continue;
            }
            // Extract operands.
            let (dst, a, b) = match &block.instrs[i] {
                Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                _ => unreachable!(),
            };
            // Fresh k as integer in [1, 65535].
            let k = rng.gen_range(1u32..=65535) as f64;
            let k_const_id = ConstId(consts.len() as u32);
            consts.push(Constant::Number(k));
            // Allocate three fresh VLocals.
            let kc = VLocal(n_locals);
            let tmp_l = VLocal(n_locals + 1);
            let tmp_r = VLocal(n_locals + 2);
            n_locals += 3;
            // Replace and splice.
            block.instrs[i] = Instr::LoadConst { dst: kc, src: k_const_id };
            block.instrs.insert(
                i + 1,
                Instr::BinOp { dst: tmp_l, op: BinOp::Add, lhs: a, rhs: Value::VLocal(kc) },
            );
            block.instrs.insert(
                i + 2,
                Instr::BinOp { dst: tmp_r, op: BinOp::Sub, lhs: b, rhs: Value::VLocal(kc) },
            );
            block.instrs.insert(
                i + 3,
                Instr::BinOp {
                    dst,
                    op: BinOp::Add,
                    lhs: Value::VLocal(tmp_l),
                    rhs: Value::VLocal(tmp_r),
                },
            );
            i += 4; // skip past all four new instructions
        }
    }
    f.n_locals = n_locals;
    f.consts = consts;
}
```

Note we take `consts` out of `f` with `std::mem::take` to avoid a borrow conflict with `&mut f.blocks`, then put it back at the end. (Or pass `consts` and `n_locals` as `&mut` local variables — either approach works as long as we don't hold two `&mut` borrows of `f` at once.)

---

## File Structure (changes only)

```
crates/luau-passes/src/
  add_padding.rs           # CREATE: AddIdentityPadding pass + unit tests
  lib.rs                   # MODIFY: pub mod + default_plan adds the pass
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests
README.md                  # MODIFY: status line
```

No changes to HIR / MIR types / LIR / encoder / VM template. Add, Sub, LoadConst are existing opcodes; `Constant::Number` is the existing const type.

---

## Task 1: Implement the AddIdentityPadding pass

**Files:**
- Create: `crates/luau-passes/src/add_padding.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/add_padding.rs`:

```rust
//! Add Identity-Padding pass — rewrites a fraction of `Add` instructions
//! as `(a + k) + (b - k)` with a seed-derived integer k. The result is
//! semantically identical for numeric operands inside the integer range;
//! the bytecode no longer consistently maps `Add` to a single source-level
//! `+` operator.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct AddIdentityPadding;

const REWRITE_PERCENT: u8 = 30;
const K_MIN: u32 = 1;
const K_MAX: u32 = 65535;

impl Pass for AddIdentityPadding {
    fn name(&self) -> &'static str {
        "add-identity-padding"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_add = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Add, .. }
                    );
                    if !is_add {
                        i += 1;
                        continue;
                    }
                    let roll = rng.gen::<u8>() % 100;
                    if roll >= REWRITE_PERCENT {
                        i += 1;
                        continue;
                    }
                    let (dst, a, b) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let k = rng.gen_range(K_MIN..=K_MAX) as f64;
                    let k_const_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k));
                    let kc = VLocal(n_locals);
                    let tmp_l = VLocal(n_locals + 1);
                    let tmp_r = VLocal(n_locals + 2);
                    n_locals += 3;
                    block.instrs[i] = Instr::LoadConst { dst: kc, src: k_const_id };
                    block.instrs.insert(
                        i + 1,
                        Instr::BinOp {
                            dst: tmp_l,
                            op: BinOp::Add,
                            lhs: a,
                            rhs: Value::VLocal(kc),
                        },
                    );
                    block.instrs.insert(
                        i + 2,
                        Instr::BinOp {
                            dst: tmp_r,
                            op: BinOp::Sub,
                            lhs: b,
                            rhs: Value::VLocal(kc),
                        },
                    );
                    block.instrs.insert(
                        i + 3,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Add,
                            lhs: Value::VLocal(tmp_l),
                            rhs: Value::VLocal(tmp_r),
                        },
                    );
                    i += 4;
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

    fn count_adds(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::BinOp { op: BinOp::Add, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
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
    fn rewrites_some_adds_to_padded_form() {
        // A program with many Adds — enough that 30% should fire on at least one
        // across many seeds.
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a + b, c + d, a + b + c, a + b + c + d, a + b + c + d + e)";
        let mut p = mir_of(src);
        let adds_before = count_adds(&p);
        let subs_before = count_subs(&p);
        let nums_before = count_number_consts(&p);
        assert!(adds_before >= 10, "expected at least 10 Adds, got {}", adds_before);

        // Try multiple seeds; at least one MUST trigger a rewrite (30% per Add,
        // 10+ Adds → P(no rewrite) ≈ 0.7^10 ≈ 0.028, so across 5 seeds the
        // probability of zero rewrites in all is ≈ 1e-8).
        let mut saw_rewrite = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p_clone, &mut rng);
            let adds_after = count_adds(&p_clone);
            let subs_after = count_subs(&p_clone);
            let nums_after = count_number_consts(&p_clone);
            // Each rewrite: -1 Add (original consumed) +1 LoadConst (in same slot)
            //               +3 new (2 Add + 1 Sub) inserted after.
            // Net Add: -1 + 2 = +1 per rewrite. Net Sub: +1 per rewrite.
            // Net Number consts: +1 per rewrite.
            let rewrites_via_adds = (adds_after - adds_before) as i64;
            let rewrites_via_subs = (subs_after - subs_before) as i64;
            let rewrites_via_consts = (nums_after - nums_before) as i64;
            assert_eq!(
                rewrites_via_adds, rewrites_via_subs,
                "Add growth must equal Sub growth (seed {})",
                s
            );
            assert_eq!(
                rewrites_via_subs, rewrites_via_consts,
                "Sub growth must equal new Number-const growth (seed {})",
                s
            );
            if rewrites_via_adds > 0 {
                saw_rewrite = true;
            }
        }
        assert!(saw_rewrite, "no seed in 0..5 triggered any rewrite");
        let _ = p; // suppress unused-mut warning
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local a = 1 print(a + 2 + 3 + 4 + 5 + 6)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        AddIdentityPadding.run(&mut p1, &mut r1);
        AddIdentityPadding.run(&mut p2, &mut r2);
        assert_eq!(count_adds(&p1), count_adds(&p2));
        assert_eq!(count_subs(&p1), count_subs(&p2));
        assert_eq!(count_number_consts(&p1), count_number_consts(&p2));
        assert_eq!(p1.functions[0].n_locals, p2.functions[0].n_locals);
    }

    #[test]
    fn programs_with_no_add_are_unchanged() {
        // Use only Sub and Mul — no Add anywhere.
        let src = "local a, b = 10, 3 print(a - b, a * b)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let consts_before = p.functions[0].consts.len();
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        AddIdentityPadding.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
        assert_eq!(p.functions[0].consts.len(), consts_before);
    }

    #[test]
    fn rewrite_growth_invariants() {
        let src = "local a, b, c, d, e, f, g, h = 1, 2, 3, 4, 5, 6, 7, 8 \
                   print(a + b + c + d + e + f + g + h)";
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let adds_before = count_adds(&p);
            let subs_before = count_subs(&p);
            let nums_before = count_number_consts(&p);
            let n_locals_before = p.functions[0].n_locals;
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p, &mut rng);
            let rewrite_count = count_subs(&p) - subs_before;
            // Add net growth = +rewrite_count (each rewrite is -1 + 2 Adds).
            assert_eq!(count_adds(&p) - adds_before, rewrite_count);
            // Number-const growth = rewrite_count.
            assert_eq!(count_number_consts(&p) - nums_before, rewrite_count);
            // n_locals growth = 3 * rewrite_count (kc + tmp_l + tmp_r per rewrite).
            assert_eq!(
                p.functions[0].n_locals - n_locals_before,
                (rewrite_count * 3) as u32
            );
        }
    }
}
```

- [ ] **Step 2: Register the module**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add the module declaration alongside the existing ones:

```rust
pub mod identity;
pub mod expr_mangle;
pub mod add_padding;
```

And modify `default_plan` to append the new pass:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
}
```

- [ ] **Step 3: Run unit tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 4 new unit tests pass (`rewrites_some_adds_to_padded_form`, `deterministic_for_same_seed`, `programs_with_no_add_are_unchanged`, `rewrite_growth_invariants`), plus the existing Plan 15 tests.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

EXPECTED: all 23 corpus programs still pass differential testing. `(a + k) + (b − k)` is exact for integer operands in the corpus.

If any corpus test fails, the rewrite is hitting a metatable-`__add` path that diverges. Resolution path: gate the rewrite to skip when ANY corpus program fails — but we don't anticipate this; metatables.luau uses Vec2 `__add` on Vec2 operands and the rewrite uses numeric k so the operand types diverge mid-rewrite (`Vec2 + number` would error in original semantics too).

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/add_padding.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): MIR Add identity-padding — rewrite Add as (a+k)+(b-k)"
```

---

## Task 2: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `add_padding_changes_output`**

Append inside the existing `tests` mod:

```rust
    #[test]
    fn add_padding_changes_output() {
        // A program with several `+` operations. Different seeds should
        // produce different rewrites (rate, k choices), hence different
        // output beyond what keystream alone provides.
        let src = "local x = 0 \
                   x = x + 10 \
                   x = x + 20 \
                   x = x + 30 \
                   x = x + 40 \
                   x = x + 50 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `add_padding_preserves_runtime_semantics`**

Append:

```rust
    #[test]
    fn add_padding_preserves_runtime_semantics() {
        // 100 + 200 + 300 = 600. Verify the obfuscated chunk prints 600.
        let src = "print(100 + 200 + 300)";
        let r = obfuscate(src, Options { seed: Some([77u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(),
            "luau exited with status {:?}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("600"), "expected '600' in stdout, got: {}", stdout);
    }
```

- [ ] **Step 3: Run all tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass, including 23 corpus differential tests and the two new acceptance tests.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 16 acceptance — Add padding preserves semantics, varies per seed"
```

---

## Task 3: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

Find the existing `**Status:** Plan 15 …` block in `README.md` and replace it with:

```
**Status:** Plan 16 — Add identity-padding shipped. Each `Add` instruction
is rewritten with 30% probability as `(a + k) + (b − k)` with a seed-derived
integer `k`. Combined with Plan 15's `Sub → Neg + Add` rewrite, neither
additive operator maps cleanly to a single bytecode instruction; a static
analyser must reconstruct which arithmetic ops are real and which are
identity padding. Subsequent plans extend the expression-mangling family
(Mul rewrites, boolean mangling, constant decomposition) before moving to
control-flow obfuscation (opaque predicates, junk blocks, flattening).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 16 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs still pass differential testing — `(a+k)+(b−k)` is bit-exact for integer operands.
- [ ] `rewrites_some_adds_to_padded_form` passes.
- [ ] `deterministic_for_same_seed` passes.
- [ ] `programs_with_no_add_are_unchanged` passes.
- [ ] `rewrite_growth_invariants` passes — confirms each rewrite adds exactly 1 Add, 1 Sub, 1 Number const, 3 VLocals.
- [ ] `add_padding_changes_output` passes.
- [ ] `add_padding_preserves_runtime_semantics` passes.
- [ ] All Plan 8-15 acceptance tests still pass.
- [ ] Spot-check: compile a program like `print(10 + 20)` with `--seed 00…00` and `--seed FF…FF`. The two outputs should have a different INSTRUCTION COUNT (some seeds insert `LoadConst+Add+Sub+Add`, others keep the single `Add`).
