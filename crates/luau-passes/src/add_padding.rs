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
