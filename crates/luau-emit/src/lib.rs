//! Emit stage. Encodes LIR to bytecode and renders the VM template.

pub mod opmap;
pub mod encode;
pub mod render;

use luau_lir::LirProgram;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("template error: {0}")]
    Template(String),
}

/// Top-level emit. Produces the final Luau chunk source.
pub fn emit(program: &LirProgram, seed: [u8; 32]) -> Result<String, EmitError> {
    let opmap = opmap::OpMap::new(&seed);
    let mut rng = ChaCha20Rng::from_seed(seed);
    render::render(program, &opmap, &mut rng)
}
