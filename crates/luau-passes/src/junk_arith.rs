//! Junk-Arithmetic pass — for each `BasicBlock`, with 30% probability,
//! inserts a dead-store chain at a seed-determined position.
//!
//! **Mode 0** (original, ~50% of injections):
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Add(δ1_vl, δ2_vl)
//!   tmp2  = Mul(tmp1, δ1_vl)
//!
//! **Mode 1** (live-operand, ~50% when candidates available):
//!   d2_vl = LoadConst(δ2)
//!   tmp1  = Add(lv, d2_vl)       -- lv is a provably-Number VLocal from earlier in block
//!   tmp2  = Mul(tmp1, d2_vl)
//!
//! All result VLocals remain dead. Mode 1 defeats an adversary's constant
//! folder because `lv` is not statically known.
//!
//! **RNG schedule** (mode-invariant — 6 draws regardless of mode):
//!   roll(u8) → pos(u32) → mode(u8) → δ1(u32) → δ2(u32) → live_idx(u32)

use crate::Pass;
use crate::analysis;
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
                // Draw 1: inject roll.
                let roll = rng.gen::<u8>() % 100;
                if roll >= INJECT_PERCENT {
                    // Still drain the remaining 5 RNG draws to stay schedule-invariant.
                    let _pos_pick = rng.gen::<u32>();
                    let _mode = rng.gen::<u8>();
                    let _d1 = rng.gen_range(DELTA_MIN..=DELTA_MAX);
                    let _d2 = rng.gen_range(DELTA_MIN..=DELTA_MAX);
                    let _live_pick = rng.gen::<u32>();
                    continue;
                }

                // Draw 2: position.
                let pos_pick = rng.gen::<u32>();
                let pos = (pos_pick as usize) % (block.instrs.len() + 1);

                // Draw 3: mode.
                let mode_pick: u8 = rng.gen::<u8>() % 2;

                // Draw 4 & 5: delta values (always drawn).
                let delta1 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                let delta2 = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;

                // Draw 6: live operand index (always drawn).
                let live_pick: u32 = rng.gen::<u32>();

                // Compute candidate set before pos.
                let candidates = analysis::number_vlocals_before(block, pos, &consts);
                let use_live = mode_pick == 1 && !candidates.is_empty();

                if use_live {
                    // Mode 1: 3-instruction shape with a live VLocal as lhs.
                    let mut cand_vec: Vec<u32> = candidates.into_iter().collect();
                    cand_vec.sort();
                    let lv = VLocal(cand_vec[(live_pick as usize) % cand_vec.len()]);

                    let c2_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(delta2));

                    let d2_vl = VLocal(n_locals);
                    let tmp1 = VLocal(n_locals + 1);
                    let tmp2 = VLocal(n_locals + 2);
                    n_locals += 3;

                    let junk: [Instr; 3] = [
                        Instr::LoadConst { dst: d2_vl, src: c2_id },
                        Instr::BinOp {
                            dst: tmp1,
                            op: BinOp::Add,
                            lhs: Value::VLocal(lv),
                            rhs: Value::VLocal(d2_vl),
                        },
                        Instr::BinOp {
                            dst: tmp2,
                            op: BinOp::Mul,
                            lhs: Value::VLocal(tmp1),
                            rhs: Value::VLocal(d2_vl),
                        },
                    ];
                    for instr in junk.iter().rev().cloned() {
                        block.instrs.insert(pos, instr);
                    }
                    let _ = tmp2;
                    let _ = delta1; // intentionally drawn but unused in mode 1
                } else {
                    // Mode 0: original 4-instruction shape.
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
                    for instr in junk.iter().rev().cloned() {
                        block.instrs.insert(pos, instr);
                    }
                    let _ = tmp2;
                    let _ = live_pick; // intentionally drawn but unused in mode 0
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

    /// Count Mul-type BinOp instructions. Each injection emits exactly one Mul.
    fn count_muls(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::BinOp { op: BinOp::Mul, .. }) {
                        n += 1;
                    }
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

        // Baseline Mul count (from user code — a for loop uses no Muls here, so 0).
        let muls_before = count_muls(&p);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkArithmetic.run(&mut p_clone, &mut rng);
            let muls_after = count_muls(&p_clone);
            // Each injection adds exactly 1 Mul (both mode 0 and mode 1).
            // Each injection adds either 3 (mode 1) or 4 (mode 0) instructions.
            let instr_growth = total_instrs(&p_clone) - total_instrs(&mir_of(src));
            let injections = (muls_after - muls_before) as usize;
            // Instruction growth should be 3*n to 4*n (mix of modes).
            if injections > 0 {
                assert!(
                    instr_growth >= 3 * injections && instr_growth <= 4 * injections,
                    "seed {}: instr growth {} not in [3n={}, 4n={}]",
                    s, instr_growth, 3 * injections, 4 * injections
                );
                saw_injection = true;
            }
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

    #[test]
    fn mixes_live_and_const_operands() {
        // A program with several arithmetic operations, giving the pass many
        // provably-Number VLocals to pick from.
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i * 2
            end
            print(s)
        ";
        let baseline = mir_of(src);
        // n_locals before the pass runs — VLocals with index < this were
        // NOT allocated by the pass itself.
        let pre_pass_n_locals = baseline.functions[0].n_locals;

        let mut found_live_operand = false;
        'outer: for seed_byte in 0..200u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p, &mut rng);
            for f in &p.functions {
                for b in &f.blocks {
                    for instr in &b.instrs {
                        if let Instr::BinOp { op: BinOp::Add, lhs: Value::VLocal(vl), .. } = instr {
                            // The pass's mode-1 Add has a live VLocal as lhs.
                            // If the vlocal index is below pre_pass_n_locals,
                            // it cannot have been allocated by this pass run.
                            if vl.0 < pre_pass_n_locals {
                                found_live_operand = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
        assert!(
            found_live_operand,
            "no seed in 0..200 produced a BinOp::Add whose lhs is a pre-pass VLocal"
        );
    }

    #[test]
    fn rng_schedule_mode_invariant() {
        // Two runs with the same seed must produce identical instruction layouts.
        // This is a stronger restatement of deterministic_for_same_seed, using
        // the mul-heavy program to force many candidate blocks.
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i * 2
            end
            print(s)
        ";
        for seed_byte in [0u8, 7, 42, 99, 137, 255] {
            let mut p1 = mir_of(src);
            let mut p2 = mir_of(src);
            let mut r1 = ChaCha20Rng::from_seed([seed_byte; 32]);
            let mut r2 = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkArithmetic.run(&mut p1, &mut r1);
            JunkArithmetic.run(&mut p2, &mut r2);
            let dump = |p: &MirProgram| -> String {
                let mut s = String::new();
                for f in &p.functions {
                    for b in &f.blocks {
                        for i in &b.instrs { s.push_str(&format!("{:?}\n", i)); }
                        s.push_str(&format!("term: {:?}\n", b.terminator));
                    }
                }
                s
            };
            assert_eq!(dump(&p1), dump(&p2), "seed {} produced different layouts", seed_byte);
        }
    }

}
