//! Constant-Decomposition pass — rewrites a fraction of `LoadConst` of
//! finite, small-magnitude `Constant::Number(n)` instructions as
//! `LoadConst(n + δ); LoadConst(δ); Sub`, where δ is a seed-derived
//! integer in [1, 65535]. The original literal disappears from the
//! constant pool, replaced by two seed-derived constants whose runtime
//! difference reproduces it. Bit-exact for finite `|n| < 2^31`.

use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct ConstantDecompose;

const REWRITE_PERCENT: u8 = 30;
const DELTA_MIN: u32 = 1;
const DELTA_MAX: u32 = 65535;
const ABS_MAX_N: f64 = (1u64 << 31) as f64; // 2^31

fn n_safe_to_decompose(n: f64) -> bool {
    n.is_finite() && n.abs() < ABS_MAX_N
}

impl Pass for ConstantDecompose {
    fn name(&self) -> &'static str {
        "constant-decompose"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_load = matches!(&block.instrs[i], Instr::LoadConst { .. });
                    if !is_load {
                        i += 1;
                        continue;
                    }
                    // Always draw the u8 for deterministic RNG advancement.
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, src_id) = match &block.instrs[i] {
                        Instr::LoadConst { dst, src } => (*dst, *src),
                        _ => unreachable!(),
                    };
                    let n = match consts.get(src_id.0 as usize) {
                        Some(Constant::Number(n)) => *n,
                        _ => {
                            i += 1;
                            continue;
                        }
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    if !want_rewrite || !n_safe_to_decompose(n) {
                        i += 1;
                        continue;
                    }
                    let delta = rng.gen_range(DELTA_MIN..=DELTA_MAX) as f64;
                    let k1 = n + delta;
                    let k2 = delta;
                    let c1_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k1));
                    let c2_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k2));
                    let tmp_a = VLocal(n_locals);
                    let tmp_b = VLocal(n_locals + 1);
                    n_locals += 2;
                    block.instrs[i] = Instr::LoadConst { dst: tmp_a, src: c1_id };
                    block.instrs.insert(
                        i + 1,
                        Instr::LoadConst { dst: tmp_b, src: c2_id },
                    );
                    block.instrs.insert(
                        i + 2,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Sub,
                            lhs: Value::VLocal(tmp_a),
                            rhs: Value::VLocal(tmp_b),
                        },
                    );
                    i += 3;
                }
            }
            f.n_locals = n_locals;
            f.consts = consts;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        luau_mir::lower::lower(&hir).unwrap()
    }

    fn count_subs(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::BinOp { op: BinOp::Sub, .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn count_loadconsts(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if matches!(i, Instr::LoadConst { .. }) {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    fn count_number_consts(p: &MirProgram) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for c in &f.consts {
                if matches!(c, Constant::Number(_)) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn rewrites_some_numeric_loadconsts() {
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a, b, c, d, e, 6, 7, 8, 9, 10, 11, 12)";
        let p = mir_of(src);
        let lc_before = count_loadconsts(&p);
        let subs_before = count_subs(&p);
        let nums_before = count_number_consts(&p);
        assert!(lc_before >= 10, "expected ≥10 LoadConsts, got {}", lc_before);

        let mut saw_rewrite = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ConstantDecompose.run(&mut p_clone, &mut rng);
            let lc_after = count_loadconsts(&p_clone);
            let subs_after = count_subs(&p_clone);
            let nums_after = count_number_consts(&p_clone);
            // Each rewrite: -1 LoadConst replaced + 2 new = net +1 LoadConst,
            //               +1 new Sub, +2 new Number consts.
            let lc_growth = lc_after as i64 - lc_before as i64;
            let subs_growth = subs_after as i64 - subs_before as i64;
            let nums_growth = nums_after as i64 - nums_before as i64;
            assert_eq!(lc_growth, subs_growth, "seed {}: lc growth ({}) != subs growth ({})", s, lc_growth, subs_growth);
            assert_eq!(nums_growth, 2 * subs_growth, "seed {}: nums growth ({}) != 2 * subs ({})", s, nums_growth, subs_growth);
            if subs_growth > 0 {
                saw_rewrite = true;
            }
        }
        assert!(saw_rewrite, "no seed in 0..5 rewrote a LoadConst");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "print(1, 2, 3, 4, 5, 6, 7, 8)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        ConstantDecompose.run(&mut p1, &mut r1);
        ConstantDecompose.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                }
                for c in &f.consts {
                    s.push_str(&format!("c: {:?}\n", c));
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn skips_non_numeric_constants() {
        let src = "print('hi', true, nil)";
        let mut p = mir_of(src);
        let subs_before = count_subs(&p);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ConstantDecompose.run(&mut p, &mut rng);
        assert_eq!(count_subs(&p), subs_before, "non-numeric LoadConsts should not be decomposed");
    }

    #[test]
    fn skips_out_of_range_numbers() {
        // 2^32 is finite but outside our safety range.
        let big_n: f64 = (1u64 << 32) as f64;
        let src = format!("print({})", big_n);
        let mut p = mir_of(&src);
        let subs_before = count_subs(&p);
        // Run with a seed that would otherwise decompose small numbers.
        for s in 0..20u8 {
            let mut p_clone = mir_of(&src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            ConstantDecompose.run(&mut p_clone, &mut rng);
            assert_eq!(
                count_subs(&p_clone),
                subs_before,
                "seed {}: out-of-range number was decomposed",
                s
            );
        }
    }

    #[test]
    fn programs_with_no_loadconst_are_unchanged() {
        // A program with no constants at all is rare; use one with only string
        // constants — Number ones are absent.
        let src = "local function f() end f()";
        let mut p = mir_of(src);
        let nlocals_before = p.functions[0].n_locals;
        let subs_before = count_subs(&p);
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        ConstantDecompose.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, nlocals_before);
        assert_eq!(count_subs(&p), subs_before);
    }
}
