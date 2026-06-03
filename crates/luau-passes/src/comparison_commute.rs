//! Comparison-Commute pass — rewrites a fraction of comparison BinOps by
//! swapping operands (and flipping the operator for non-symmetric ops):
//!
//!   Lt(a, b) → Gt(b, a)        Eq(a, b) → Eq(b, a)
//!   Gt(a, b) → Lt(b, a)        Ne(a, b) → Ne(b, a)
//!   Le(a, b) → Ge(b, a)
//!   Ge(a, b) → Le(b, a)
//!
//! Semantically identical for ALL Luau values (including NaN and metatable
//! `__lt` / `__le` / `__eq`). No operand-type gate required.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{Instr, MirProgram};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ComparisonCommute;

const COMMUTE_PERCENT: u8 = 40;

fn flip_op(op: BinOp) -> Option<BinOp> {
    match op {
        BinOp::Lt => Some(BinOp::Gt),
        BinOp::Gt => Some(BinOp::Lt),
        BinOp::Le => Some(BinOp::Ge),
        BinOp::Ge => Some(BinOp::Le),
        BinOp::Eq => Some(BinOp::Eq),
        BinOp::Ne => Some(BinOp::Ne),
        _ => None,
    }
}

impl Pass for ComparisonCommute {
    fn name(&self) -> &'static str {
        "comparison-commute"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            for block in f.blocks.iter_mut() {
                for i in block.instrs.iter_mut() {
                    let (dst, op, lhs, rhs) = match i {
                        Instr::BinOp { dst, op, lhs, rhs } => (*dst, *op, *lhs, *rhs),
                        _ => continue,
                    };
                    let new_op = match flip_op(op) {
                        Some(o) => o,
                        None => continue,
                    };
                    let roll = rng.gen::<u8>() % 100;
                    if roll >= COMMUTE_PERCENT {
                        continue;
                    }
                    *i = Instr::BinOp {
                        dst,
                        op: new_op,
                        lhs: rhs,
                        rhs: lhs,
                    };
                }
            }
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

    fn count_op(p: &MirProgram, target: BinOp) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if let Instr::BinOp { op, .. } = i {
                        if *op == target {
                            n += 1;
                        }
                    }
                }
            }
        }
        n
    }

    #[test]
    fn commutes_some_lt_to_gt() {
        // A program with several `<` comparisons.
        let src = "
            local a, b, c, d = 1, 2, 3, 4
            print(a < b, b < c, c < d, a < c, b < d)
        ";
        let p = mir_of(src);
        let lt_before = count_op(&p, BinOp::Lt);
        let gt_before = count_op(&p, BinOp::Gt);
        assert!(lt_before >= 5, "expected ≥5 Lt, got {}", lt_before);

        let mut saw_commute = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ComparisonCommute.run(&mut p_clone, &mut rng);
            let lt_after = count_op(&p_clone, BinOp::Lt);
            let gt_after = count_op(&p_clone, BinOp::Gt);
            // Every Lt commuted becomes Gt: Lt count drops by N, Gt count rises by N.
            let commuted = lt_before as i64 - lt_after as i64;
            let gained = gt_after as i64 - gt_before as i64;
            assert_eq!(
                commuted, gained,
                "seed {}: Lt-loss ({}) != Gt-gain ({})",
                s, commuted, gained
            );
            if commuted > 0 {
                saw_commute = true;
            }
        }
        assert!(saw_commute, "no seed 0..5 commuted any Lt");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local a, b = 1, 2
            print(a < b, a == b, a >= b)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ComparisonCommute.run(&mut p1, &mut r1);
        ComparisonCommute.run(&mut p2, &mut r2);
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
    fn programs_with_no_comparisons_are_unchanged() {
        let src = "local x = 1 + 2 * 3 print(x)";
        let mut p = mir_of(src);
        let nlocals_before = p.functions[0].n_locals;
        let dump_before = format!("{:?}", p.functions[0].blocks);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ComparisonCommute.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, nlocals_before);
        let dump_after = format!("{:?}", p.functions[0].blocks);
        assert_eq!(dump_before, dump_after, "non-comparison program changed");
    }

    #[test]
    fn instruction_count_is_unchanged() {
        // The pass mutates in place; never inserts or removes instructions.
        let src = "
            local a, b = 1, 2
            if a < b then print('lt') end
            if a == b then print('eq') end
            if a <= b then print('le') end
            print(a > b, a >= b, a ~= b)
        ";
        let baseline = mir_of(src);
        let baseline_count: usize = baseline
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .map(|b| b.instrs.len())
            .sum();
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ComparisonCommute.run(&mut p, &mut rng);
            let after_count: usize = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| b.instrs.len())
                .sum();
            assert_eq!(after_count, baseline_count, "seed {}: instr count changed", s);
        }
    }
}
