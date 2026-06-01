//! MIR → LIR lowering. One MIR function becomes one LIR function (proto).

use crate::{
    BlockLabel, ConstIdx, LirError, LirFunction, LirInstr, LirProgram, OpKind, Operand, ProtoId,
    Reg, RegMap,
};
use luau_hir::{BinOp, UnOp};
use luau_mir::{
    BlockId, Instr as MInstr, MirFunction, MirProgram, Terminator, Value as MValue,
};

pub fn lower(mir: &MirProgram) -> Result<LirProgram, LirError> {
    let mut functions = Vec::with_capacity(mir.functions.len());
    for f in &mir.functions {
        functions.push(lower_function(f)?);
    }
    Ok(LirProgram { functions })
}

fn lower_function(f: &MirFunction) -> Result<LirFunction, LirError> {
    let rm = RegMap::assign_all(f);
    let mut instrs: Vec<LirInstr> = Vec::new();
    let mut label_positions: Vec<(BlockLabel, u32)> = Vec::new();

    let label_of = |b: BlockId| BlockLabel(b.0);

    // Compute the maximum number of args across all Call instructions so we can
    // allocate a scratch window past all VLocal registers.
    let max_call_args: u16 = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(|instr| {
            if let MInstr::Call { args, .. } = instr { Some(args.len() as u16) } else { None }
        })
        .max()
        .unwrap_or(0);

    // scratch_base is the first register beyond all VLocal registers.
    // The scratch window occupies scratch_base (callee) + scratch_base+1..N (args).
    let scratch_base: u16 = rm.num_regs();
    let final_num_regs: u16 = if max_call_args > 0 || f.blocks.iter().flat_map(|b| &b.instrs).any(|i| matches!(i, MInstr::Call { .. })) {
        scratch_base + 1 + max_call_args
    } else {
        rm.num_regs()
    };

    for (idx, block) in f.blocks.iter().enumerate() {
        label_positions.push((label_of(block.id), instrs.len() as u32));

        for instr in &block.instrs {
            lower_instr(instr, &rm, scratch_base, &mut instrs);
        }

        let next_block = f.blocks.get(idx + 1).map(|b| b.id);
        lower_terminator(&block.terminator, &rm, &mut instrs, next_block, label_of);
    }

    Ok(LirFunction {
        id: ProtoId(f.id.0 as u16),
        num_params: f.params.len() as u16,
        num_regs: final_num_regs,
        consts: f.consts.clone(),
        instrs,
        label_positions,
    })
}

fn val_to_reg(v: MValue, rm: &RegMap) -> Reg {
    match v {
        MValue::VLocal(l) => Reg(rm.get(l)),
        MValue::Const(_) => {
            panic!("MIR Value::Const reached LIR lowering — should have been LoadConst'd");
        }
    }
}

fn lower_instr(instr: &MInstr, rm: &RegMap, scratch_base: u16, out: &mut Vec<LirInstr>) {
    match instr {
        MInstr::LoadConst { dst, src } => {
            out.push(LirInstr {
                op: OpKind::LoadConst,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Const(ConstIdx(src.0 as u16)),
                ],
            });
        }
        MInstr::Move { dst, src } => {
            out.push(LirInstr {
                op: OpKind::Move,
                operands: vec![Operand::Reg(Reg(rm.get(*dst))), Operand::Reg(Reg(rm.get(*src)))],
            });
        }
        MInstr::BinOp { dst, op, lhs, rhs } => {
            let (kind, swap, invert) = lower_binop(*op);
            let lhs_r = val_to_reg(*lhs, rm);
            let rhs_r = val_to_reg(*rhs, rm);
            let dst_r = Reg(rm.get(*dst));
            let (a, b) = if swap { (rhs_r, lhs_r) } else { (lhs_r, rhs_r) };
            out.push(LirInstr {
                op: kind,
                operands: vec![Operand::Reg(dst_r), Operand::Reg(a), Operand::Reg(b)],
            });
            if invert {
                out.push(LirInstr {
                    op: OpKind::Not,
                    operands: vec![Operand::Reg(dst_r), Operand::Reg(dst_r)],
                });
            }
        }
        MInstr::UnOp { dst, op, operand } => {
            let r = val_to_reg(*operand, rm);
            let kind = match op {
                UnOp::Neg => OpKind::Neg,
                UnOp::Not => OpKind::Not,
                UnOp::Len => OpKind::Len,
            };
            out.push(LirInstr {
                op: kind,
                operands: vec![Operand::Reg(Reg(rm.get(*dst))), Operand::Reg(r)],
            });
        }
        MInstr::GetGlobal { dst, name } => {
            out.push(LirInstr {
                op: OpKind::GetGlobal,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Const(ConstIdx(name.0 as u16)),
                ],
            });
        }
        MInstr::SetGlobal { name, value } => {
            let r = val_to_reg(*value, rm);
            out.push(LirInstr {
                op: OpKind::SetGlobal,
                operands: vec![Operand::Const(ConstIdx(name.0 as u16)), Operand::Reg(r)],
            });
        }
        MInstr::Call { dst, callee, args } => {
            let callee_src = val_to_reg(*callee, rm);
            let n = args.len() as u16;
            // Move callee into the scratch window (scratch_base), unless it's
            // already there (won't happen since scratch_base >= rm.num_regs()).
            let scratch_callee = Reg(scratch_base);
            if scratch_callee != callee_src {
                out.push(LirInstr {
                    op: OpKind::Move,
                    operands: vec![Operand::Reg(scratch_callee), Operand::Reg(callee_src)],
                });
            }
            // Move each arg into scratch_base + 1 + i, past all live VLocals.
            for (i, a) in args.iter().enumerate() {
                let src = val_to_reg(*a, rm);
                let target = Reg(scratch_base + 1 + i as u16);
                if target != src {
                    out.push(LirInstr {
                        op: OpKind::Move,
                        operands: vec![Operand::Reg(target), Operand::Reg(src)],
                    });
                }
            }
            let dst_reg = match dst {
                Some(d) => Reg(rm.get(*d)),
                None => Reg(0xFFFF),
            };
            out.push(LirInstr {
                op: OpKind::Call,
                operands: vec![
                    Operand::Reg(dst_reg),
                    Operand::Reg(scratch_callee),
                    Operand::SmallInt(n as i16),
                ],
            });
        }
        MInstr::MakeClosure { dst, function, .. } => {
            // Task 5 will use the upvalues field. For now, emit a Closure with
            // no upvalue operands (Plan 2 behavior).
            out.push(LirInstr {
                op: OpKind::Closure,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Proto(ProtoId(function.0 as u16)),
                ],
            });
        }
        MInstr::GetUpval { .. } | MInstr::SetUpval { .. } => {
            // Task 5 will lower upvalue instructions to LIR opcodes.
            // Unimplemented for now — functions with upvalues won't reach this path
            // in the current test corpus.
            todo!("LIR lowering for GetUpval/SetUpval — Task 5");
        }
        MInstr::NewTable { dst } => {
            out.push(LirInstr {
                op: OpKind::NewTable,
                operands: vec![Operand::Reg(Reg(rm.get(*dst)))],
            });
        }
        MInstr::GetIndex { dst, obj, key } => {
            let obj_r = val_to_reg(*obj, rm);
            let key_r = val_to_reg(*key, rm);
            out.push(LirInstr {
                op: OpKind::GetTable,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Reg(obj_r),
                    Operand::Reg(key_r),
                ],
            });
        }
        MInstr::SetIndex { obj, key, value } => {
            let obj_r = val_to_reg(*obj, rm);
            let key_r = val_to_reg(*key, rm);
            let val_r = val_to_reg(*value, rm);
            out.push(LirInstr {
                op: OpKind::SetTable,
                operands: vec![Operand::Reg(obj_r), Operand::Reg(key_r), Operand::Reg(val_r)],
            });
        }
    }
}

fn lower_binop(op: BinOp) -> (OpKind, bool, bool) {
    match op {
        BinOp::Add => (OpKind::Add, false, false),
        BinOp::Sub => (OpKind::Sub, false, false),
        BinOp::Mul => (OpKind::Mul, false, false),
        BinOp::Div => (OpKind::Div, false, false),
        BinOp::Mod => (OpKind::Mod, false, false),
        BinOp::Pow => (OpKind::Pow, false, false),
        BinOp::Concat => (OpKind::Concat, false, false),
        BinOp::Eq => (OpKind::Eq, false, false),
        BinOp::Ne => (OpKind::Eq, false, true),
        BinOp::Lt => (OpKind::Lt, false, false),
        BinOp::Le => (OpKind::Le, false, false),
        BinOp::Gt => (OpKind::Lt, true, false),
        BinOp::Ge => (OpKind::Le, true, false),
        BinOp::And | BinOp::Or => {
            unreachable!("short-circuit binops should be CFG-lowered before LIR");
        }
    }
}

fn lower_terminator(
    t: &Terminator,
    rm: &RegMap,
    out: &mut Vec<LirInstr>,
    next_block: Option<BlockId>,
    label_of: impl Fn(BlockId) -> BlockLabel,
) {
    match t {
        Terminator::Goto(target) => {
            if Some(*target) != next_block {
                out.push(LirInstr {
                    op: OpKind::Jmp,
                    operands: vec![Operand::JmpTarget(label_of(*target))],
                });
            }
        }
        Terminator::Branch { cond, then_block, else_block } => {
            let r = match cond {
                MValue::VLocal(l) => Reg(rm.get(*l)),
                MValue::Const(_) => panic!("branch on Const — should be LoadConst'd"),
            };
            if Some(*else_block) == next_block {
                out.push(LirInstr {
                    op: OpKind::JmpIfTrue,
                    operands: vec![Operand::Reg(r), Operand::JmpTarget(label_of(*then_block))],
                });
            } else {
                out.push(LirInstr {
                    op: OpKind::JmpIfFalse,
                    operands: vec![Operand::Reg(r), Operand::JmpTarget(label_of(*else_block))],
                });
                if Some(*then_block) != next_block {
                    out.push(LirInstr {
                        op: OpKind::Jmp,
                        operands: vec![Operand::JmpTarget(label_of(*then_block))],
                    });
                }
            }
        }
        Terminator::Return(Some(v)) => {
            let r = match v {
                MValue::VLocal(l) => Reg(rm.get(*l)),
                MValue::Const(_) => panic!("return Const"),
            };
            out.push(LirInstr {
                op: OpKind::Return,
                operands: vec![Operand::Reg(r)],
            });
        }
        Terminator::Return(None) => {
            out.push(LirInstr {
                op: OpKind::Return,
                operands: vec![Operand::Reg(Reg(0xFFFF))],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::lower::lower as mir_lower;

    fn lir_of(src: &str) -> LirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        let mir = mir_lower(&hir).unwrap();
        lower(&mir).unwrap()
    }

    #[test]
    fn empty_program_produces_one_function() {
        let p = lir_of("");
        assert_eq!(p.functions.len(), 1);
        let f = &p.functions[0];
        assert!(matches!(f.instrs.last(), Some(LirInstr { op: OpKind::Return, .. })));
    }

    #[test]
    fn local_add_global_print_lowers() {
        let p = lir_of("local x = 1 + 2 print(x)");
        let f = &p.functions[0];
        let ops: Vec<OpKind> = f.instrs.iter().map(|i| i.op).collect();
        assert!(ops.contains(&OpKind::LoadConst));
        assert!(ops.contains(&OpKind::Add));
        assert!(ops.contains(&OpKind::GetGlobal));
        assert!(ops.contains(&OpKind::Call));
        assert!(ops.contains(&OpKind::Return));
    }

    #[test]
    fn function_decl_produces_closure_op() {
        let p = lir_of("function f(x) return x end");
        let main = &p.functions[0];
        let ops: Vec<OpKind> = main.instrs.iter().map(|i| i.op).collect();
        assert!(ops.contains(&OpKind::Closure));
        assert!(ops.contains(&OpKind::SetGlobal));
        assert!(p.functions.len() == 2);
        assert!(matches!(p.functions[1].instrs.last(), Some(LirInstr { op: OpKind::Return, .. })));
    }

    #[test]
    fn call_through_local_ref_does_not_clobber_locals() {
        let src = r#"
function add(a, b) return a + b end
local fn_ref = add
local x = 5
local y = 3
local r = fn_ref(y, x)
print(r)
print(x)
print(y)
"#;
        let p = lir_of(src);
        let main = &p.functions[0];
        // The function's num_regs must exceed the highest VLocal register
        // used for locals (fn_ref, x, y), because scratch goes beyond.
        assert!(main.num_regs > 3, "expected scratch zone past locals, got num_regs={}", main.num_regs);
    }

    #[test]
    fn table_program_lowers_to_table_opcodes() {
        let p = lir_of("local t = {1, 2} t.x = 3 print(t[1] + t.x)");
        let main = &p.functions[0];
        let ops: Vec<OpKind> = main.instrs.iter().map(|i| i.op).collect();
        assert!(ops.contains(&OpKind::NewTable));
        assert!(ops.contains(&OpKind::SetTable));
        assert!(ops.contains(&OpKind::GetTable));
        assert!(ops.contains(&OpKind::Call));
    }
}
