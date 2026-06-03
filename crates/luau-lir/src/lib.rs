//! LIR — low-level IR. Linear bytecode for the custom ISA.

mod types;
mod regalloc;
pub mod lower;
pub mod shuffle;

pub use types::*;
pub use regalloc::RegMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LirError {
    #[error("mir error: {0}")]
    Mir(#[from] luau_mir::MirError),
    #[error("internal: {0}")]
    Internal(String),
}
