//! Obfuscation pass registry and scheduler. Plan 1 ships an identity pass only.





use luau_mir::MirProgram;
use rand_chacha::ChaCha20Rng;

pub mod analysis;
pub mod identity;
pub mod expr_mangle;
pub mod add_padding;
pub mod mul_padding;
pub mod branch_polarity;
// but how?
/// A MIR→MIR transform.
pub trait Pass {
    /// Stable short name for the pass — used in PassPlan and logs.
    fn name(&self) -> &'static str;

    // i think this is cool 
    /// Run the pass. Receives a seeded RNG to source any randomness from.
    /// Mutates the program in place.
    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng);

}
// huh...
/// An ordered plan of passes to run. Built by the CLI from flags+seed.
pub struct PassPlan {

    pub passes: Vec<Box<dyn Pass>>,

}





impl PassPlan {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }
    pub fn push(mut self, p: Box<dyn Pass>) -> Self {
        self.passes.push(p);
        self
    }
    pub fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for p in &self.passes {
            p.run(program, rng);
        }
    }
}







impl Default for PassPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// The default plan: identity then expression mangling (Plan 15).
pub fn default_plan() -> PassPlan {
    PassPlan::new()
        .push(Box::new(identity::Identity))
        .push(Box::new(expr_mangle::ExpressionMangle))
        .push(Box::new(add_padding::AddIdentityPadding))
        .push(Box::new(mul_padding::MulOperandScatter))
        .push(Box::new(branch_polarity::BranchPolarityFlip))
}
