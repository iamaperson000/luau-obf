//! Shared dataflow / static-analysis utilities used by multiple passes.

pub mod numeric;

/// Returns the set of VLocal indices that are provably Number-typed at
/// the END of position `before` (exclusive) in this block, given the
/// function's constant pool.
///
/// Conservative forward scan (single pass, no fixed-point):
/// - `LoadConst { dst, src }` where `consts[src.0]` is `Constant::Number(_)` → add dst.
/// - `BinOp { dst, op ∈ Add|Sub|Mul|Div|Mod|Pow, lhs, rhs }` where both operands
///   are provably numeric (VLocal in set, or Const(c) where consts[c.0] is Number) → add dst.
/// - Anything else that writes a dst → remove dst (conservative kill).
pub fn number_vlocals_before(
    block: &luau_mir::BasicBlock,
    before: usize,
    consts: &[luau_mir::Constant],
) -> std::collections::HashSet<u32> {
    use luau_hir::BinOp;
    use luau_mir::{Constant, Instr, Value};

    let mut set: std::collections::HashSet<u32> = std::collections::HashSet::new();

    let limit = before.min(block.instrs.len());
    for instr in &block.instrs[..limit] {
        let is_numeric_value = |v: &Value| -> bool {
            match v {
                Value::Const(c) => matches!(consts.get(c.0 as usize), Some(Constant::Number(_))),
                Value::VLocal(vl) => set.contains(&vl.0),
            }
        };

        match instr {
            Instr::LoadConst { dst, src } => {
                if matches!(consts.get(src.0 as usize), Some(Constant::Number(_))) {
                    set.insert(dst.0);
                } else {
                    set.remove(&dst.0);
                }
            }
            Instr::BinOp { dst, op, lhs, rhs } => {
                let numeric_op = matches!(
                    op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
                );
                if numeric_op && is_numeric_value(lhs) && is_numeric_value(rhs) {
                    set.insert(dst.0);
                } else {
                    set.remove(&dst.0);
                }
            }
            // All other instructions with destinations conservatively kill numeric status.
            Instr::Move { dst, .. }
            | Instr::UnOp { dst, .. }
            | Instr::GetGlobal { dst, .. }
            | Instr::GetIndex { dst, .. }
            | Instr::GetUpval { dst, .. }
            | Instr::GetVarargs { dst }
            | Instr::NewTable { dst }
            | Instr::BuildResults { dst, .. }
            | Instr::MakeClosure { dst, .. } => {
                set.remove(&dst.0);
            }
            Instr::Call { dst: Some(d), .. } | Instr::CallVar { dst: Some(d), .. } => {
                set.remove(&d.0);
            }
            // Instructions with no destination (side-effects only).
            Instr::Call { dst: None, .. }
            | Instr::CallVar { dst: None, .. }
            | Instr::SetGlobal { .. }
            | Instr::SetIndex { .. }
            | Instr::SetUpval { .. } => {}
        }
    }

    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::BinOp;
    use luau_mir::{BasicBlock, BlockId, Constant, ConstId, Instr, Terminator, VLocal, Value};

    fn block_with_instrs(instrs: Vec<Instr>) -> BasicBlock {
        BasicBlock {
            id: BlockId(0),
            instrs,
            terminator: Terminator::Return(None),
        }
    }

    #[test]
    fn empty_block_returns_empty_set() {
        let block = block_with_instrs(vec![]);
        let consts: Vec<Constant> = vec![];
        let set = number_vlocals_before(&block, 0, &consts);
        assert!(set.is_empty());
    }

    #[test]
    fn loadconst_number_included() {
        let consts = vec![Constant::Number(1.0)];
        let block = block_with_instrs(vec![
            Instr::LoadConst { dst: VLocal(0), src: ConstId(0) },
        ]);
        let set = number_vlocals_before(&block, 1, &consts);
        assert!(set.contains(&0));
    }

    #[test]
    fn loadconst_string_excluded() {
        let consts = vec![Constant::String("hi".into())];
        let block = block_with_instrs(vec![
            Instr::LoadConst { dst: VLocal(0), src: ConstId(0) },
        ]);
        let set = number_vlocals_before(&block, 1, &consts);
        assert!(!set.contains(&0));
    }

    #[test]
    fn binop_of_numbers_included() {
        let consts = vec![Constant::Number(2.0)];
        let block = block_with_instrs(vec![
            Instr::LoadConst { dst: VLocal(0), src: ConstId(0) },
            Instr::LoadConst { dst: VLocal(1), src: ConstId(0) },
            Instr::BinOp {
                dst: VLocal(2),
                op: BinOp::Add,
                lhs: Value::VLocal(VLocal(0)),
                rhs: Value::VLocal(VLocal(1)),
            },
        ]);
        let set = number_vlocals_before(&block, 3, &consts);
        assert!(set.contains(&0));
        assert!(set.contains(&1));
        assert!(set.contains(&2));
    }

    #[test]
    fn before_boundary_respected() {
        let consts = vec![Constant::Number(1.0)];
        let block = block_with_instrs(vec![
            Instr::LoadConst { dst: VLocal(0), src: ConstId(0) },
            Instr::LoadConst { dst: VLocal(1), src: ConstId(0) },
        ]);
        // Only scan up to position 1 (exclusive), so VLocal(1) not yet seen.
        let set = number_vlocals_before(&block, 1, &consts);
        assert!(set.contains(&0));
        assert!(!set.contains(&1));
    }
}
