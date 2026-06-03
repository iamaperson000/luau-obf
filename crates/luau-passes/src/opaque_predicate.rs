//! Opaque-True Predicate pass — wraps unconditional `Goto` terminators
//! with a `Branch { cond: opaque_true, then: target, else: junk }` whose
//! condition is `δ * δ > 0` for a seed-derived integer δ ∈ [1, 65535].
//! Always true at runtime; static analyzers must constant-propagate
//! through Mul + Gt to prove it.
//!
//! Per function: one junk block holding ~3 dead `LoadConst Number`
//! instructions and a `Return(None)` terminator. Only created when at
//! least one `Goto` in the function will be wrapped.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{
    BasicBlock, BlockId, ConstId, Constant, Instr, MirProgram, Terminator, VLocal, Value,
};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct OpaqueTruePredicate;

const FN_ENABLE_PERCENT: u8 = 50;
const GOTO_WRAP_PERCENT: u8 = 40;
const DELTA_MIN: u32 = 1;
const DELTA_MAX: u32 = 65535;
const JUNK_LOADCONST_COUNT: usize = 3;
const JUNK_DELTA_MIN: u32 = 2;
const JUNK_DELTA_MAX: u32 = 255;

impl Pass for OpaqueTruePredicate {
    fn name(&self) -> &'static str {
        "opaque-true-predicate"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            // One u8 per function to decide enablement (always draw for
            // deterministic RNG advancement).
            let fn_roll = rng.gen::<u8>() % 100;
            let fn_enable = fn_roll < FN_ENABLE_PERCENT;
            if !fn_enable {
                continue;
            }

            // Phase 1: decide which Gotos to wrap (always draw a u8 per Goto).
            let mut wrap_decisions: Vec<bool> = Vec::with_capacity(f.blocks.len());
            for block in &f.blocks {
                let is_goto = matches!(block.terminator, Terminator::Goto(_));
                if !is_goto {
                    wrap_decisions.push(false);
                    continue;
                }
                let roll = rng.gen::<u8>() % 100;
                wrap_decisions.push(roll < GOTO_WRAP_PERCENT);
            }

            if !wrap_decisions.iter().any(|&w| w) {
                continue;
            }

            // Phase 2: create the junk block.
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            let next_block_id_value = f
                .blocks
                .iter()
                .map(|b| b.id.0)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
            let junk_block_id = BlockId(next_block_id_value);
            let mut junk_instrs: Vec<Instr> = Vec::with_capacity(JUNK_LOADCONST_COUNT);
            for _ in 0..JUNK_LOADCONST_COUNT {
                let junk_delta = rng.gen_range(JUNK_DELTA_MIN..=JUNK_DELTA_MAX) as f64;
                let cid = ConstId(consts.len() as u32);
                consts.push(Constant::Number(junk_delta));
                let dst = VLocal(n_locals);
                n_locals += 1;
                junk_instrs.push(Instr::LoadConst { dst, src: cid });
            }
            let junk_block = BasicBlock {
                id: junk_block_id,
                instrs: junk_instrs,
                terminator: Terminator::Return(None),
            };

            // Phase 3: for each Goto selected, append opaque computation and
            // rewrite the terminator.
            for (idx, &wrap) in wrap_decisions.iter().enumerate() {
                if !wrap {
                    continue;
                }
                let target = match &f.blocks[idx].terminator {
                    Terminator::Goto(t) => *t,
                    _ => continue,
                };
                let delta = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                let delta_const_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta));
                let zero_const_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(0.0));
                let delta_vl = VLocal(n_locals);
                let mul_vl = VLocal(n_locals + 1);
                let zero_vl = VLocal(n_locals + 2);
                let cond_vl = VLocal(n_locals + 3);
                n_locals += 4;
                let block = &mut f.blocks[idx];
                block.instrs.push(Instr::LoadConst { dst: delta_vl, src: delta_const_id });
                block.instrs.push(Instr::BinOp {
                    dst: mul_vl,
                    op: BinOp::Mul,
                    lhs: Value::VLocal(delta_vl),
                    rhs: Value::VLocal(delta_vl),
                });
                block.instrs.push(Instr::LoadConst { dst: zero_vl, src: zero_const_id });
                block.instrs.push(Instr::BinOp {
                    dst: cond_vl,
                    op: BinOp::Gt,
                    lhs: Value::VLocal(mul_vl),
                    rhs: Value::VLocal(zero_vl),
                });
                block.terminator = Terminator::Branch {
                    cond: Value::VLocal(cond_vl),
                    then_block: target,
                    else_block: junk_block_id,
                };
            }

            f.blocks.push(junk_block);
            f.n_locals = n_locals;
            f.consts = consts;
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

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
    }

    #[test]
    fn wraps_some_gotos() {
        // A program with multiple blocks that reach via Goto. A while loop
        // or numeric for produces these.
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let gotos_before = count_gotos(&p);
        let branches_before = count_branches(&p);
        let blocks_before = total_blocks(&p);
        assert!(gotos_before >= 1, "expected ≥1 Goto, got {}", gotos_before);

        let mut saw_wrap = false;
        for s in 0..10u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            OpaqueTruePredicate.run(&mut p_clone, &mut rng);
            // Each wrap converts a Goto into a Branch and adds one block (the
            // junk block, shared per function).
            let gotos_after = count_gotos(&p_clone);
            let branches_after = count_branches(&p_clone);
            let blocks_after = total_blocks(&p_clone);
            if branches_after > branches_before {
                assert!(blocks_after > blocks_before, "wrap happened but no junk block added");
                assert!(gotos_after < gotos_before, "wrap happened but Goto count didn't drop");
                saw_wrap = true;
            }
        }
        assert!(saw_wrap, "no seed 0..10 wrapped any Goto");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local x = 0
            while x < 5 do
                x = x + 1
            end
            print(x)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        OpaqueTruePredicate.run(&mut p1, &mut r1);
        OpaqueTruePredicate.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                    s.push_str(&format!("term: {:?}\n", b.terminator));
                }
                for c in &f.consts {
                    s.push_str(&format!("c: {:?}\n", c));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn programs_with_no_gotos_are_unchanged() {
        // A program with only a top-level print — produces a single block
        // with Return(None), no Goto.
        let src = "print('hi')";
        let mut p = mir_of(src);
        let blocks_before = total_blocks(&p);
        let nlocals_before = p.functions[0].n_locals;
        for s in 0..20u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            OpaqueTruePredicate.run(&mut p_clone, &mut rng);
            assert_eq!(total_blocks(&p_clone), blocks_before, "seed {}: blocks changed", s);
            assert_eq!(
                p_clone.functions[0].n_locals,
                nlocals_before,
                "seed {}: nlocals changed",
                s
            );
        }
        let _ = p; // silence unused warning
    }

    #[test]
    fn each_wrapped_function_has_at_most_one_junk_block() {
        // After a run with several Gotos, all of them route to the SAME junk
        // block (the one created per function).
        let src = "
            local s = 0
            for i = 1, 10 do
                s = s + i
            end
            for j = 1, 10 do
                s = s + j
            end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            OpaqueTruePredicate.run(&mut p, &mut rng);
            for f in &p.functions {
                // Collect every else_block target where the else_block looks
                // like a junk block (i.e., a block whose terminator is
                // Return(None) and whose instrs are all LoadConst).
                let mut junk_targets: Vec<BlockId> = Vec::new();
                for b in &f.blocks {
                    if let Terminator::Branch { else_block, .. } = &b.terminator {
                        // Look up the else_block.
                        let referenced = f.blocks.iter().find(|bb| bb.id == *else_block);
                        if let Some(jb) = referenced {
                            let is_junk = matches!(jb.terminator, Terminator::Return(None))
                                && jb.instrs.iter().all(|i| matches!(i, Instr::LoadConst { .. }));
                            if is_junk && !junk_targets.contains(else_block) {
                                junk_targets.push(*else_block);
                            }
                        }
                    }
                }
                // At most one junk block per function.
                assert!(
                    junk_targets.len() <= 1,
                    "seed {}: function {:?} has {} junk targets — should be ≤1",
                    seed_byte,
                    f.id,
                    junk_targets.len()
                );
            }
        }
    }
}
