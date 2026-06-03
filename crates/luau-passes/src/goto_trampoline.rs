//! Goto-Trampoline pass — for each `Terminator::Goto(target)`, with 30%
//! probability, insert an empty intermediate `BasicBlock` whose terminator
//! is `Goto(target)`, then rewrite the original Goto to point at the
//! intermediate. CFG inflates without semantic change.

use crate::Pass;
use luau_mir::{BasicBlock, BlockId, MirProgram, Terminator};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct GotoTrampoline;

const TRAMPOLINE_PERCENT: u8 = 30;

impl Pass for GotoTrampoline {
    fn name(&self) -> &'static str {
        "goto-trampoline"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            if f.blocks.is_empty() {
                continue;
            }
            // Snapshot block indices that have Goto terminators.
            let goto_indices: Vec<usize> = f
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(idx, b)| match b.terminator {
                    Terminator::Goto(_) => Some(idx),
                    _ => None,
                })
                .collect();
            let mut next_block_id_value = f
                .blocks
                .iter()
                .map(|b| b.id.0)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
            let mut new_blocks: Vec<BasicBlock> = Vec::new();
            for idx in goto_indices {
                let roll = rng.gen::<u8>() % 100;
                if roll >= TRAMPOLINE_PERCENT {
                    continue;
                }
                let original_target = match f.blocks[idx].terminator {
                    Terminator::Goto(t) => t,
                    _ => continue,
                };
                let new_id = BlockId(next_block_id_value);
                next_block_id_value += 1;
                let trampoline = BasicBlock {
                    id: new_id,
                    instrs: Vec::new(),
                    terminator: Terminator::Goto(original_target),
                };
                new_blocks.push(trampoline);
                f.blocks[idx].terminator = Terminator::Goto(new_id);
            }
            f.blocks.extend(new_blocks);
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

    fn total_blocks(p: &MirProgram) -> usize {
        p.functions.iter().map(|f| f.blocks.len()).sum()
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

    #[test]
    fn inserts_some_trampolines() {
        let src = "
            local s = 0
            for i = 1, 5 do
                s = s + i
            end
            print(s)
        ";
        let p = mir_of(src);
        let gotos_before = count_gotos(&p);
        let blocks_before = total_blocks(&p);
        assert!(gotos_before >= 1, "expected ≥1 Goto, got {}", gotos_before);

        let mut saw_inflation = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            GotoTrampoline.run(&mut p_clone, &mut rng);
            let blocks_after = total_blocks(&p_clone);
            let gotos_after = count_gotos(&p_clone);
            // Each insertion: +1 block, +1 Goto (the trampoline's own).
            // The original Goto is REROUTED, not removed, so it still counts.
            let added_blocks = blocks_after as i64 - blocks_before as i64;
            let added_gotos = gotos_after as i64 - gotos_before as i64;
            assert_eq!(
                added_blocks, added_gotos,
                "seed {}: block growth ({}) != Goto growth ({})",
                s, added_blocks, added_gotos
            );
            if added_blocks > 0 {
                saw_inflation = true;
            }
        }
        assert!(saw_inflation, "no seed 0..5 inserted any trampoline");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "
            local s = 0
            for i = 1, 3 do s = s + i end
            print(s)
        ";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        GotoTrampoline.run(&mut p1, &mut r1);
        GotoTrampoline.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    s.push_str(&format!("block {:?}: ", b.id));
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}, ", i));
                    }
                    s.push_str(&format!("term: {:?}\n", b.terminator));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn programs_with_no_gotos_are_unchanged() {
        let src = "print('hi')";
        let mut p = mir_of(src);
        let blocks_before = total_blocks(&p);
        for s in 0..20u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            GotoTrampoline.run(&mut p_clone, &mut rng);
            assert_eq!(total_blocks(&p_clone), blocks_before, "seed {}: blocks grew", s);
        }
        let _ = p;
    }

    #[test]
    fn each_trampoline_block_has_no_instrs() {
        // After a run, every newly-added block should be empty (only a Goto terminator).
        let src = "
            local s = 0
            for i = 1, 10 do s = s + i end
            for j = 1, 10 do s = s + j end
            print(s)
        ";
        for seed_byte in 0..20u8 {
            let baseline_block_ids: std::collections::HashSet<u32> = mir_of(src)
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| b.id.0)
                .collect();
            let mut p = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([seed_byte; 32]);
            GotoTrampoline.run(&mut p, &mut rng);
            for f in &p.functions {
                for b in &f.blocks {
                    if !baseline_block_ids.contains(&b.id.0) {
                        // It's a new trampoline. Must have empty instrs and Goto terminator.
                        assert!(b.instrs.is_empty(),
                            "seed {}: new block {:?} has {} instrs",
                            seed_byte, b.id, b.instrs.len());
                        assert!(matches!(b.terminator, Terminator::Goto(_)),
                            "seed {}: new block {:?} has non-Goto terminator",
                            seed_byte, b.id);
                    }
                }
            }
        }
    }
}
