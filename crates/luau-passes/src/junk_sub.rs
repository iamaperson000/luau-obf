//! Junk-Sub-Chain pass — for each `BasicBlock`, with 20% probability,
//! inserts a 4-instruction dead-store chain at a seed-determined position:
//!
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Sub(δ1_vl, δ2_vl)
//!   tmp2  = Sub(tmp1, δ1_vl)
//!
//! Symmetric to JunkArithmetic but uses Sub. Adds dead-store pattern variety.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct JunkSubChain;

const INJECT_PERCENT: u8 = 20;
const DELTA_MIN: u32 = 2;
const DELTA_MAX: u32 = 65535;

impl Pass for JunkSubChain {
    fn name(&self) -> &'static str {
        "junk-sub-chain"
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
                let pos_pick = rng.gen::<u32>();
                let pos = (pos_pick as usize) % (block.instrs.len() + 1);

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
                        op: BinOp::Sub,
                        lhs: Value::VLocal(d1_vl),
                        rhs: Value::VLocal(d2_vl),
                    },
                    Instr::BinOp {
                        dst: tmp2,
                        op: BinOp::Sub,
                        lhs: Value::VLocal(tmp1),
                        rhs: Value::VLocal(d1_vl),
                    },
                ];
                for instr in junk.iter().rev().cloned() {
                    block.instrs.insert(pos, instr);
                }
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
        p.functions.iter().flat_map(|f| &f.blocks).map(|b| b.instrs.len()).sum()
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

    #[test]
    fn injects_some_sub_chains() {
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            for j = 1, 5 do s = s + j end
            for k = 1, 5 do s = s + k end
            print(s)
        ";
        let p = mir_of(src);
        let instrs_before = total_instrs(&p);
        let subs_before = count_subs(&p);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkSubChain.run(&mut p_clone, &mut rng);
            let instrs_after = total_instrs(&p_clone);
            let subs_after = count_subs(&p_clone);
            // Each injection: +4 instrs, +2 Subs.
            let instr_growth = instrs_after as i64 - instrs_before as i64;
            let sub_growth = subs_after as i64 - subs_before as i64;
            assert_eq!(
                instr_growth, 2 * sub_growth,
                "seed {}: instr growth ({}) != 2 * sub growth ({})",
                s, instr_growth, sub_growth
            );
            if sub_growth > 0 {
                saw_injection = true;
            }
        }
        assert!(saw_injection, "no seed 0..5 injected any sub chain");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local x = 1 if x then x = x + 1 end print(x)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        JunkSubChain.run(&mut p1, &mut r1);
        JunkSubChain.run(&mut p2, &mut r2);
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
    fn terminators_and_block_count_unchanged() {
        let src = "
            local s = 0
            for i = 1, 5 do s = s + i end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline = mir_of(src);
            let baseline_block_count: usize = baseline.functions.iter().map(|f| f.blocks.len()).sum();
            let baseline_terms: Vec<_> = baseline.functions.iter().flat_map(|f| &f.blocks).map(|b| format!("{:?}", b.terminator)).collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkSubChain.run(&mut p, &mut rng);
            let after_block_count: usize = p.functions.iter().map(|f| f.blocks.len()).sum();
            let after_terms: Vec<_> = p.functions.iter().flat_map(|f| &f.blocks).map(|b| format!("{:?}", b.terminator)).collect();
            assert_eq!(baseline_block_count, after_block_count, "seed {}: block count changed", seed_byte);
            assert_eq!(baseline_terms, after_terms, "seed {}: terminator changed", seed_byte);
        }
    }
}
