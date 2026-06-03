//! Intra-block forward dataflow tracking which `VLocal`s are
//! **provably numeric** at each point in a basic block.
//!
//! A VLocal is provably numeric if its most-recent same-block def is:
//! - `LoadConst Number`,
//! - a numeric arithmetic `BinOp` (`Add`/`Sub`/`Mul`/`Div`/`Mod`/`Pow`) on
//!   numeric operands,
//! - `UnOp::Neg` on a numeric operand,
//! - `UnOp::Len` (always numeric in Lua/Luau), or
//! - `Move` from a provably-numeric VLocal.
//!
//! Anything else (parameters, prior-block values, `Call` returns,
//! `GetIndex`, `GetGlobal`, `GetUpval`, `NewTable`, `BuildResults`,
//! `MakeClosure`, `GetVarargs`) is conservatively NOT provably numeric.
//!
//! Passes that materialise synthetic numeric operands into existing
//! arithmetic (e.g. `(x + k) * y − k * y`) MUST gate on this set —
//! otherwise the rewrite changes operand types and crashes user code that
//! overloads `__add` / `__mul` / etc.

use luau_hir::{BinOp, UnOp};
use luau_mir::{Constant, Instr, Value};
use std::collections::HashSet;

/// Set of VLocal indices known to be numeric at the current point in a
/// basic block. Construct one per block, fresh.
#[derive(Default, Debug)]
pub struct NumericLocals {
    set: HashSet<u32>,
}

impl NumericLocals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, vlocal_idx: u32) -> bool {
        self.set.contains(&vlocal_idx)
    }

    pub fn insert(&mut self, vlocal_idx: u32) {
        self.set.insert(vlocal_idx);
    }

    pub fn remove(&mut self, vlocal_idx: u32) {
        self.set.remove(&vlocal_idx);
    }

    /// Whether the given `Value` is provably numeric at this point.
    pub fn is_value_numeric(&self, v: &Value, consts: &[Constant]) -> bool {
        match v {
            Value::Const(c) => {
                matches!(consts.get(c.0 as usize), Some(Constant::Number(_)))
            }
            Value::VLocal(v) => self.set.contains(&v.0),
        }
    }

    /// Apply the effect of executing `instr` to the numeric set.
    /// Insert or remove `dst` based on whether the produced value is
    /// provably numeric.
    pub fn update(&mut self, instr: &Instr, consts: &[Constant]) {
        let (dst, is_num) = match instr {
            Instr::LoadConst { dst, src } => {
                let n = matches!(
                    consts.get(src.0 as usize),
                    Some(Constant::Number(_))
                );
                (Some(*dst), n)
            }
            Instr::BinOp { dst, op, lhs, rhs } => {
                let numeric_op = matches!(
                    op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
                );
                let n = numeric_op
                    && self.is_value_numeric(lhs, consts)
                    && self.is_value_numeric(rhs, consts);
                (Some(*dst), n)
            }
            Instr::UnOp { dst, op, operand } => {
                let n = match op {
                    UnOp::Len => true,
                    UnOp::Neg => self.is_value_numeric(operand, consts),
                    _ => false,
                };
                (Some(*dst), n)
            }
            Instr::Move { dst, src } => {
                let n = self.set.contains(&src.0);
                (Some(*dst), n)
            }
            Instr::Call { dst: Some(d), .. } | Instr::CallVar { dst: Some(d), .. } => {
                (Some(*d), false)
            }
            Instr::Call { dst: None, .. } | Instr::CallVar { dst: None, .. } => (None, false),
            Instr::GetGlobal { dst, .. } => (Some(*dst), false),
            Instr::GetIndex { dst, .. } => (Some(*dst), false),
            Instr::GetUpval { dst, .. } => (Some(*dst), false),
            Instr::GetVarargs { dst } => (Some(*dst), false),
            Instr::NewTable { dst } => (Some(*dst), false),
            Instr::BuildResults { dst, .. } => (Some(*dst), false),
            Instr::MakeClosure { dst, .. } => (Some(*dst), false),
            Instr::SetGlobal { .. } | Instr::SetIndex { .. } | Instr::SetUpval { .. } => {
                (None, false)
            }
        };
        if let Some(d) = dst {
            if is_num {
                self.set.insert(d.0);
            } else {
                self.set.remove(&d.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::{ConstId, VLocal};

    fn vl(i: u32) -> VLocal { VLocal(i) }

    #[test]
    fn loadconst_number_marks_numeric() {
        let consts = vec![Constant::Number(3.14)];
        let mut nl = NumericLocals::new();
        nl.update(
            &Instr::LoadConst { dst: vl(0), src: ConstId(0) },
            &consts,
        );
        assert!(nl.contains(0));
    }

    #[test]
    fn loadconst_string_doesnt_mark_numeric() {
        let consts = vec![Constant::String("hi".into())];
        let mut nl = NumericLocals::new();
        nl.update(
            &Instr::LoadConst { dst: vl(0), src: ConstId(0) },
            &consts,
        );
        assert!(!nl.contains(0));
    }

    #[test]
    fn add_of_numerics_is_numeric() {
        let consts = vec![Constant::Number(1.0)];
        let mut nl = NumericLocals::new();
        nl.update(&Instr::LoadConst { dst: vl(0), src: ConstId(0) }, &consts);
        nl.update(&Instr::LoadConst { dst: vl(1), src: ConstId(0) }, &consts);
        nl.update(
            &Instr::BinOp {
                dst: vl(2),
                op: BinOp::Add,
                lhs: Value::VLocal(vl(0)),
                rhs: Value::VLocal(vl(1)),
            },
            &consts,
        );
        assert!(nl.contains(2));
    }

    #[test]
    fn add_with_non_numeric_operand_isnt_numeric() {
        let consts = vec![Constant::Number(1.0)];
        let mut nl = NumericLocals::new();
        // vl(0) is from a Call — not numeric.
        nl.update(
            &Instr::Call { dst: Some(vl(0)), callee: Value::VLocal(vl(99)), args: vec![] },
            &consts,
        );
        nl.update(&Instr::LoadConst { dst: vl(1), src: ConstId(0) }, &consts);
        nl.update(
            &Instr::BinOp {
                dst: vl(2),
                op: BinOp::Add,
                lhs: Value::VLocal(vl(0)),
                rhs: Value::VLocal(vl(1)),
            },
            &consts,
        );
        assert!(!nl.contains(2));
    }

    #[test]
    fn move_propagates_numeric_status() {
        let consts = vec![Constant::Number(7.0)];
        let mut nl = NumericLocals::new();
        nl.update(&Instr::LoadConst { dst: vl(0), src: ConstId(0) }, &consts);
        nl.update(&Instr::Move { dst: vl(1), src: vl(0) }, &consts);
        assert!(nl.contains(1));
    }

    #[test]
    fn len_is_always_numeric() {
        let consts: Vec<Constant> = vec![];
        let mut nl = NumericLocals::new();
        // vl(0) from NewTable: not numeric.
        nl.update(&Instr::NewTable { dst: vl(0) }, &consts);
        nl.update(
            &Instr::UnOp { dst: vl(1), op: UnOp::Len, operand: Value::VLocal(vl(0)) },
            &consts,
        );
        assert!(nl.contains(1));
    }
}
