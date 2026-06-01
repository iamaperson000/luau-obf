//! HIR — high-level IR for Luau. AST → HIR lowering and name resolution.

mod types;
pub mod lower;
pub mod resolve;

pub use types::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HirError {
    #[error("parse error: {0}")]
    Parse(#[from] luau_parse::ParseError),
    #[error("unsupported construct: {0}")]
    Unsupported(String),
}
