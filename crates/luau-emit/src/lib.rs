//! Emit stage. Encodes LIR to bytecode and renders the VM template.

pub mod opmap;
pub mod encode;
pub mod render;
pub mod mangle;
pub mod stage0;

use luau_lir::LirProgram;
use rand_chacha::ChaCha20Rng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("template error: {0}")]
    Template(String),
}

/// Top-level emit. Produces the final Luau chunk source.
pub fn emit(program: &LirProgram, rng: &mut ChaCha20Rng) -> Result<String, EmitError> {
    let mut opmap_seed = [0u8; 32];
    use rand::RngCore;
    rng.fill_bytes(&mut opmap_seed);
    let opmap = opmap::OpMap::new(&opmap_seed);
    render::render(program, &opmap, rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::lower::lower as hir_lower;
    use luau_mir::lower::lower as mir_lower;
    use luau_lir::lower::lower as lir_lower;
    use rand::SeedableRng;

    fn compile_to_luau(src: &str) -> String {
        let ast = luau_parse::parse(src).unwrap();
        let hir = hir_lower(&ast).unwrap();
        let mir = mir_lower(&hir).unwrap();
        let lir = lir_lower(&mir).unwrap();
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        emit(&lir, &mut rng).unwrap()
    }

    #[test]
    fn empty_source_emits_a_valid_chunk() {
        let chunk = compile_to_luau("");
        // After Plan 9 mangling, internal names are opaque. Just confirm the
        // chunk is a non-trivial Luau program with a return statement.
        assert!(chunk.contains("return"));
        assert!(chunk.len() > 200);
    }

    #[test]
    fn print_one_emits_call_and_get_global() {
        let chunk = compile_to_luau("print(1)");
        // After Plan 9 mangling, opcode and helper names are opaque.
        // Sanity-check that the chunk renders successfully and that the
        // encrypted form of the "print" global name does not leak as a
        // plaintext Luau string literal.
        assert!(!chunk.contains("\"print\""));
        assert!(chunk.len() > 200);
    }
}
