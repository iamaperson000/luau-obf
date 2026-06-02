//! Renders the VM template to produce the final Luau chunk. Implemented in Task 14.

use crate::opmap::OpMap;
use crate::EmitError;
use luau_lir::LirProgram;
use rand_chacha::ChaCha20Rng;

pub fn render(
    _program: &LirProgram,
    _opmap: &OpMap,
    _rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    todo!("Task 14 implements template rendering")
}
