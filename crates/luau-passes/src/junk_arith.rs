//! Junk-Arithmetic pass — for each `BasicBlock`, with 30% probability,
//! inserts a 4-instruction dead-store chain at a seed-determined position:
//!
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Add(δ1_vl, δ2_vl)
//!   tmp2  = Mul(tmp1, δ1_vl)
//!
//! The four fresh VLocals are dead (never read). Plans 16/17 may further
//! scatter the inserted Add and Mul.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct JunkArithmetic;

const INJECT_PERCENT: u8 = 30;
const DELTA_MIN: u32 = 2;
const DELTA_MAX: u32 = 65535;

impl Pass for JunkArithmetic {
    fn name(&self) -> &'static str {
        "junk-arithmetic"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let roll = rng.gen::<u8>() % 100;
                if roll >= INJECT_PERCENT {
                    continue;
                }
                // Pick insertion position uniformly in [0, instrs.len()].
                let pos_pick = rng.gen::<u32>();
                let pos = (pos_pick as usize) % (block.instrs.len() + 1);

                // Pick two δ values.
                let delta1 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                let delta2 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;

                let c1_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta1));
                let c2_id = ConstId(consts.len() as u32);
                consts.push(Constant::Number(delta2));

                let d1_vl = VLocal(n_locals);
                let d2_vl = VLocal(n_locals + 1);
                let tmp1 = VLocal(n_locals + 2);
                let tmp2 = VLocal(n_locals + 3);
                n_locals += 4;

                let junk: [Instr; 4] = [
                    Instr::LoadConst { dst: d1_vl, src: c1_id },
                    Instr::LoadConst { dst: d2_vl, src: c2_id },
                    Instr::BinOp {
                        dst: tmp1,
                        op: BinOp::Add,
                        lhs: Value::VLocal(d1_vl),
                        rhs: Value::VLocal(d2_vl),
                    },
                    Instr::BinOp {
                        dst: tmp2,
                        op: BinOp::Mul,
                        lhs: Value::VLocal(tmp1),
                        rhs: Value::VLocal(d1_vl),
                    },
                ];
                // Insert in reverse so each `insert` lands at the same index.
                for instr in junk.iter().rev().cloned() {
                    block.instrs.insert(pos, instr);
                }
                // Suppress unused warning for tmp2 — it's intentionally dead.
                let _ = tmp2;
            }
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

    fn total_instrs(p: &MirProgram) -> usize {
        p.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .map(|b| b.instrs.len())
            .sum()
    }

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
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
    fn injects_some_junk() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let blocks = total_blocks(&p);
        assert!(blocks >= 3, "expected ≥3 blocks, got {}", blocks);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let instrs_before = total_instrs(&p_clone);
            let consts_before = count_number_consts(&p_clone);
            let nlocals_before = p_clone.functions[0].n_locals;
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkArithmetic.run(&mut p_clone, &mut rng);
            let instrs_growth = total_instrs(&p_clone) - instrs_before;
            let consts_growth = count_number_consts(&p_clone) - consts_before;
            let nlocals_growth = (p_clone.functions[0].n_locals - nlocals_before) as usize;
            // Each injection: +4 instrs, +2 consts, +4 vlocals (in any function).
            // Per-function bookkeeping is tracked by us, but the totals across all
            // functions should preserve the ratio.
            // Compute total nlocals across all functions:
            let total_nlocals_growth: usize = p_clone
                .functions
                .iter()
                .zip(mir_of(src).functions.iter())
                .map(|(after, before)| (after.n_locals - before.n_locals) as usize)
                .sum();
            let total_consts_growth: usize = p_clone
                .functions
                .iter()
                .zip(mir_of(src).functions.iter())
                .map(|(after, before)| {
                    let a = after.consts.iter().filter(|c| matches!(c, Constant::Number(_))).count();
                    let b = before.consts.iter().filter(|c| matches!(c, Constant::Number(_))).count();
                    a - b
                })
                .sum();
            assert_eq!(
                instrs_growth, 4 * (total_consts_growth / 2),
                "seed {}: instr growth ({}) not 4x injection count ({})",
                s, instrs_growth, total_consts_growth / 2
            );
            assert_eq!(
                total_nlocals_growth, 2 * total_consts_growth,
                "seed {}: nlocals growth ({}) not 2x const growth ({})",
                s, total_nlocals_growth, total_consts_growth
            );
            if instrs_growth > 0 {
                saw_injection = true;
            }
            let _ = consts_growth; // already covered by total_consts_growth
            let _ = nlocals_growth;
        }
        assert!(saw_injection, "no seed 0..5 injected any junk");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local x = 1
            if x then x = x + 1 else x = x - 1 end
            print(x)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        JunkArithmetic.run(&mut p1, &mut r1);
        JunkArithmetic.run(&mut p2, &mut r2);
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
    fn terminators_are_unchanged() {
        // The pass only inserts instructions; never modifies terminators.
        let src = "
            local s = 0
            for i = 1, 3 do
                s = s + i
                if i == 2 then break end
            end
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
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p, &mut rng);
            let after_terms: Vec<_> = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| format!("{:?}", b.terminator))
                .collect();
            assert_eq!(
                baseline_terms, after_terms,
                "seed {}: terminator changed", seed_byte
            );
        }
    }

    #[test]
    fn block_count_is_unchanged() {
        // The pass never adds or removes blocks.
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline_blocks = total_blocks(&mir_of(src));
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p, &mut rng);
            assert_eq!(total_blocks(&p), baseline_blocks, "seed {}: block count changed", seed_byte);
        }
    }
}
