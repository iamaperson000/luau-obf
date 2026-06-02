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

#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::lower::lower as hir_lower;
    use luau_mir::lower::lower as mir_lower;
    use luau_lir::lower::lower as lir_lower;

    fn compile_to_luau(src: &str) -> String {
        let ast = luau_parse::parse(src).unwrap();
        let hir = hir_lower(&ast).unwrap();
        let mir = mir_lower(&hir).unwrap();
        let lir = lir_lower(&mir).unwrap();
        emit(&lir, [0u8; 32]).unwrap()
    }

    #[test]
    fn empty_source_emits_a_valid_chunk() {
        let chunk = compile_to_luau("");
        assert!(chunk.contains("vm_call"));
        assert!(chunk.contains("OP_Return"));
    }

    #[test]
    fn print_one_emits_call_and_get_global() {
        let chunk = compile_to_luau("print(1)");
        assert!(chunk.contains("OP_Call"));
        assert!(chunk.contains("OP_GetGlobal"));
    }
}
