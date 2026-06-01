//! MIR — mid-level IR for Luau. HIR → MIR lowering.

mod types;
pub mod lower;
mod lower_stmts;

pub use types::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MirError {
    #[error("hir error: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("unsupported: {0}")]
    Unsupported(String),
}
