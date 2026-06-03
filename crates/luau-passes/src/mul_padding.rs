//! Mul Operand-Scatter pass — rewrites a fraction of `Mul` instructions
//! as `(x + k) * y - k * y` with a seed-derived integer k. Both operands
//! must be provably numeric (via [`crate::analysis::numeric::NumericLocals`])
//! to avoid breaking programs that overload `__mul`.

use crate::analysis::numeric::NumericLocals;
use crate::Pass;
use luau_hir::BinOp;
use luau_mir::{ConstId, Constant, Instr, MirProgram, VLocal, Value};
use rand::Rng;
use rand_chacha::ChaCha20Rng;

pub struct MulOperandScatter;

const REWRITE_PERCENT: u8 = 30;
const K_MIN: u32 = 1;
const K_MAX: u32 = 65535;

impl Pass for MulOperandScatter {
    fn name(&self) -> &'static str {
        "mul-operand-scatter"
    }

    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng) {
        for f in &mut program.functions {
            let mut n_locals = f.n_locals;
            let mut consts = std::mem::take(&mut f.consts);
            for block in f.blocks.iter_mut() {
                let mut numeric = NumericLocals::new();
                let mut i = 0;
                while i < block.instrs.len() {
                    let is_mul = matches!(
                        &block.instrs[i],
                        Instr::BinOp { op: BinOp::Mul, .. }
                    );
                    if !is_mul {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
                    let roll = rng.gen::<u8>() % 100;
                    let (dst, x, y) = match &block.instrs[i] {
                        Instr::BinOp { dst, lhs, rhs, .. } => (*dst, *lhs, *rhs),
                        _ => unreachable!(),
                    };
                    let want_rewrite = roll < REWRITE_PERCENT;
                    let operands_numeric = numeric.is_value_numeric(&x, &consts)
                        && numeric.is_value_numeric(&y, &consts);
                    if !want_rewrite || !operands_numeric {
                        numeric.update(&block.instrs[i], &consts);
                        i += 1;
                        continue;
                    }
                    // Rewrite path: dst = Mul(x, y)
                    //   -> kc = LoadConst k
                    //      tmp_a = Add(x, kc)         -- x + k
                    //      tmp_m = Mul(tmp_a, y)      -- (x + k) * y = xy + ky
                    //      tmp_k = Mul(kc, y)          -- k * y
                    //      dst   = Sub(tmp_m, tmp_k)  -- xy + ky - ky = xy
                    let k = rng.gen_range(K_MIN..=K_MAX) as f64;
                    let k_const_id = ConstId(consts.len() as u32);
                    consts.push(Constant::Number(k));
                    let kc = VLocal(n_locals);
                    let tmp_a = VLocal(n_locals + 1);
                    let tmp_m = VLocal(n_locals + 2);
                    let tmp_k = VLocal(n_locals + 3);
                    n_locals += 4;
                    block.instrs[i] = Instr::LoadConst { dst: kc, src: k_const_id };
                    block.instrs.insert(
                        i + 1,
                        Instr::BinOp {
                            dst: tmp_a,
                            op: BinOp::Add,
                            lhs: x,
                            rhs: Value::VLocal(kc),
                        },
                    );
                    block.instrs.insert(
                        i + 2,
                        Instr::BinOp {
                            dst: tmp_m,
                            op: BinOp::Mul,
                            lhs: Value::VLocal(tmp_a),
                            rhs: y,
                        },
                    );
                    block.instrs.insert(
                        i + 3,
                        Instr::BinOp {
                            dst: tmp_k,
                            op: BinOp::Mul,
                            lhs: Value::VLocal(kc),
                            rhs: y,
                        },
                    );
                    block.instrs.insert(
                        i + 4,
                        Instr::BinOp {
                            dst,
                            op: BinOp::Sub,
                            lhs: Value::VLocal(tmp_m),
                            rhs: Value::VLocal(tmp_k),
                        },
                    );
                    numeric.insert(kc.0);
                    numeric.insert(tmp_a.0);
                    numeric.insert(tmp_m.0);
                    numeric.insert(tmp_k.0);
                    numeric.insert(dst.0);
                    i += 5;
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

    fn count_op(p: &MirProgram, target: BinOp) -> usize {
        let mut n = 0;
        for f in &p.functions {
            for b in &f.blocks {
                for i in &b.instrs {
                    if let Instr::BinOp { op, .. } = i {
                        if std::mem::discriminant(op) == std::mem::discriminant(&target) {
                            n += 1;
                        }
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
    fn rewrites_some_muls() {
        let src = "local a, b, c, d, e = 1, 2, 3, 4, 5 \
                   print(a * b, c * d, a * b * c, a * b * c * d, a * b * c * d * e)";
        let p = mir_of(src);
        let muls_before = count_op(&p, BinOp::Mul);
        assert!(muls_before >= 10, "expected ≥10 Muls, got {}", muls_before);

        let mut saw = false;
        for s in 0..5u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p_clone, &mut rng);
            if count_op(&p_clone, BinOp::Mul) > muls_before {
                saw = true;
                break;
            }
        }
        assert!(saw, "no seed 0..5 rewrote any Mul");
    }

    #[test]
    fn deterministic_for_same_seed() {
        let src = "local a = 1 print(a * 2 * 3 * 4 * 5 * 6)";
        let mut p1 = mir_of(src);
        let mut p2 = mir_of(src);
        let mut r1 = ChaCha20Rng::from_seed([42u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([42u8; 32]);
        MulOperandScatter.run(&mut p1, &mut r1);
        MulOperandScatter.run(&mut p2, &mut r2);
        let dump = |p: &MirProgram| {
            let mut s = String::new();
            for f in &p.functions {
                for b in &f.blocks {
                    for i in &b.instrs {
                        s.push_str(&format!("{:?}\n", i));
                    }
                }
            }
            s
        };
        assert_eq!(dump(&p1), dump(&p2));
    }

    #[test]
    fn programs_with_no_mul_are_unchanged() {
        let src = "local a, b = 10, 3 print(a + b, a - b)";
        let mut p = mir_of(src);
        let n_locals_before = p.functions[0].n_locals;
        let consts_before = p.functions[0].consts.len();
        let mut rng = ChaCha20Rng::from_seed([0u8; 32]);
        MulOperandScatter.run(&mut p, &mut rng);
        assert_eq!(p.functions[0].n_locals, n_locals_before);
        assert_eq!(p.functions[0].consts.len(), consts_before);
    }

    #[test]
    fn gate_skips_call_operands() {
        let src = "local function f() return 2 end \
                   local function g() return 3 end \
                   print(f() * g())";
        let p = mir_of(src);
        let muls_before = count_op(&p, BinOp::Mul);
        for s in 0..50u8 {
            let mut p_clone = mir_of(src);
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p_clone, &mut rng);
            assert_eq!(count_op(&p_clone, BinOp::Mul), muls_before,
                "seed {} rewrote a Call-operand Mul", s);
        }
    }

    #[test]
    fn rewrite_growth_invariants() {
        let src = "local x = 1 print(x * 2 * 3 * 4 * 5 * 6 * 7 * 8)";
        for s in 0..20u8 {
            let mut p = mir_of(src);
            let muls_before = count_op(&p, BinOp::Mul);
            let subs_before = count_op(&p, BinOp::Sub);
            let nums_before = count_number_consts(&p);
            let nlocals_before = p.functions[0].n_locals;
            let mut rng = ChaCha20Rng::from_seed([s; 32]);
            MulOperandScatter.run(&mut p, &mut rng);
            let new_subs = count_op(&p, BinOp::Sub) - subs_before;
            // Each rewrite: removes 1 Mul, adds 2 Muls (tmp_m and tmp_k), adds 1 Sub.
            // Net Muls: +1. Net Subs: +1.
            assert_eq!(count_op(&p, BinOp::Mul) - muls_before, new_subs);
            // Each rewrite adds 1 numeric const (k) and 4 VLocals.
            assert_eq!(count_number_consts(&p) - nums_before, new_subs);
            assert_eq!(p.functions[0].n_locals - nlocals_before, (new_subs * 4) as u32);
        }
    }
}
