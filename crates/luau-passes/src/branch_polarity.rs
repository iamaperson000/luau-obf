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
