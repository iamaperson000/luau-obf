//! Expression Mangle pass — rewrites a fraction of `Sub` instructions as
//! `Add(a, Neg(b))`. The result is semantically identical for numeric
//! operands; the bytecode no longer consistently maps `Sub` to the
//! source-level `-` operator.

use crate::Pass;
use luau_hir::{BinOp, UnOp};
use luau_mir::{Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ExpressionMangle;

impl Pass for ExpressionMangle {
    fn name(&self) -> &'static str {
        "expression-mangle"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            for block in f.blocks.iter_mut() {
                let mut i = 0;
                while i < block.instrs.len() {
                    let should_rewrite = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Sub, .. }
                    ) && rng.gen::<u8>() & 1 == 1;
                    if !should_rewrite {
                        i += 1;
                        continue;
                    }
                    let (dst, lhs, rhs) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let tmp = VLocal(n_locals);
                    n_locals += 1;
                    block.instrs[i] = Instr::UnOp {
                        dst: tmp,
                        op: UnOp::Neg,
                        operand: rhs,
                    };
                    block.instrs.insert(
                        i + 1,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Add,
                            lhs,
                            rhs: Value::VLocal(tmp),
                        },
                    );
                    i += 2;
                }
            }
            f.n_locals = n_locals;
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

    fn count_negs(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::UnOp { op: UnOp::Neg, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn rewrites_some_subs_to_add_neg() {
        // A program with several Sub instructions.
        let src = "local a, b, c, d = 10, 20, 30, 40 \
                   print(a - b, c - d, (a - b) - (c - d), a - (b - c))";
        let mut p = mir_of(src);
        let subs_before = count_subs(&p);
        let negs_before = count_negs(&p);
        assert!(subs_before >= 4, "expected at least 4 Subs, got {}", subs_before);
        let mut rng = ChaCha20Rng::from_seed([7u8; 32]);
        ExpressionMangle.run(&mut p, &mut rng);
        let subs_after = count_subs(&p);
        let negs_after = count_negs(&p);
        // Some Subs should be rewritten to (Neg + Add), so:
        // - subs_after < subs_before (fewer Subs remain)
        // - negs_after > negs_before (more Negs were inserted)
        // - the COUNT of Negs added equals the COUNT of Subs removed.
        assert!(subs_after < subs_before,
            "expected some Subs to be rewritten (before={} after={})", subs_before, subs_after);
        assert_eq!(negs_after - negs_before, subs_before - subs_after);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local a = 1 print(a - 2 - 3 - 4)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ExpressionMangle.run(&mut p1, &mut r1);
        ExpressionMangle.run(&mut p2, &mut r2);
        assert_eq!(count_subs(&p1), count_subs(&p2));
        assert_eq!(count_negs(&p1), count_negs(&p2));
    }

    #[test]
    fn programs_with_no_sub_are_unchanged() {
        let src = "local a = 1 + 2 print(a)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ExpressionMangle.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
    }

    #[test]
    fn rewrite_increases_n_locals() {
        let src = "local a, b = 10, 5 print(a - b)";
        let p = mir_of(src);
        let before = p.functions[0].n_locals;
        // Use a seed that we know rewrites the Sub (drawing 1 from rng.gen::<u8>() & 1).
        // ChaCha20Rng with seed [255u8; 32] should reliably trigger SOME rewrite.
        // We'll just check that AT LEAST ONE seed in a small set produces an
        // increase.
        let mut found_rewrite = false;
        for s in 0..10u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ExpressionMangle.run(&mut p_clone, &mut rng);
            if p_clone.functions[0].n_locals > before {
                found_rewrite = true;
                break;
            }
        }
        assert!(found_rewrite, "no seed in 0..10 triggered a Sub rewrite");
    }
}
