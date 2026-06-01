//! Placeholder — real implementation lands in Task 8.

use crate::{lower::FnBuilder, MirError};
use luau_hir::HirStmt;

pub(crate) fn lower(_b: &mut FnBuilder, stmts: &[HirStmt]) -> Result<(), MirError> {
    if stmts.is_empty() {
        // Lets empty-program tests pass before Task 8 lands real impl.
        Ok(())
    } else {
        Err(MirError::Unsupported(
            "statement lowering not implemented yet (Task 8)".into(),
        ))
    }
}
