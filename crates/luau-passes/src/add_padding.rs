//! Add Identity-Padding pass — rewrites a fraction of `Add` instructions
//! as `(a + k) + (b - k)` with a seed-derived integer k.
//!
//! Both operands must be **provably numeric** at the rewrite site (otherwise
//! the rewrite invokes the `__add` metamethod with a synthetic numeric value
//! on one side, which crashes programs whose metatables don't accept mixed
//! operands — e.g. `Vec2.__add(a, b)` doing `a.x + b.x` for a `Vec2 + number`
//! pair). The provably-numeric check is an intra-block forward dataflow:
//! a VLocal is numeric if its most-recent definition in the current block
//! is a `LoadConst Number`, a numeric `BinOp` (Add/Sub/Mul/Div/Mod/Pow) on
//! numeric operands, an arithmetic `UnOp` on a numeric operand, `UnOp::Len`
//! (whose result is always a number), or a `Move` from a numeric VLocal.
//! Anything else (Call return, GetIndex, GetGlobal, GetUpval, NewTable,
//! parameters, prior-block values) is conservatively NOT numeric, so the
//! rewrite skips that Add.

use crate::Pass;
use luau_hir::{BinOp, UnOp};
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use std::collections::HashSet;

const REWRITE_PERCENT: u8 = 30;
const K_MIN: u32 = 1;
const K_MAX: u32 = 65535;

fn is_value_numeric(v: &Value, numeric: &HashSet<u32>, consts: &[Constant]) -> bool {
    match v {
        Value::Const(c) => matches!(consts.get(c.0 as usize), Some(Constant::Number(_))),
        Value::VLocal(v) => numeric.contains(&v.0),
    }
}

/// Walk one MIR instruction's effect on the "provably numeric" set: mark
/// `dst` as numeric / not-numeric based on op semantics, or no-op for
/// instructions without a writable VLocal.
fn update_numeric(instr: &Instr, numeric: &mut HashSet<u32>, consts: &[Constant]) {
    let (dst, is_num) = match instr {
        Instr::LoadConst { dst, src } => {
            let n = matches!(consts.get(src.0 as usize), Some(Constant::Number(_)));
            (Some(*dst), n)
        }
        Instr::BinOp { dst, op, lhs, rhs } => {
            let numeric_op = matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
            );
            let n = numeric_op
                && is_value_numeric(lhs, numeric, consts)
                && is_value_numeric(rhs, numeric, consts);
            (Some(*dst), n)
        }
        Instr::UnOp { dst, op, operand } => {
            let n = match op {
                UnOp::Len => true,
                UnOp::Neg => is_value_numeric(operand, numeric, consts),
                _ => false,
            };
            (Some(*dst), n)
        }
        Instr::Move { dst, src } => {
            let n = numeric.contains(&src.0);
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
            numeric.insert(d.0);
        } else {
            numeric.remove(&d.0);
        }
    }
}

pub struct AddIdentityPadding;

impl Pass for AddIdentityPadding {
    fn name(&self) -> &'static str {
        "add-identity-padding"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut numeric: HashSet<u32> = HashSet::new();
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_add = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Add, .. }
                    );
                    if !is_add {
                        update_numeric(&block.instrs[i], &mut numeric, &consts);
                        i += 1;
                        continue;
                    }
                    // Add candidate — roll FIRST (deterministic RNG advancement).
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, a, b) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    let operands_numeric = is_value_numeric(&a, &numeric, &consts)
                        && is_value_numeric(&b, &numeric, &consts);
                    if !want_rewrite || !operands_numeric {
                        update_numeric(&block.instrs[i], &mut numeric, &consts);
                        i += 1;
                        continue;
                    }
                    // Rewrite path.
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
                    // All four new dsts (kc, tmp_l, tmp_r, dst) are numeric.
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
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a + b, c + d, a + b + c, a + b + c + d, a + b + c + d + e)";
        let p = mir_of(src);
        let adds_before = count_adds(&p);
        let subs_before = count_subs(&p);
        let nums_before = count_number_consts(&p);
        assert!(adds_before >= 10, "expected at least 10 Adds, got {}", adds_before);

        let mut saw_rewrite = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p_clone, &mut rng);
            let adds_after = count_adds(&p_clone);
            let subs_after = count_subs(&p_clone);
            let nums_after = count_number_consts(&p_clone);
            let rewrites_via_adds = adds_after as i64 - adds_before as i64;
            let rewrites_via_subs = subs_after as i64 - subs_before as i64;
            let rewrites_via_consts = nums_after as i64 - nums_before as i64;
            assert_eq!(rewrites_via_adds, rewrites_via_subs);
            assert_eq!(rewrites_via_subs, rewrites_via_consts);
            if rewrites_via_adds > 0 {
                saw_rewrite = true;
            }
        }
        assert!(saw_rewrite, "no seed in 0..5 triggered any rewrite");
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
        // Strong determinism: full instr sequence Debug-equal.
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
    fn programs_with_no_add_are_unchanged() {
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
            assert_eq!(count_adds(&p) - adds_before, rewrite_count);
            assert_eq!(count_number_consts(&p) - nums_before, rewrite_count);
            assert_eq!(
                p.functions[0].n_locals - n_locals_before,
                (rewrite_count * 3) as u32
            );
        }
    }

    #[test]
    fn gate_skips_call_result_operands() {
        // `f() + g()` — both operands are Call results, NOT provably numeric.
        // The gate must skip every such Add across all seeds.
        let src = "local function f() return 1 end \
                   local function g() return 2 end \
                   print(f() + g())";
        let p = mir_of(src);
        let adds_before = count_adds(&p);
        let subs_before = count_subs(&p);
        for s in 0..50u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p_clone, &mut rng);
            assert_eq!(
                count_adds(&p_clone),
                adds_before,
                "seed {} rewrote a Call-operand Add",
                s
            );
            assert_eq!(count_subs(&p_clone), subs_before);
        }
    }

    #[test]
    fn gate_skips_get_index_operands() {
        // `t.a + t.b` — both operands are GetIndex results. Not provably
        // numeric (the table fields could hold metatables).
        let src = "local t = { a = 1, b = 2 } print(t.a + t.b)";
        let p = mir_of(src);
        let adds_before = count_adds(&p);
        for s in 0..50u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p_clone, &mut rng);
            assert_eq!(
                count_adds(&p_clone),
                adds_before,
                "seed {} rewrote a GetIndex-operand Add",
                s
            );
        }
    }

    #[test]
    fn gate_allows_loadconst_operands() {
        // `local x = 1; x + 2 + 3` — all operands are LoadConst Number or
        // numeric BinOp results, so the gate allows rewriting.
        let src = "local x = 1 print(x + 2 + 3 + 4 + 5 + 6 + 7 + 8)";
        let mut saw_rewrite = false;
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let adds_before = count_adds(&p);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            AddIdentityPadding.run(&mut p, &mut rng);
            if count_adds(&p) > adds_before {
                saw_rewrite = true;
                break;
            }
        }
        assert!(saw_rewrite, "gate too aggressive: no seed rewrote a numeric Add");
    }
}
