//! HIR statement → MIR lowering. Handles control flow desugaring.
// Just a comment. Ignore meee!!!

use crate::{
    lower::FnBuilder, Constant, Instr, MirError, Terminator, Value,
};
use luau_hir::{HirExpr, HirStmt};

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
                    let mut arg_vs = Vec::with_capacity(args.len());
                    for a in args {
                        arg_vs.push(Value::VLocal(b.lower_expr(a)?));
                    }
                    b.emit(Instr::Call {
                        dst: None,
                        callee: Value::VLocal(callee_v),
                        args: arg_vs,
                    });
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
            lower(b, body)?;
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
        HirStmt::Repeat { cond, body } => {
            let body_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(body_block));
            b.switch_to(body_block);
            b.loop_exits.push(exit);
            lower(b, body)?;
            b.loop_exits.pop();
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
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(header);
            let cmp_dst = b.fresh_local();
            // Plan 2: positive step only (default 1). Negative-step `for` loops
            // require a runtime sign check on `step` — deferred to a later plan.
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
            lower(b, body)?;
            b.loop_exits.pop();
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
            let closure_dst = b.fresh_local();
            b.emit(Instr::MakeClosure { dst: closure_dst, function: fid });
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
    }
}
