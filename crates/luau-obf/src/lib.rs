//! Top-level facade. One function: obfuscate.

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 32-byte seed. If None, a random seed is generated and surfaced via `seed_used`.
    pub seed: Option<[u8; 32]>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse: {0}")]
    Parse(#[from] luau_parse::ParseError),
    #[error("hir: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("mir: {0}")]
    Mir(#[from] luau_mir::MirError),
    #[error("lir: {0}")]
    Lir(#[from] luau_lir::LirError),
    #[error("emit: {0}")]
    Emit(#[from] luau_emit::EmitError),
}

pub struct ObfuscateResult {
    /// The obfuscated Luau chunk.
    pub output: String,
    /// The seed actually used (whether user-provided or generated).
    pub seed_used: [u8; 32],
}

pub fn obfuscate(source: &str, opts: Options) -> Result<ObfuscateResult, Error> {
    let seed = opts.seed.unwrap_or_else(random_seed);
    let mut rng = ChaCha20Rng::from_seed(seed);

    let ast = luau_parse::parse(source)?;
    let hir = luau_hir::lower::lower(&ast)?;
    let mut mir = luau_mir::lower::lower(&hir)?;
    let plan = luau_passes::default_plan();
    plan.run(&mut mir, &mut rng);
    let mut lir = luau_lir::lower::lower(&mir)?;
    luau_lir::shuffle::shuffle_constants(&mut lir, &mut rng);
    let output = luau_emit::emit(&lir, &mut rng)?;
    Ok(ObfuscateResult { output, seed_used: seed })
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscate_empty() {
        let r = obfuscate("", Options::default()).unwrap();
        assert!(r.output.contains("vm_call"));
        assert_eq!(r.seed_used.len(), 32);
    }

    #[test]
    fn obfuscate_simple_print() {
        let r = obfuscate("print(1)", Options { seed: Some([1u8; 32]) }).unwrap();
        assert_eq!(r.seed_used, [1u8; 32]);
        assert!(r.output.contains("OP_GetGlobal"));
    }

    #[test]
    fn deterministic_with_fixed_seed() {
        let a = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        let b = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        assert_eq!(a.output, b.output);
    }

    #[test]
    fn different_seeds_produce_different_outputs() {
        let src = "local x = 1 + 2 print(x)";
        let a = obfuscate(src, Options { seed: Some([1u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([2u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }

    #[test]
    fn three_seeds_produce_three_distinct_outputs() {
        let src = "local function add(a, b) return a + b end print(add(3, 4))";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn output_does_not_contain_plaintext_global_name() {
        // Cardinal test: "print" must NOT appear in the obfuscated output.
        // Compile a program that uses print; the output should contain _enc(...)
        // and NOT the literal string "print" inside a Luau string literal.
        let r = obfuscate("print(\"hello\")", Options { seed: Some([55u8; 32]) }).unwrap();
        // We allow the substring "print" to appear in things like comments or
        // template scaffolding; what we forbid is a Luau string literal
        // containing the word print. Check that no `"print"` substring exists.
        assert!(!r.output.contains("\"print\""), "found plaintext \"print\" in output");
        assert!(!r.output.contains("'print'"), "found plaintext 'print' in output");
        // Sanity: confirm the encrypted-string marker IS present.
        assert!(r.output.contains("_enc("), "expected _enc(...) marker in output");
    }

    #[test]
    fn different_seeds_change_encrypted_byte_shape() {
        // Same source, two different seeds → the encrypted byte sequences
        // for "print" are different (because the keys are different).
        let a = obfuscate("print(1)", Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate("print(1)", Options { seed: Some([20u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }
}
