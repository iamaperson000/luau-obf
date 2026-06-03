//! Maps symbolic OpKinds to integer opcode bytes.
//!
//! Plan 7+: per-build randomization. Same API; the mapping is a seeded
//! permutation of the bytes 1..=N where N = ALL_OPS.len().

use luau_lir::OpKind;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

pub struct OpMap {
    /// table[OpKind index] = opcode byte
    forward: [u8; 36],
}

impl OpMap {
    pub fn new(seed: &[u8; 32]) -> Self {
        let mut rng = ChaCha20Rng::from_seed(*seed);
        let mut bytes: Vec<u8> = (1..=ALL_OPS.len() as u8).collect();
        bytes.shuffle(&mut rng);
        let mut forward = [0u8; 36];
        for (i, b) in bytes.into_iter().enumerate() {
            forward[i] = b;
        }
        OpMap { forward }
    }

    pub fn opcode_of(&self, k: OpKind) -> u8 {
        let idx = op_index(k);
        self.forward[idx]
    }

    pub fn assigned_pairs(&self) -> Vec<(OpKind, u8)> {
        ALL_OPS.iter().map(|k| (*k, self.opcode_of(*k))).collect()
    }
}

pub const ALL_OPS: &[OpKind] = &[
    OpKind::LoadNil, OpKind::LoadTrue, OpKind::LoadFalse, OpKind::LoadConst,
    OpKind::Move,
    OpKind::Add, OpKind::Sub, OpKind::Mul, OpKind::Div, OpKind::Mod, OpKind::Pow,
    OpKind::Concat,
    OpKind::Lt, OpKind::Le, OpKind::Eq,
    OpKind::Not, OpKind::Neg, OpKind::Len,
    OpKind::GetGlobal, OpKind::SetGlobal,
    OpKind::Call, OpKind::Return,
    OpKind::Jmp, OpKind::JmpIfTrue, OpKind::JmpIfFalse,
    OpKind::Closure,
    OpKind::NewTable, OpKind::GetTable, OpKind::SetTable,
    OpKind::GetUpval, OpKind::SetUpval,
    OpKind::CallVar, OpKind::BuildResults, OpKind::Vararg, OpKind::ReturnMulti,
    // Plan 30: encode-time superop (fused LoadConst pair).
    OpKind::LoadConstLoadConst,
];

fn op_index(k: OpKind) -> usize {
    ALL_OPS.iter().position(|x| *x == k).expect("known op")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_mapping() {
        let m1 = OpMap::new(&[7u8; 32]);
        let m2 = OpMap::new(&[7u8; 32]);
        for k in ALL_OPS {
            assert_eq!(m1.opcode_of(*k), m2.opcode_of(*k));
        }
    }

    #[test]
    fn different_seeds_produce_different_mappings() {
        let m1 = OpMap::new(&[1u8; 32]);
        let m2 = OpMap::new(&[2u8; 32]);
        // At least one opcode must differ between the two mappings.
        let any_diff = ALL_OPS.iter().any(|k| m1.opcode_of(*k) != m2.opcode_of(*k));
        assert!(any_diff, "different seeds produced identical opmap");
    }

    #[test]
    fn opcodes_are_a_permutation_of_1_to_n() {
        let m = OpMap::new(&[42u8; 32]);
        let mut bytes: Vec<u8> = ALL_OPS.iter().map(|k| m.opcode_of(*k)).collect();
        bytes.sort_unstable();
        let expected: Vec<u8> = (1..=ALL_OPS.len() as u8).collect();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn opcode_zero_is_never_assigned() {
        // 0 is reserved as a halt sentinel.
        let m = OpMap::new(&[99u8; 32]);
        for k in ALL_OPS {
            assert_ne!(m.opcode_of(*k), 0);
        }
    }
}
