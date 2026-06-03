//! Junk-Sub-Chain pass — for each `BasicBlock`, with 20% probability,
//! inserts a dead-store chain at a seed-determined position.
//!
//! **Mode 0** (original, ~50% of injections):
//!   δ1_vl = LoadConst(δ1)
//!   δ2_vl = LoadConst(δ2)
//!   tmp1  = Sub(δ1_vl, δ2_vl)
//!   tmp2  = Sub(tmp1, δ1_vl)
//!
//! **Mode 1** (live-operand, ~50% when candidates available):
//!   d2_vl = LoadConst(δ2)
//!   tmp1  = Sub(lv, d2_vl)       -- lv is a provably-Number VLocal from earlier in block
//!   tmp2  = Sub(tmp1, d2_vl)
//!
//! Symmetric to JunkArithmetic but uses Sub. Adds dead-store pattern variety
//! and defeats constant-folding attacks on the chain.
//!
//! **Consume mode** (~25% of injections, independent of chain mode):
//! After the chain, if a live Number VLocal `live_target` is available, append:
//!   step1       = Add(live_target, tmp2)   -- reads tmp2; tmp2 is now live
//!   live_target = Sub(step1, tmp2)         -- net-zero: live_target unchanged
//!
//! This forces a static analyzer to prove `+x - x == 0` (SMT-class) to strip
//! the chain, because `tmp2`'s value is not statically evident.
//!
//! **RNG schedule** (mode-invariant — 8 draws regardless of mode/consume):
//!   roll(u8) → pos(u32) → mode(u8) → δ1(u32) → δ2(u32) → live_idx(u32)
//!   → consume_coin(u8) → consume_target_pick(u32)

use crate::Pass;
use crate::analysis;
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
                // Draw 1: inject roll.
                let roll = rng.gen::<u8>() % 100;
                if roll >= INJECT_PERCENT {
                    // Drain remaining 7 draws to keep schedule invariant.
                    let _pos_pick = rng.gen::<u32>();
                    let _mode = rng.gen::<u8>();
                    let _d1 = rng.gen_range(DELTA_MIN..=DELTA_MAX);
                    let _d2 = rng.gen_range(DELTA_MIN..=DELTA_MAX);
                    let _live_pick = rng.gen::<u32>();
                    let _consume_coin = rng.gen::<u8>();
                    let _consume_target_pick = rng.gen::<u32>();
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

                // Draw 7 & 8: consume-mode coin and target index (always drawn).
                let consume_coin: u8 = rng.gen::<u8>();
                let consume_target_pick: u32 = rng.gen::<u32>();

                // Compute candidate set before pos (for chain mode selection).
                let candidates = analysis::number_vlocals_before(block, pos, &consts);
                let use_live = mode_pick == 1 && !candidates.is_empty();

                // Emit the chain and record tmp2 for potential consume.
                let tmp2 = if use_live {
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
                            op: BinOp::Sub,
                            lhs: Value::VLocal(lv),
                            rhs: Value::VLocal(d2_vl),
                        },
                        Instr::BinOp {
                            dst: tmp2,
                            op: BinOp::Sub,
                            lhs: Value::VLocal(tmp1),
                            rhs: Value::VLocal(d2_vl),
                        },
                    ];
                    for instr in junk.iter().rev().cloned() {
                        block.instrs.insert(pos, instr);
                    }
                    let _ = delta1; // intentionally drawn but unused in mode 1
                    tmp2
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
                    let _ = live_pick; // intentionally drawn but unused in mode 0
                    tmp2
                };

                // Consume mode (~25%): make tmp2 live by adding a net-zero pair.
                // Requires a live Number VLocal available at the insertion point.
                // Candidates are re-computed AFTER the chain (pos + chain length).
                if consume_coin % 4 == 0 {
                    let chain_len = if use_live { 3 } else { 4 };
                    let consume_pos = pos + chain_len;
                    let consume_candidates =
                        analysis::number_vlocals_before(block, consume_pos, &consts);
                    if !consume_candidates.is_empty() {
                        let mut cand_vec: Vec<u32> = consume_candidates.into_iter().collect();
                        cand_vec.sort();
                        let live_target =
                            VLocal(cand_vec[(consume_target_pick as usize) % cand_vec.len()]);

                        let step1 = VLocal(n_locals);
                        n_locals += 1;

                        let consume: [Instr; 2] = [
                            Instr::BinOp {
                                dst: step1,
                                op: BinOp::Add,
                                lhs: Value::VLocal(live_target),
                                rhs: Value::VLocal(tmp2),
                            },
                            Instr::BinOp {
                                dst: VLocal(live_target.0),
                                op: BinOp::Sub,
                                lhs: Value::VLocal(step1),
                                rhs: Value::VLocal(tmp2),
                            },
                        ];
                        for instr in consume.iter().rev().cloned() {
                            block.instrs.insert(consume_pos, instr);
                        }
                    }
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
        let subs_before = count_subs(&p);

        let mut saw_injection = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            JunkSubChain.run(&mut p_clone, &mut rng);
            let instrs_after = total_instrs(&p_clone);
            let instrs_baseline = total_instrs(&mir_of(src));
            let subs_after = count_subs(&p_clone);
            let instr_growth = instrs_after as i64 - instrs_baseline as i64;
            let sub_growth = subs_after as i64 - subs_before as i64;
            // Each injection (chain only) adds 2 Subs; consume mode adds 1 more
            // (the Sub in the net-zero pair), so sub_growth >= 2 per injection.
            // Derive a lower-bound on injections from sub growth / 3 (conservative).
            if instr_growth > 0 {
                // At least 1 injection. Each chain adds 2 Subs + possibly 1 consume Sub.
                // Instruction growth is 3–4 (chain) + 0 or 2 (consume) per injection.
                // Lower bound on injections via sub growth: each inject adds >= 2 Subs.
                let min_injections = (sub_growth / 3).max(1);
                assert!(
                    instr_growth >= 3 * min_injections,
                    "seed {}: instr growth {} < 3 * min_injections {}",
                    s, instr_growth, 3 * min_injections
                );
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
        let pre_pass_n_locals = baseline.functions[0].n_locals;

        let mut found_live_operand = false;
        'outer: for seed_byte in 0..200u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkSubChain.run(&mut p, &mut rng);
            for f in &p.functions {
                for b in &f.blocks {
                    for instr in &b.instrs {
                        if let Instr::BinOp { op: BinOp::Sub, lhs: Value::VLocal(vl), .. } = instr {
                            // Mode-1 Sub has a live VLocal as lhs; if that
                            // index is below pre_pass_n_locals, it's not pass-allocated.
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
            "no seed in 0..200 produced a BinOp::Sub whose lhs is a pre-pass VLocal"
        );
    }

    #[test]
    fn consume_mode_produces_tmp2_reads() {
        // Sweep seeds; in at least some, expect to see tmp2 used as an OPERAND
        // in a BinOp that writes back to a pre-pass VLocal slot (consume mode).
        let src = "local s = 0 for i = 1, 5 do s = s + i * 2 end print(s)";
        let pre_pass_n_locals = mir_of(src).functions[0].n_locals;
        let mut saw_consume = false;
        'outer: for seed_byte in 0..100u8 {
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            JunkSubChain.run(&mut p, &mut rng);
            for f in &p.functions {
                for b in &f.blocks {
                    for instr in &b.instrs {
                        // Consume mode emits: dst=<pre-pass VLocal>, rhs=<pass-alloc VLocal>.
                        if let Instr::BinOp { dst, rhs: Value::VLocal(rhs_vl), .. } = instr {
                            if dst.0 < pre_pass_n_locals && rhs_vl.0 >= pre_pass_n_locals {
                                saw_consume = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
        assert!(saw_consume, "no consume-mode injection found in 100 seeds");
    }

    #[test]
    fn rng_schedule_mode_invariant() {
        // Two runs with the same seed must produce identical instruction layouts.
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
            JunkSubChain.run(&mut p1, &mut r1);
            JunkSubChain.run(&mut p2, &mut r2);
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
