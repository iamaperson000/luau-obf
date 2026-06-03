//! Maps symbolic OpKinds to integer opcode bytes.
//!
//! In Plan 3 the mapping is still deterministic and seed-independent.
//! Plan 4+ introduces per-build randomization: keep the API stable.

use luau_lir::OpKind;

pub struct OpMap {
    /// table[OpKind index] = opcode byte
    forward: [u8; 35],
}

impl OpMap {
    pub fn new(_seed: &[u8; 32]) -> Self {
        let mut forward = [0u8; 35];
        for (i, slot) in forward.iter_mut().enumerate() {
            *slot = (i + 1) as u8;
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
];

fn op_index(k: OpKind) -> usize {
    ALL_OPS.iter().position(|x| *x == k).expect("known op")
}
