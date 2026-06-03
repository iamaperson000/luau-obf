# Luau Obfuscator — Plan 17: Mul Operand-Scatter (Third MIR Pass)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Land the third MIR→MIR obfuscation pass. Each `BinOp::Mul` instruction whose operands are **provably numeric**, with 30% probability per instance, is rewritten as a four-instruction operand-scattering sequence:

```
dst = Mul(x, y)
```
becomes
```
kc   = LoadConst(<fresh integer k in [1, 65535]>)
tmpA = Add(x, kc)         -- x + k
tmpM = Mul(tmpA, y)       -- (x + k) * y  =  xy + ky
tmpK = Mul(kc, y)          -- k * y
dst  = Sub(tmpM, tmpK)    -- xy + ky - ky  =  xy
```

Plan 16 broke `Add`-to-`+` correspondence by operand-scattering Adds. Plan 17 does the same for `Mul`. Together with Plans 15 and 16, no source-level arithmetic operator (`+`, `-`, `*`) maps cleanly to a single bytecode instruction.

**Architecture:**
- Refactor first: extract the per-block numeric-VLocal dataflow analysis from `add_padding.rs` into a new shared module `crates/luau-passes/src/analysis/numeric.rs`. Both `add_padding` and the new `mul_padding` import from it. This keeps logic in one place and lets every future arithmetic-mangling pass reuse it.
- New file `crates/luau-passes/src/mul_padding.rs` exposes `pub struct MulOperandScatter;` with `impl Pass`.
- `default_plan` adds `MulOperandScatter` after `AddIdentityPadding`.
- LIR / encode / VM unchanged — Add, Sub, Mul, LoadConst are existing opcodes.

**Tech Stack:** Same as Plans 1-16.

---

## Implementation notes

**FP semantics.** `(x + k) * y − k * y = xy + ky − ky = xy`. The subtraction `ky − ky` is exact in IEEE 754 (subtracting a value from itself yields 0). The result `xy + 0 = xy` is exact. With integer k in `[1, 65535]` and operands inside the integer range (which covers all 23 corpus programs), the rewrite is bit-exact.

**Metatable safety.** Same hazard as Plan 16 — `Vec2 * Vec2` or any `Mul` invoking `__mul` would break if we substituted a numeric operand. The numeric-VLocal gate (extracted from Plan 16) protects against this. If either operand isn't provably numeric, skip.

**RNG consumption.** One `u8` per Mul visited. One `u32` (k draw) per Mul actually rewritten.

**Walk semantics.** Single forward pass per function. The new Muls and the new Sub created by the rewrite are skipped via `i += 5` (one replaced instruction + four inserted = five total). Net effect: one Mul replaced, three new Muls/Adds inserted, one new Sub inserted = +4 instructions, +4 VLocals (`kc`, `tmpA`, `tmpM`, `tmpK`), +1 numeric constant.

**Wait, that's 4 inserts not 3.** Let me re-count the rewrite:
- Replace original Mul with `LoadConst kc` (1 replacement, 0 net change in length).
- Insert at i+1: `Add tmpA = x + kc`.
- Insert at i+2: `Mul tmpM = tmpA * y`.
- Insert at i+3: `Mul tmpK = kc * y`.
- Insert at i+4: `Sub dst = tmpM - tmpK`.

So 4 inserts after the replacement. Total new instructions: 5 at positions i..i+4. Advance `i += 5`.

Per rewrite: +4 instructions vs original (5 total replacing 1), +4 VLocals, +1 numeric constant.

**Default plan update.**

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
}
```

---

## File Structure (changes only)

```
crates/luau-passes/src/
  analysis/
    mod.rs                 # CREATE: pub mod numeric;
    numeric.rs             # CREATE: shared per-block numeric-VLocal dataflow
  add_padding.rs           # MODIFY: import from analysis::numeric (delete in-file dup)
  mul_padding.rs           # CREATE: MulOperandScatter pass + unit tests
  lib.rs                   # MODIFY: pub mod analysis; pub mod mul_padding;
                           #         default_plan adds MulOperandScatter
crates/luau-obf/src/
  lib.rs                   # MODIFY: two acceptance tests for Mul scatter
README.md                  # MODIFY: status line
```

---

## Task 1: Extract the numeric-VLocal analyzer to a shared module

**Files:**
- Create: `crates/luau-passes/src/analysis/mod.rs`
- Create: `crates/luau-passes/src/analysis/numeric.rs`
- Modify: `crates/luau-passes/src/add_padding.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the analysis module root**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/analysis/mod.rs`:

```rust
//! Shared dataflow / static-analysis utilities used by multiple passes.

pub mod numeric;
```

- [ ] **Step 2: Create the numeric analyzer**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/analysis/numeric.rs`:

```rust
//! Intra-block forward dataflow tracking which `VLocal`s are
//! **provably numeric** at each point in a basic block.
//!
//! A VLocal is provably numeric if its most-recent same-block def is:
//! - `LoadConst Number`,
//! - a numeric arithmetic `BinOp` (`Add`/`Sub`/`Mul`/`Div`/`Mod`/`Pow`) on
//!   numeric operands,
//! - `UnOp::Neg` on a numeric operand,
//! - `UnOp::Len` (always numeric in Lua/Luau), or
//! - `Move` from a provably-numeric VLocal.
//!
//! Anything else (parameters, prior-block values, `Call` returns,
//! `GetIndex`, `GetGlobal`, `GetUpval`, `NewTable`, `BuildResults`,
//! `MakeClosure`, `GetVarargs`) is conservatively NOT provably numeric.
//!
//! Passes that materialise synthetic numeric operands into existing
//! arithmetic (e.g. `(x + k) * y − k * y`) MUST gate on this set —
//! otherwise the rewrite changes operand types and crashes user code that
//! overloads `__add` / `__mul` / etc.

use luau_hir::{BinOp, UnOp};
use luau_mir::{Constant, Instr, Value};
use std::collections::HashSet;

/// Set of VLocal indices known to be numeric at the current point in a
/// basic block. Construct one per block, fresh.
#[derive(Default, Debug)]
pub struct NumericLocals {
    set: HashSet<u32>,
}

impl NumericLocals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, vlocal_idx: u32) -> bool {
        self.set.contains(&vlocal_idx)
    }

    pub fn insert(&mut self, vlocal_idx: u32) {
        self.set.insert(vlocal_idx);
    }

    pub fn remove(&mut self, vlocal_idx: u32) {
        self.set.remove(&vlocal_idx);
    }

    /// Whether the given `Value` is provably numeric at this point.
    pub fn is_value_numeric(&self, v: &Value, consts: &[Constant]) -> bool {
        match v {
            Value::Const(c) => {
                matches!(consts.get(c.0 as usize), Some(Constant::Number(_)))
            }
            Value::VLocal(v) => self.set.contains(&v.0),
        }
    }

    /// Apply the effect of executing `instr` to the numeric set.
    /// Insert or remove `dst` based on whether the produced value is
    /// provably numeric.
    pub fn update(&mut self, instr: &Instr, consts: &[Constant]) {
        let (dst, is_num) = match instr {
            Instr::LoadConst { dst, src } => {
                let n = matches!(
                    consts.get(src.0 as usize),
                    Some(Constant::Number(_))
                );
                (Some(*dst), n)
            }
            Instr::BinOp { dst, op, lhs, rhs } => {
                let numeric_op = matches!(
                    op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
                );
                let n = numeric_op
                    && self.is_value_numeric(lhs, consts)
                    && self.is_value_numeric(rhs, consts);
                (Some(*dst), n)
            }
            Instr::UnOp { dst, op, operand } => {
                let n = match op {
                    UnOp::Len => true,
                    UnOp::Neg => self.is_value_numeric(operand, consts),
                    _ => false,
                };
                (Some(*dst), n)
            }
            Instr::Move { dst, src } => {
                let n = self.set.contains(&src.0);
                (Some(*dst), n)
            }
            Instr::Call { dst: Some(d), .. } | Instr::CallVar { dst: Some(d), .. } => {
                (Some(*d), false)
            }
            Instr::Call { dst: None, .. } | Instr::CallVar { dst: None, .. } => (None, false),
            Instr::GetGlobal { dst, .. } => (Some(*dst), false),
            Instr::GetIndex { dst, .. } => (Some(*dst), false),
            Instr::GetUpval { dst, .. } => (Some(*dst), false),
            Instr::GetVarargs { dst } => (Some(*dst), false),
            Instr::NewTable { dst } => (Some(*dst), false),
            Instr::BuildResults { dst, .. } => (Some(*dst), false),
            Instr::MakeClosure { dst, .. } => (Some(*dst), false),
            Instr::SetGlobal { .. } | Instr::SetIndex { .. } | Instr::SetUpval { .. } => {
                (None, false)
            }
        };
        if let Some(d) = dst {
            if is_num {
                self.set.insert(d.0);
            } else {
                self.set.remove(&d.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::{ConstId, VLocal};

    fn vl(i: u32) -> VLocal { VLocal(i) }

    #[test]
    fn loadconst_number_marks_numeric() {
        let consts = vec![Constant::Number(3.14)];
        let mut nl = NumericLocals::new();
        nl.update(
            &Instr::LoadConst { dst: vl(0), src: ConstId(0) },
            &consts,
        );
        assert!(nl.contains(0));
    }

    #[test]
    fn loadconst_string_doesnt_mark_numeric() {
        let consts = vec![Constant::String("hi".into())];
        let mut nl = NumericLocals::new();
        nl.update(
            &Instr::LoadConst { dst: vl(0), src: ConstId(0) },
            &consts,
        );
        assert!(!nl.contains(0));
    }

    #[test]
    fn add_of_numerics_is_numeric() {
        let consts = vec![Constant::Number(1.0)];
        let mut nl = NumericLocals::new();
        nl.update(&Instr::LoadConst { dst: vl(0), src: ConstId(0) }, &consts);
        nl.update(&Instr::LoadConst { dst: vl(1), src: ConstId(0) }, &consts);
        nl.update(
            &Instr::BinOp {
                dst: vl(2),
                op: BinOp::Add,
                lhs: Value::VLocal(vl(0)),
                rhs: Value::VLocal(vl(1)),
            },
            &consts,
        );
        assert!(nl.contains(2));
    }

    #[test]
    fn add_with_non_numeric_operand_isnt_numeric() {
        let consts = vec![Constant::Number(1.0)];
        let mut nl = NumericLocals::new();
        // vl(0) is from a Call — not numeric.
        nl.update(
            &Instr::Call { dst: Some(vl(0)), callee: Value::VLocal(vl(99)), args: vec![] },
            &consts,
        );
        nl.update(&Instr::LoadConst { dst: vl(1), src: ConstId(0) }, &consts);
        nl.update(
            &Instr::BinOp {
                dst: vl(2),
                op: BinOp::Add,
                lhs: Value::VLocal(vl(0)),
                rhs: Value::VLocal(vl(1)),
            },
            &consts,
        );
        assert!(!nl.contains(2));
    }

    #[test]
    fn move_propagates_numeric_status() {
        let consts = vec![Constant::Number(7.0)];
        let mut nl = NumericLocals::new();
        nl.update(&Instr::LoadConst { dst: vl(0), src: ConstId(0) }, &consts);
        nl.update(&Instr::Move { dst: vl(1), src: vl(0) }, &consts);
        assert!(nl.contains(1));
    }

    #[test]
    fn len_is_always_numeric() {
        let consts: Vec<Constant> = vec![];
        let mut nl = NumericLocals::new();
        // vl(0) from NewTable: not numeric.
        nl.update(&Instr::NewTable { dst: vl(0) }, &consts);
        nl.update(
            &Instr::UnOp { dst: vl(1), op: UnOp::Len, operand: Value::VLocal(vl(0)) },
            &consts,
        );
        assert!(nl.contains(1));
    }
}
```

- [ ] **Step 3: Refactor `add_padding.rs` to import the shared module**

Open `/Volumes/External Drive/luau-obf/crates/luau-passes/src/add_padding.rs`.

Replace the entire file body — keep the doc comment at the top, but the imports and types change. Replace from `use crate::Pass;` through the end of `update_numeric()` with:

```rust
use crate::analysis::numeric::NumericLocals;
use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct AddIdentityPadding;

const REWRITE_PERCENT: u8 = 30;
const K_MIN: u32 = 1;
const K_MAX: u32 = 65535;
```

Then replace the body of `impl Pass for AddIdentityPadding { fn run(...) { ... } }`:

```rust
    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut numeric = NumericLocals::new();
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_add = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Add, .. }
                    );
                    if !is_add {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, a, b) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    let operands_numeric = numeric.is_value_numeric(&a, &consts)
                        && numeric.is_value_numeric(&b, &consts);
                    if !want_rewrite || !operands_numeric {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
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
                    numeric.insert(kc.0);
                    numeric.insert(tmp_l.0);
                    numeric.insert(tmp_r.0);
                    numeric.insert(dst.0);
                    i += 4;
                }
            }
            f.n_locals = n_locals;
            f.consts = consts;
        }
    }
```

Delete the standalone `fn is_value_numeric` and `fn update_numeric` from `add_padding.rs` — they're now methods on `NumericLocals` in the shared module.

Keep the existing `#[cfg(test)] mod tests` block at the bottom of `add_padding.rs` unchanged. Those tests still exercise the public `AddIdentityPadding` behavior, not the internal helpers, so they continue to pass.

- [ ] **Step 4: Register the analysis module**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`, add:

```rust
pub mod analysis;
```

(Alongside the existing `pub mod identity;`, `pub mod expr_mangle;`, `pub mod add_padding;`.)

- [ ] **Step 5: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: 18 tests pass (12 pre-existing + 6 new in `analysis::numeric::tests`).

- [ ] **Step 6: Run the workspace**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every existing test still passes. The corpus (multi-seed) still passes.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-passes/src/analysis/mod.rs crates/luau-passes/src/analysis/numeric.rs crates/luau-passes/src/add_padding.rs crates/luau-passes/src/lib.rs
git commit -m "refactor(passes): extract NumericLocals analyzer to shared module"
```

---

## Task 2: Implement the MulOperandScatter pass

**Files:**
- Create: `crates/luau-passes/src/mul_padding.rs`
- Modify: `crates/luau-passes/src/lib.rs`

- [ ] **Step 1: Create the pass module**

Write `/Volumes/External Drive/luau-obf/crates/luau-passes/src/mul_padding.rs`:

```rust
//! Mul Operand-Scatter pass — rewrites a fraction of `Mul` instructions
//! as `(x + k) * y - k * y` with a seed-derived integer k. Both operands
//! must be provably numeric (via [`crate::analysis::numeric::NumericLocals`])
//! to avoid breaking programs that overload `__mul`.

use crate::analysis::numeric::NumericLocals;
use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct MulOperandScatter;

const REWRITE_PERCENT: u8 = 30;
const K_MIN: u32 = 1;
const K_MAX: u32 = 65535;

impl Pass for MulOperandScatter {
    fn name(&self) -> &'static str {
        "mul-operand-scatter"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut numeric = NumericLocals::new();
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_mul = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Mul, .. }
                    );
                    if !is_mul {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, x, y) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    let operands_numeric = numeric.is_value_numeric(&x, &consts)
                        && numeric.is_value_numeric(&y, &consts);
                    if !want_rewrite || !operands_numeric {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
                    // Rewrite path: dst = Mul(x, y)
                    //   -> kc = LoadConst k
                    //      tmp_a = Add(x, kc)         -- x + k
                    //      tmp_m = Mul(tmp_a, y)      -- (x + k) * y = xy + ky
                    //      tmp_k = Mul(kc, y)          -- k * y
                    //      dst   = Sub(tmp_m, tmp_k)  -- xy + ky - ky = xy
                    let k = rng.gen_range(K_MIN..=K_MAX) as f64;
                    let k_const_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k));
                    let kc = VLocal(n_locals);
                    let tmp_a = VLocal(n_locals + 1);
                    let tmp_m = VLocal(n_locals + 2);
                    let tmp_k = VLocal(n_locals + 3);
                    n_locals += 4;
                    block.instrs[i] = Instr::LoadConst { dst: kc, src: k_const_id };
                    block.instrs.insert(
                        i + 1,
                        Instr::BinOp {
                            dst: tmp_a,
                            op: BinOp::Add,
                            lhs: x,
                            rhs: Value::VLocal(kc),
                        },
                    );
                    block.instrs.insert(
                        i + 2,
                        Instr::BinOp {
                            dst: tmp_m,
                            op: BinOp::Mul,
                            lhs: Value::VLocal(tmp_a),
                            rhs: y,
                        },
                    );
                    block.instrs.insert(
                        i + 3,
                        Instr::BinOp {
                            dst: tmp_k,
                            op: BinOp::Mul,
                            lhs: Value::VLocal(kc),
                            rhs: y,
                        },
                    );
                    block.instrs.insert(
                        i + 4,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Sub,
                            lhs: Value::VLocal(tmp_m),
                            rhs: Value::VLocal(tmp_k),
                        },
                    );
                    numeric.insert(kc.0);
                    numeric.insert(tmp_a.0);
                    numeric.insert(tmp_m.0);
                    numeric.insert(tmp_k.0);
                    numeric.insert(dst.0);
                    i += 5;
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

    fn count_op(p: &MirProgram, target: BinOp) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if let Instr::BinOp { op, .. } = i {
                        if std::mem::discriminant(op) == std::mem::discriminant(&target) {
                            n += 1;
                        }
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
    fn rewrites_some_muls() {
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a * b, c * d, a * b * c, a * b * c * d, a * b * c * d * e)";
        let p = mir_of(src);
        let muls_before = count_op(&p, BinOp::Mul);
        assert!(muls_before >= 10, "expected ≥10 Muls, got {}", muls_before);

        let mut saw = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p_clone, &mut rng);
            if count_op(&p_clone, BinOp::Mul) > muls_before {
                saw = true;
                break;
            }
        }
        assert!(saw, "no seed 0..5 rewrote any Mul");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local a = 1 print(a * 2 * 3 * 4 * 5 * 6)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        MulOperandScatter.run(&mut p1, &mut r1);
        MulOperandScatter.run(&mut p2, &mut r2);
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
    fn programs_with_no_mul_are_unchanged() {
        let src = "local a, b = 10, 3 print(a + b, a - b)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let consts_before = p.functions[0].consts.len();
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        MulOperandScatter.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
        assert_eq!(p.functions[0].consts.len(), consts_before);
    }

    #[test]
    fn gate_skips_call_operands() {
        let src = "local function f() return 2 end \
                   local function g() return 3 end \
                   print(f() * g())";
        let p = mir_of(src);
        let muls_before = count_op(&p, BinOp::Mul);
        for s in 0..50u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p_clone, &mut rng);
            assert_eq!(count_op(&p_clone, BinOp::Mul), muls_before,
                "seed {} rewrote a Call-operand Mul", s);
        }
    }

    #[test]
    fn rewrite_growth_invariants() {
        let src = "local x = 1 print(x * 2 * 3 * 4 * 5 * 6 * 7 * 8)";
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let muls_before = count_op(&p, BinOp::Mul);
            let subs_before = count_op(&p, BinOp::Sub);
            let nums_before = count_number_consts(&p);
            let nlocals_before = p.functions[0].n_locals;
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p, &mut rng);
            let new_subs = count_op(&p, BinOp::Sub) - subs_before;
            // Each rewrite: removes 1 Mul, adds 2 Muls (tmp_m and tmp_k), adds 1 Sub.
            // Net Muls: +1. Net Subs: +1.
            assert_eq!(count_op(&p, BinOp::Mul) - muls_before, new_subs);
            // Each rewrite adds 1 numeric const (k) and 4 VLocals.
            assert_eq!(count_number_consts(&p) - nums_before, new_subs);
            assert_eq!(p.functions[0].n_locals - nlocals_before, (new_subs * 4) as u32);
        }
    }
}
```

- [ ] **Step 2: Register and add to default plan**

In `/Volumes/External Drive/luau-obf/crates/luau-passes/src/lib.rs`:

Add module declaration:

```rust
pub mod mul_padding;
```

Update `default_plan`:

```rust
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
}
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-passes --release`

Expected: all prior tests + 5 new tests pass.

- [ ] **Step 4: Run the workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all 23 corpus programs still pass under the 8-seed multi-seed harness.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes/src/mul_padding.rs crates/luau-passes/src/lib.rs
git commit -m "feat(passes): MIR Mul operand-scatter — rewrite Mul as (x+k)*y - k*y"
```

---

## Task 3: Acceptance tests in luau-obf

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add `mul_scatter_changes_output`**

Append inside `tests` mod:

```rust
    #[test]
    fn mul_scatter_changes_output() {
        let src = "local x = 1 \
                   x = x * 3 \
                   x = x * 5 \
                   x = x * 7 \
                   x = x * 11 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }
```

- [ ] **Step 2: Add `mul_scatter_preserves_semantics`**

```rust
    #[test]
    fn mul_scatter_preserves_semantics() {
        let src = "print(7 * 11 * 13)";  // 1001
        let r = obfuscate(src, Options { seed: Some([88u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(),
            "luau exited {:?}; stderr: {}",
            out.status, String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("1001"), "expected '1001' in stdout, got: {}", stdout);
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: Plan 17 acceptance — Mul scatter preserves semantics, varies per seed"
```

---

## Task 4: README — status bump

- [ ] **Step 1: Replace the Status paragraph**

```
**Status:** Plan 17 — Mul operand-scatter shipped. Combined with Plans 15
(Sub→Neg+Add) and 16 (Add operand-padding), every additive and
multiplicative MIR operator is rewritten probabilistically into a
seed-derived synthetic expansion. The numeric-VLocal dataflow analyzer
guards every rewrite, so programs that overload arithmetic metamethods
remain correct. Next plans extend the family (boolean mangling, constant
decomposition) before moving to control-flow obfuscation.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 17 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass under the 8-seed multi-seed harness.
- [ ] `mul_padding::tests::rewrites_some_muls` passes.
- [ ] `mul_padding::tests::deterministic_for_same_seed` passes (strong: full instr Debug-dump compare).
- [ ] `mul_padding::tests::programs_with_no_mul_are_unchanged` passes.
- [ ] `mul_padding::tests::gate_skips_call_operands` passes.
- [ ] `mul_padding::tests::rewrite_growth_invariants` passes.
- [ ] `mul_scatter_changes_output` passes.
- [ ] `mul_scatter_preserves_semantics` passes.
- [ ] Spot-check: compile a program like `print(2 * 3)` with two different seeds. Different seeds should produce a different instruction count in the obfuscated output (some seeds inject `LoadConst+Add+Mul+Mul+Sub`, others keep the single `Mul`).
