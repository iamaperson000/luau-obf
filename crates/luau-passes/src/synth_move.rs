//! Synthetic-Move pass — for each MIR `BasicBlock` with 25% probability,
//! find the first instruction writing a VLocal and insert a `Move` from
//! that VLocal to a fresh (dead) VLocal immediately after it. Adds Move
//! opcodes throughout the bytecode without changing semantics.

use crate::Pass;
use luau_mir::{Instr, MirProgram, VLocal};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct SyntheticMove;

const INSERT_PERCENT: u8 = 25;

fn instr_dst(instr: &Instr) -> Option<VLocal> {
    match instr {
        Instr::LoadConst { dst, .. } => Some(*dst),
        Instr::Move { dst, .. } => Some(*dst),
        Instr::BinOp { dst, .. } => Some(*dst),
        Instr::UnOp { dst, .. } => Some(*dst),
        Instr::GetGlobal { dst, .. } => Some(*dst),
        Instr::Call { dst, .. } => *dst,
        Instr::CallVar { dst, .. } => *dst,
        Instr::BuildResults { dst, .. } => Some(*dst),
        Instr::GetVarargs { dst } => Some(*dst),
        Instr::MakeClosure { dst, .. } => Some(*dst),
        Instr::GetUpval { dst, .. } => Some(*dst),
        Instr::NewTable { dst } => Some(*dst),
        Instr::GetIndex { dst, .. } => Some(*dst),
        Instr::SetGlobal { .. } | Instr::SetIndex { .. } | Instr::SetUpval { .. } => None,
    }
}

impl Pass for SyntheticMove {
    fn name(&self) -> &'static str {
        "synthetic-move"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            for block in f.blocks.iter_mut() {
                let roll = rng.gen::<u8>() % 100;
                if roll >= INSERT_PERCENT {
                    continue;
                }
                let (insert_after_idx, src_vl) = match block
                    .instrs
                    .iter()
                    .enumerate()
                    .find_map(|(idx, instr)| instr_dst(instr).map(|vl| (idx, vl)))
                {
                    Some((i, vl)) => (i, vl),
                    None => continue,
                };
                let dst = VLocal(n_locals);
                n_locals += 1;
                block.instrs.insert(
                    insert_after_idx + 1,
                    Instr::Move { dst, src: src_vl },
                );
            }
            f.n_locals = n_locals;
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

    fn count_moves(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::Move { .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
    }

    #[test]
    fn inserts_some_moves() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let moves_before = count_moves(&p);
        let blocks = total_blocks(&p);
        assert!(blocks >= 3, "expected ≥3 blocks, got {}", blocks);

        let mut saw_insert = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            SyntheticMove.run(&mut p_clone, &mut rng);
            if count_moves(&p_clone) > moves_before {
                saw_insert = true;
                break;
            }
        }
        assert!(saw_insert, "no seed 0..5 inserted any synthetic Move");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local x = 1 x = x + 2 print(x)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        SyntheticMove.run(&mut p1, &mut r1);
        SyntheticMove.run(&mut p2, &mut r2);
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
    fn terminators_and_blocks_are_unchanged() {
        let src = "
            local s = 0
            for i = 1, 3 do s = s + i end
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
            let baseline_block_count = total_blocks(&baseline);
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            SyntheticMove.run(&mut p, &mut rng);
            let after_terms: Vec<_> = p
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| format!("{:?}", b.terminator))
                .collect();
            assert_eq!(baseline_terms, after_terms, "seed {}: terminator changed", seed_byte);
            assert_eq!(total_blocks(&p), baseline_block_count, "seed {}: block count changed", seed_byte);
        }
    }

    #[test]
    fn each_inserted_move_has_fresh_dst() {
        let src = "local s = 0 for i = 1, 5 do s = s + i end print(s)";
        for seed_byte in 0..20u8 {
            let baseline_n_locals: Vec<u32> =
                mir_of(src).functions.iter().map(|f| f.n_locals).collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            SyntheticMove.run(&mut p, &mut rng);
            for (i, f) in p.functions.iter().enumerate() {
                assert!(
                    f.n_locals >= baseline_n_locals[i],
                    "seed {}: function {} n_locals shrunk",
                    seed_byte, i
                );
            }
        }
    }
}
