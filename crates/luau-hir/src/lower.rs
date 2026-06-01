//! AST → HIR lowering. Plan 1 supports the subset documented in the plan.

use crate::{HirError, HirProgram};
use luau_parse::Ast;

/// Lower a parsed AST into HIR (without name resolution applied yet).
pub fn lower(_ast: &Ast) -> Result<HirProgram, HirError> {
    todo!("implemented in later tasks")
}
