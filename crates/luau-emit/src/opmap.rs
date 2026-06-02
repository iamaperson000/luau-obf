//! Maps symbolic OpKinds to integer opcode bytes.
//!
//! In Plan 1 the mapping is deterministic and seed-independent. Plan 3 introduces
//! per-build randomization: keep the API stable so we just swap implementations.

use luau_lir::OpKind;

pub struct OpMap {
    /// table[OpKind index] = opcode byte
    forward: [u8; 26],
}

impl OpMap {
    /// Build a fresh map. `_seed` is the entropy source for Plan 3; ignored here.
    pub fn new(_seed: &[u8; 32]) -> Self {
        let mut forward = [0u8; 26];
        for i in 0..26 {
            forward[i] = (i + 1) as u8;
        }
        OpMap { forward }
    }

    pub fn opcode_of(&self, k: OpKind) -> u8 {
        let idx = op_index(k);
        self.forward[idx]
    }

    /// All used opcode bytes in their assignment order (for the VM dispatch table).
    pub fn assigned_pairs(&self) -> Vec<(OpKind, u8)> {
        ALL_OPS.iter().map(|k| (*k, self.opcode_of(*k))).collect()
    }
}

pub const ALL_OPS: &[OpKind] = &[
    OpKind::LoadNil, OpKind::LoadTrue, OpKind::LoadFalse, OpKind::LoadConst,
    OpKind::Move, OpKind::Add, OpKind::Sub, OpKind::Mul, OpKind::Div, OpKind::Mod,
    OpKind::Pow, OpKind::Concat, OpKind::Lt, OpKind::Le, OpKind::Eq,
    OpKind::Not, OpKind::Neg, OpKind::Len,
    OpKind::GetGlobal, OpKind::SetGlobal, OpKind::Call, OpKind::Return,
    OpKind::Jmp, OpKind::JmpIfTrue, OpKind::JmpIfFalse, OpKind::Closure,
];

fn op_index(k: OpKind) -> usize {
    ALL_OPS.iter().position(|x| *x == k).expect("known op")
}
