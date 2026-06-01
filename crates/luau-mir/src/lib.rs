//! MIR — mid-level IR for Luau. HIR → MIR lowering.

mod types;
pub mod lower;

pub use types::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MirError {
    #[error("hir error: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("unsupported in Plan 1 subset: {0}")]
    Unsupported(String),
}
