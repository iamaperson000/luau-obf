//! HIR statement → MIR lowering. Handles control flow desugaring.
// Just a comment. Ignore meee!!!

use crate::{
    lower::FnBuilder, CallMode, Constant, Instr, MirError, Terminator, Value, VLocal,
};
use luau_hir::{AssignTarget, HirExpr, HirStmt, UpvalueSource};

pub(crate) fn lower(b: &mut FnBuilder, stmts: &[HirStmt]) -> Result<(), MirError> {
    for stmt in stmts {
        lower_stmt(b, stmt)?;
    }
    Ok(())
}

fn lower_stmt(b: &mut FnBuilder, stmt: &HirStmt) -> Result<(), MirError> {
    match stmt {
        HirStmt::LocalDecl { symbol, value } => {
            let v = b.lower_expr(value)?;
            let slot = b.local_for(*symbol);
            if slot != v {
                b.emit(Instr::Move { dst: slot, src: v });
            }
            Ok(())
        }
        HirStmt::Assign { target, value } => {
            let v = b.lower_expr(value)?;
            if b.is_global(*target) {
                let name_str = b.name_of(*target);
                let name = b.intern_const(Constant::String(name_str));
                b.emit(Instr::SetGlobal { name, value: Value::VLocal(v) });
            } else {
                let slot = b.local_for(*target);
                if slot != v {
                    b.emit(Instr::Move { dst: slot, src: v });
                }
            }
            Ok(())
        }
        HirStmt::ExprStmt(e) => {
            match e {
                HirExpr::Call { callee, args } => {
                    let callee_v = b.lower_expr(callee)?;
                    let (arg_vs, spread) = b.lower_expr_list(args)?;
                    if spread.is_some() {
                        b.emit(Instr::CallVar {
                            dst: None,
                            callee: Value::VLocal(callee_v),
                            args: arg_vs,
                            spread_tail: spread,
                            mode: CallMode::None,
                        });
                    } else {
                        b.emit(Instr::Call {
                            dst: None,
                            callee: Value::VLocal(callee_v),
                            args: arg_vs,
                        });
                    }
                    Ok(())
                }
                HirExpr::MethodCall { obj, method, args } => {
                    let obj_v = b.lower_expr(obj)?;
                    let key_const = b.intern_const(Constant::String(method.clone()));
                    let key_v = b.fresh_local();
                    b.emit(Instr::LoadConst { dst: key_v, src: key_const });
                    let fn_v = b.fresh_local();
                    b.emit(Instr::GetIndex {
                        dst: fn_v,
                        obj: Value::VLocal(obj_v),
                        key: Value::VLocal(key_v),
                    });
                    let mut arg_vs = Vec::with_capacity(args.len() + 1);
                    arg_vs.push(Value::VLocal(obj_v));
                    let (rest, spread) = b.lower_expr_list(args)?;
                    arg_vs.extend(rest);
                    if spread.is_some() {
                        b.emit(Instr::CallVar {
                            dst: None,
                            callee: Value::VLocal(fn_v),
                            args: arg_vs,
                            spread_tail: spread,
                            mode: CallMode::None,
                        });
                    } else {
                        b.emit(Instr::Call {
                            dst: None,
                            callee: Value::VLocal(fn_v),
                            args: arg_vs,
                        });
                    }
                    Ok(())
                }
                other => {
                    let _ = b.lower_expr(other)?;
                    Ok(())
                }
            }
        }
        HirStmt::If { cond, then_body, else_body } => {
            let cond_v = b.lower_expr(cond)?;
            let then_block = b.new_block();
            let else_block = b.new_block();
            let join_block = b.new_block();
            b.set_terminator(Terminator::Branch {
                cond: Value::VLocal(cond_v),
                then_block,
                else_block,
            });
            b.switch_to(then_block);
            lower(b, then_body)?;
            b.set_terminator(Terminator::Goto(join_block));
            b.switch_to(else_block);
            lower(b, else_body)?;
            b.set_terminator(Terminator::Goto(join_block));
            b.switch_to(join_block);
            Ok(())
        }
        HirStmt::While { cond, body } => {
            let header = b.new_block();
            let body_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(header);
            let cond_v = b.lower_expr(cond)?;
            b.set_terminator(Terminator::Branch {
                cond: Value::VLocal(cond_v),
                then_block: body_block,
                else_block: exit,
            });
            b.switch_to(body_block);
            b.loop_exits.push(exit);
            b.loop_continues.push(header);
            lower(b, body)?;
            b.loop_continues.pop();
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
        HirStmt::Repeat { cond, body } => {
            let body_block = b.new_block();
            let cond_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(body_block));
            b.switch_to(body_block);
            b.loop_exits.push(exit);
            b.loop_continues.push(cond_block);
            lower(b, body)?;
            b.loop_continues.pop();
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(cond_block));
            b.switch_to(cond_block);
            let cond_v = b.lower_expr(cond)?;
            b.set_terminator(Terminator::Branch {
                cond: Value::VLocal(cond_v),
                then_block: exit,
                else_block: body_block,
            });
            b.switch_to(exit);
            Ok(())
        }
        HirStmt::NumericFor { var, start, stop, step, body } => {
            if let Some(n) = constant_step_value(step) {
                if n <= 0.0 {
                    return Err(MirError::Unsupported(format!(
                        "numeric `for` with non-positive step {n} (negative-step loops deferred to a later plan)"
                    )));
                }
            }
            let i_slot = b.local_for(*var);
            let start_v = b.lower_expr(start)?;
            b.emit(Instr::Move { dst: i_slot, src: start_v });
            let stop_v = b.lower_expr(stop)?;
            let step_v = b.lower_expr(step)?;
            let stop_slot = b.fresh_local();
            b.emit(Instr::Move { dst: stop_slot, src: stop_v });
            let step_slot = b.fresh_local();
            b.emit(Instr::Move { dst: step_slot, src: step_v });

            let header = b.new_block();
            let body_block = b.new_block();
            let incr_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(header);
            let cmp_dst = b.fresh_local();
            b.emit(Instr::BinOp {
                dst: cmp_dst,
                op: luau_hir::BinOp::Le,
                lhs: Value::VLocal(i_slot),
                rhs: Value::VLocal(stop_slot),
            });
            b.set_terminator(Terminator::Branch {
                cond: Value::VLocal(cmp_dst),
                then_block: body_block,
                else_block: exit,
            });
            b.switch_to(body_block);
            b.loop_exits.push(exit);
            b.loop_continues.push(incr_block);
            lower(b, body)?;
            b.loop_continues.pop();
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(incr_block));
            b.switch_to(incr_block);
            let new_i = b.fresh_local();
            b.emit(Instr::BinOp {
                dst: new_i,
                op: luau_hir::BinOp::Add,
                lhs: Value::VLocal(i_slot),
                rhs: Value::VLocal(step_slot),
            });
            b.emit(Instr::Move { dst: i_slot, src: new_i });
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
        HirStmt::Return(maybe) => {
            let v = match maybe {
                Some(e) => Some(Value::VLocal(b.lower_expr(e)?)),
                None => None,
            };
            b.set_terminator(Terminator::Return(v));
            // Unreachable sentinel block.
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
        HirStmt::FunctionDecl { name, function } => {
            let fid = b.queue_function(function);
            let upvalues: Vec<crate::MirUpvalSource> = function.upvalues.iter().map(|src| match src {
                UpvalueSource::ParentLocal(sym) => crate::MirUpvalSource::Local(b.local_for(*sym)),
                UpvalueSource::ParentUpval(idx) => crate::MirUpvalSource::ParentUpval(*idx),
            }).collect();
            let closure_dst = b.fresh_local();
            b.emit(Instr::MakeClosure { dst: closure_dst, function: fid, upvalues });
            if b.is_global(*name) {
                let name_str = b.name_of(*name);
                let name_const = b.intern_const(Constant::String(name_str));
                b.emit(Instr::SetGlobal { name: name_const, value: Value::VLocal(closure_dst) });
            } else {
                let slot = b.local_for(*name);
                b.emit(Instr::Move { dst: slot, src: closure_dst });
            }
            Ok(())
        }
        HirStmt::Break => {
            let exit = *b
                .loop_exits
                .last()
                .ok_or_else(|| MirError::Unsupported("break outside loop".into()))?;
            b.set_terminator(Terminator::Goto(exit));
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
        HirStmt::Continue => {
            let target = *b
                .loop_continues
                .last()
                .ok_or_else(|| MirError::Unsupported("continue outside loop".into()))?;
            b.set_terminator(Terminator::Goto(target));
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
        HirStmt::CompoundAssign { .. } => {
            Err(MirError::Unsupported("compound assign (Plan 5 Task 6)".into()))
        }
        HirStmt::IndexAssign { obj, key, value } => {
            let obj_v = b.lower_expr(obj)?;
            let key_v = b.lower_expr(key)?;
            let val_v = b.lower_expr(value)?;
            b.emit(Instr::SetIndex {
                obj: Value::VLocal(obj_v),
                key: Value::VLocal(key_v),
                value: Value::VLocal(val_v),
            });
            Ok(())
        }
        HirStmt::UpvalueAssign { upvalue, value } => {
            let v = b.lower_expr(value)?;
            b.emit(Instr::SetUpval { idx: *upvalue, value: Value::VLocal(v) });
            Ok(())
        }
        HirStmt::ReturnMulti(exprs) => {
            let (values, spread_tail) = b.lower_expr_list(exprs)?;
            let dst = b.fresh_local();
            b.emit(Instr::BuildResults { dst, values, spread_tail });
            b.set_terminator(Terminator::ReturnMulti(Value::VLocal(dst)));
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
        HirStmt::LocalDeclMulti { symbols, exprs } => {
            let tbl = build_results_for_binding(b, exprs)?;
            for (i, sym) in symbols.iter().enumerate() {
                let slot = b.local_for(*sym);
                let idx_const = b.intern_const(Constant::Number((i + 1) as f64));
                let key_v = b.fresh_local();
                b.emit(Instr::LoadConst { dst: key_v, src: idx_const });
                let val_v = b.fresh_local();
                b.emit(Instr::GetIndex {
                    dst: val_v,
                    obj: Value::VLocal(tbl),
                    key: Value::VLocal(key_v),
                });
                b.emit(Instr::Move { dst: slot, src: val_v });
            }
            Ok(())
        }
        HirStmt::AssignMulti { targets, exprs } => {
            // Evaluate LHS index targets BEFORE the RHS — preserves left-to-right
            // evaluation semantics for non-aliasing cases.
            enum PreLhs {
                Symbol(luau_hir::SymbolId),
                Upvalue(u32),
                Index { obj: VLocal, key: VLocal },
            }
            let mut pre: Vec<PreLhs> = Vec::with_capacity(targets.len());
            for t in targets {
                match t {
                    AssignTarget::Symbol(s) => pre.push(PreLhs::Symbol(*s)),
                    AssignTarget::Upvalue(idx) => pre.push(PreLhs::Upvalue(*idx)),
                    AssignTarget::Index { obj, key } => {
                        let o = b.lower_expr(obj)?;
                        let k = b.lower_expr(key)?;
                        pre.push(PreLhs::Index { obj: o, key: k });
                    }
                }
            }
            let tbl = build_results_for_binding(b, exprs)?;
            for (i, lhs) in pre.into_iter().enumerate() {
                let idx_const = b.intern_const(Constant::Number((i + 1) as f64));
                let key_v = b.fresh_local();
                b.emit(Instr::LoadConst { dst: key_v, src: idx_const });
                let val_v = b.fresh_local();
                b.emit(Instr::GetIndex {
                    dst: val_v,
                    obj: Value::VLocal(tbl),
                    key: Value::VLocal(key_v),
                });
                match lhs {
                    PreLhs::Symbol(sym) => {
                        if b.is_global(sym) {
                            let name_str = b.name_of(sym);
                            let name = b.intern_const(Constant::String(name_str));
                            b.emit(Instr::SetGlobal { name, value: Value::VLocal(val_v) });
                        } else {
                            let slot = b.local_for(sym);
                            b.emit(Instr::Move { dst: slot, src: val_v });
                        }
                    }
                    PreLhs::Upvalue(idx) => {
                        b.emit(Instr::SetUpval { idx, value: Value::VLocal(val_v) });
                    }
                    PreLhs::Index { obj, key } => {
                        b.emit(Instr::SetIndex {
                            obj: Value::VLocal(obj),
                            key: Value::VLocal(key),
                            value: Value::VLocal(val_v),
                        });
                    }
                }
            }
            Ok(())
        }
        HirStmt::GenericFor { vars, exprs, body } => {
            // Compute iter/state/ctrl from exprs, applying the "last expr can spread" rule.
            let triple = build_results_for_binding(b, exprs)?;
            let iter = read_idx(b, triple, 1)?;
            let state = read_idx(b, triple, 2)?;
            let ctrl_init = read_idx(b, triple, 3)?;
            let ctrl_slot = b.fresh_local();
            b.emit(Instr::Move { dst: ctrl_slot, src: ctrl_init });

            let header = b.new_block();
            let body_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(header);
            // step = iter(state, ctrl) — multi-result.
            let step_tbl = b.fresh_local();
            b.emit(Instr::CallVar {
                dst: Some(step_tbl),
                callee: Value::VLocal(iter),
                args: vec![Value::VLocal(state), Value::VLocal(ctrl_slot)],
                spread_tail: None,
                mode: CallMode::Multi,
            });
            // ctrl = step[1]; if ctrl == nil break.
            let new_ctrl = read_idx(b, step_tbl, 1)?;
            let nil_const = b.intern_const(Constant::Nil);
            let nil_v = b.fresh_local();
            b.emit(Instr::LoadConst { dst: nil_v, src: nil_const });
            let is_nil = b.fresh_local();
            b.emit(Instr::BinOp {
                dst: is_nil,
                op: luau_hir::BinOp::Eq,
                lhs: Value::VLocal(new_ctrl),
                rhs: Value::VLocal(nil_v),
            });
            let not_nil = b.fresh_local();
            b.emit(Instr::UnOp {
                dst: not_nil,
                op: luau_hir::UnOp::Not,
                operand: Value::VLocal(is_nil),
            });
            b.set_terminator(Terminator::Branch {
                cond: Value::VLocal(not_nil),
                then_block: body_block,
                else_block: exit,
            });
            b.switch_to(body_block);
            b.emit(Instr::Move { dst: ctrl_slot, src: new_ctrl });
            // Bind each loop var = step[i+1] (1-based; step[1] is the new ctrl, which
            // is also the first loop var per the Lua generic-for protocol).
            for (i, var) in vars.iter().enumerate() {
                let val = read_idx(b, step_tbl, (i + 1) as i64)?;
                let slot = b.local_for(*var);
                b.emit(Instr::Move { dst: slot, src: val });
            }
            b.loop_exits.push(exit);
            b.loop_continues.push(header);
            crate::lower_stmts::lower(b, body)?;
            b.loop_continues.pop();
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
    }
}

/// For multi-bind RHS: build a results table from the leading non-spread values
/// plus the trailing spread. Mirrors the "adjust" rule.
fn build_results_for_binding(
    b: &mut FnBuilder,
    exprs: &[HirExpr],
) -> Result<VLocal, MirError> {
    let (values, spread_tail) = b.lower_expr_list(exprs)?;
    let dst = b.fresh_local();
    b.emit(Instr::BuildResults { dst, values, spread_tail });
    Ok(dst)
}

/// Read `tbl[idx]` (with `idx` a positive integer literal) into a fresh VLocal.
fn read_idx(b: &mut FnBuilder, tbl: VLocal, idx: i64) -> Result<VLocal, MirError> {
    let key_const = b.intern_const(Constant::Number(idx as f64));
    let key_v = b.fresh_local();
    b.emit(Instr::LoadConst { dst: key_v, src: key_const });
    let dst = b.fresh_local();
    b.emit(Instr::GetIndex {
        dst,
        obj: Value::VLocal(tbl),
        key: Value::VLocal(key_v),
    });
    Ok(dst)
}

/// Best-effort constant evaluation of a numeric-for step expression. Returns
/// the constant value if it's a literal number or a unary-neg of a literal number;
/// otherwise None.
fn constant_step_value(e: &HirExpr) -> Option<f64> {
    match e {
        HirExpr::Literal(luau_hir::HirLiteral::Number(n)) => Some(*n),
        HirExpr::UnOp(luau_hir::UnOp::Neg, inner) => {
            if let HirExpr::Literal(luau_hir::HirLiteral::Number(n)) = inner.as_ref() {
                Some(-*n)
            } else {
                None
            }
        }
        _ => None,
    }
}
