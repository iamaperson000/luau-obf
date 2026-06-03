//! LIR-level constant-pool shuffling.
//!
//! For each function, computes a permutation σ over its constant pool indices,
//! reorders the consts vec accordingly, and rewrites every `Operand::Const`
//! reference to point at the new index. Pure relabelling — the program's
//! runtime behavior is identical, but the bytes in the constant pool and the
//! ConstIdx operands embedded in the bytecode are different.

use crate::{ConstIdx, LirProgram, Operand};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha20Rng;

pub fn shuffle_constants(program: &mut LirProgram, rng: &mut ChaCha20Rng) {
    for f in &mut program.functions {
        let n = f.consts.len();
        if n <= 1 {
            continue;
        }
        // sigma[i] = new position of what was at old position i.
        let mut sigma: Vec<usize> = (0..n).collect();
        sigma.shuffle(rng);
        // Reorder consts: new[sigma[i]] = old[i].
        let mut new_consts = vec![f.consts[0].clone(); n];
        for (old_i, c) in f.consts.iter().enumerate() {
            new_consts[sigma[old_i]] = c.clone();
        }
        f.consts = new_consts;
        // Rewrite every Operand::Const(ConstIdx(i)) → ConstIdx(sigma[i]).
        for instr in &mut f.instrs {
            for operand in &mut instr.operands {
                if let Operand::Const(ConstIdx(i)) = operand {
                    let old = *i as usize;
                    *i = sigma[old] as u16;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LirFunction, LirInstr, OpKind, ProtoId, Reg};
    use luau_mir::Constant;
    use rand::SeedableRng;

    fn make_program() -> LirProgram {
        // A single function with 3 constants and one LoadConst per constant.
        let f = LirFunction {
            id: ProtoId(0),
            num_params: 0,
            num_regs: 3,
            num_upvals: 0,
            is_vararg: false,
            consts: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(30.0),
            ],
            instrs: vec![
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(0)), Operand::Const(ConstIdx(0))] },
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(1)), Operand::Const(ConstIdx(1))] },
                LirInstr { op: OpKind::LoadConst, operands: vec![Operand::Reg(Reg(2)), Operand::Const(ConstIdx(2))] },
            ],
            label_positions: vec![],
            closure_upval_sources: vec![],
            build_results_values: vec![],
        };
        LirProgram { functions: vec![f] }
    }

    fn collect_loaded_pairs(p: &LirProgram) -> Vec<(u16, Constant)> {
        // For each LoadConst, return (dst_reg, the actual constant it loads).
        let f = &p.functions[0];
        let mut pairs = Vec::new();
        for i in &f.instrs {
            if i.op != OpKind::LoadConst {
                continue;
            }
            let Operand::Reg(Reg(r)) = i.operands[0] else { unreachable!() };
            let Operand::Const(ConstIdx(c)) = i.operands[1] else { unreachable!() };
            pairs.push((r, f.consts[c as usize].clone()));
        }
        pairs
    }

    #[test]
    fn shuffle_preserves_constant_per_register() {
        // For each register, the constant it loads must be the same before and after.
        let mut before = make_program();
        let pairs_before = collect_loaded_pairs(&before);
        let mut rng = ChaCha20Rng::from_seed([5u8; 32]);
        shuffle_constants(&mut before, &mut rng);
        let pairs_after = collect_loaded_pairs(&before);
        assert_eq!(pairs_before, pairs_after);
    }

    #[test]
    fn shuffle_actually_reorders_for_typical_seed() {
        let mut p = make_program();
        let consts_before = p.functions[0].consts.clone();
        let mut rng = ChaCha20Rng::from_seed([99u8; 32]);
        shuffle_constants(&mut p, &mut rng);
        // Very high probability the order changed (only 1/6 chance of identity for n=3).
        assert_ne!(p.functions[0].consts, consts_before);
    }

    #[test]
    fn shuffle_is_deterministic_for_same_seed() {
        let mut p1 = make_program();
        let mut p2 = make_program();
        let mut r1 = ChaCha20Rng::from_seed([3u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([3u8; 32]);
        shuffle_constants(&mut p1, &mut r1);
        shuffle_constants(&mut p2, &mut r2);
        assert_eq!(p1.functions[0].consts, p2.functions[0].consts);
    }
}
