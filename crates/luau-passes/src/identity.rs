//! No-op pass. Useful as a smoke test that the pass scheduler works.

use crate::Pass;
use luau_mir::MirProgram;
use rand_chacha::ChaCha20Rng;

pub struct Identity;

impl Pass for Identity {
    fn name(&self) -> &'static str { "identity" }
    fn run(&self, _program: &mut MirProgram, _rng: &mut ChaCha20Rng) {
        // intentionally empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn identity_runs_without_panic() {
        let ast = luau_parse::parse("return 1").unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        let mut mir = luau_mir::lower::lower(&hir).unwrap();
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let n_funcs = mir.functions.len();
        Identity.run(&mut mir, &mut rng);
        assert_eq!(mir.functions.len(), n_funcs);
    }
}
