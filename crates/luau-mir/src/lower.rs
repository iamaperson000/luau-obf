//! HIR → MIR lowering. Builds the CFG and desugars short-circuit operators.

use crate::{
    BasicBlock, BlockId, CallMode, Constant, ConstId, FunctionId, Instr, MirError,
    MirFunction, MirProgram, SymbolMap, Terminator, VLocal, Value,
};
use luau_hir::{
    BinOp, HirExpr, HirFunction, HirLiteral, HirProgram, Symbol, SymbolId, SymbolKind, UpvalueSource,
};

pub(crate) struct FnBuilder<'a> {
    pub(crate) _id: FunctionId,
    pub(crate) next_local: u32,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) next_block: u32,
    pub(crate) current: Option<BlockId>,
    pub(crate) consts: Vec<Constant>,
    pub(crate) symbols: SymbolMap,
    /// Read-only view of all symbols in the HIR program.
    pub(crate) all_symbols: &'a [Symbol],
    /// Queue of nested functions (owned clones) discovered during lowering.
    pub(crate) pending_functions: &'a mut Vec<(FunctionId, HirFunction)>,
    pub(crate) next_function: &'a mut u32,
    pub(crate) params: Vec<VLocal>,
    /// Stack of innermost-first loop-exit blocks. `Break` jumps to the top.
    pub(crate) loop_exits: Vec<BlockId>,
}

impl<'a> FnBuilder<'a> {
    pub(crate) fn new(
        id: FunctionId,
        all_symbols: &'a [Symbol],
        pending_functions: &'a mut Vec<(FunctionId, HirFunction)>,
        next_function: &'a mut u32,
    ) -> Self {
        Self {
            _id: id,
            next_local: 0,
            blocks: Vec::new(),
            next_block: 0,
            current: None,
            consts: Vec::new(),
            symbols: SymbolMap::new(),
            all_symbols,
            pending_functions,
            next_function,
            params: Vec::new(),
            loop_exits: Vec::new(),
        }
    }

    pub(crate) fn fresh_local(&mut self) -> VLocal {
        let v = VLocal(self.next_local);
        self.next_local += 1;
        v
    }

    pub(crate) fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            instrs: Vec::new(),
            terminator: Terminator::Return(None),
        });
        id
    }

    pub(crate) fn switch_to(&mut self, block: BlockId) {
        self.current = Some(block);
    }

    pub(crate) fn emit(&mut self, instr: Instr) {
        let id = self.current.expect("no current block");
        let block = self.blocks.iter_mut().find(|b| b.id == id).expect("block exists");
        block.instrs.push(instr);
    }

    pub(crate) fn set_terminator(&mut self, term: Terminator) {
        let id = self.current.expect("no current block");
        let block = self.blocks.iter_mut().find(|b| b.id == id).expect("block exists");
        block.terminator = term;
    }

    pub(crate) fn intern_const(&mut self, c: Constant) -> ConstId {
        if let Some((i, _)) = self.consts.iter().enumerate().find(|(_, x)| **x == c) {
            return ConstId(i as u32);
        }
        let id = ConstId(self.consts.len() as u32);
        self.consts.push(c);
        id
    }

    pub(crate) fn local_for(&mut self, sym: SymbolId) -> VLocal {
        if let Some(&v) = self.symbols.get(&sym) {
            return v;
        }
        let v = self.fresh_local();
        self.symbols.insert(sym, v);
        v
    }

    pub(crate) fn is_global(&self, sym: SymbolId) -> bool {
        self.all_symbols
            .iter()
            .find(|s| s.id == sym)
            .map(|s| s.kind == SymbolKind::Global)
            .unwrap_or(false)
    }

    pub(crate) fn name_of(&self, sym: SymbolId) -> String {
        // Return an owned String so callers can't get tangled in borrow-of-self issues.
        self.all_symbols
            .iter()
            .find(|s| s.id == sym)
            .expect("symbol")
            .name
            .clone()
    }

    pub(crate) fn queue_function(&mut self, f: &HirFunction) -> FunctionId {
        let id = FunctionId(*self.next_function);
        *self.next_function += 1;
        self.pending_functions.push((id, f.clone()));
        id
    }

    fn lower_literal(&mut self, lit: &HirLiteral) -> ConstId {
        let c = match lit {
            HirLiteral::Nil => Constant::Nil,
            HirLiteral::Bool(b) => Constant::Bool(*b),
            HirLiteral::Number(n) => Constant::Number(*n),
            HirLiteral::String(s) => Constant::String(s.clone()),
        };
        self.intern_const(c)
    }

    pub(crate) fn lower_expr(&mut self, e: &HirExpr) -> Result<VLocal, MirError> {
        match e {
            HirExpr::Literal(lit) => {
                let c = self.lower_literal(lit);
                let dst = self.fresh_local();
                self.emit(Instr::LoadConst { dst, src: c });
                Ok(dst)
            }
            HirExpr::Symbol(sym) => {
                if self.is_global(*sym) {
                    let name = self.name_of(*sym);
                    let name_const = self.intern_const(Constant::String(name));
                    let dst = self.fresh_local();
                    self.emit(Instr::GetGlobal { dst, name: name_const });
                    Ok(dst)
                } else {
                    Ok(self.local_for(*sym))
                }
            }
            HirExpr::BinOp(op, l, r) => {
                if matches!(op, BinOp::And | BinOp::Or) {
                    return self.lower_short_circuit(*op, l, r);
                }
                let lhs = self.lower_expr(l)?;
                let rhs = self.lower_expr(r)?;
                let dst = self.fresh_local();
                self.emit(Instr::BinOp {
                    dst,
                    op: *op,
                    lhs: Value::VLocal(lhs),
                    rhs: Value::VLocal(rhs),
                });
                Ok(dst)
            }
            HirExpr::UnOp(op, e) => {
                let operand = self.lower_expr(e)?;
                let dst = self.fresh_local();
                self.emit(Instr::UnOp { dst, op: *op, operand: Value::VLocal(operand) });
                Ok(dst)
            }
            HirExpr::Call { callee, args } => {
                let callee_v = self.lower_expr(callee)?;
                let (arg_vs, spread) = self.lower_expr_list(args)?;
                let dst = self.fresh_local();
                if spread.is_some() {
                    self.emit(Instr::CallVar {
                        dst: Some(dst),
                        callee: Value::VLocal(callee_v),
                        args: arg_vs,
                        spread_tail: spread,
                        mode: CallMode::Scalar,
                    });
                } else {
                    self.emit(Instr::Call {
                        dst: Some(dst),
                        callee: Value::VLocal(callee_v),
                        args: arg_vs,
                    });
                }
                Ok(dst)
            }
            HirExpr::Table(entries) => self.lower_table(entries),
            HirExpr::Index { obj, key } => {
                let obj_v = self.lower_expr(obj)?;
                let key_v = self.lower_expr(key)?;
                let dst = self.fresh_local();
                self.emit(Instr::GetIndex {
                    dst,
                    obj: Value::VLocal(obj_v),
                    key: Value::VLocal(key_v),
                });
                Ok(dst)
            }
            HirExpr::MethodCall { obj, method, args } => {
                let obj_v = self.lower_expr(obj)?;
                let key_const = self.intern_const(Constant::String(method.clone()));
                let key_v = self.fresh_local();
                self.emit(Instr::LoadConst { dst: key_v, src: key_const });
                let fn_v = self.fresh_local();
                self.emit(Instr::GetIndex {
                    dst: fn_v,
                    obj: Value::VLocal(obj_v),
                    key: Value::VLocal(key_v),
                });
                let mut arg_vs = Vec::with_capacity(args.len() + 1);
                arg_vs.push(Value::VLocal(obj_v));
                let (rest_args, spread) = self.lower_expr_list(args)?;
                arg_vs.extend(rest_args);
                let dst = self.fresh_local();
                if spread.is_some() {
                    self.emit(Instr::CallVar {
                        dst: Some(dst),
                        callee: Value::VLocal(fn_v),
                        args: arg_vs,
                        spread_tail: spread,
                        mode: CallMode::Scalar,
                    });
                } else {
                    self.emit(Instr::Call {
                        dst: Some(dst),
                        callee: Value::VLocal(fn_v),
                        args: arg_vs,
                    });
                }
                Ok(dst)
            }
            HirExpr::Function(f) => {
                let fid = self.queue_function(f);
                // Translate the HIR upvalue sources into MIR sources by mapping
                // ParentLocal(SymbolId) → MirUpvalSource::Local(VLocal in current fn).
                let upvalues: Vec<crate::MirUpvalSource> = f.upvalues.iter().map(|src| match src {
                    UpvalueSource::ParentLocal(sym) => crate::MirUpvalSource::Local(self.local_for(*sym)),
                    UpvalueSource::ParentUpval(idx) => crate::MirUpvalSource::ParentUpval(*idx),
                }).collect();
                let dst = self.fresh_local();
                self.emit(Instr::MakeClosure { dst, function: fid, upvalues });
                Ok(dst)
            }
            HirExpr::Upvalue(idx) => {
                let dst = self.fresh_local();
                self.emit(Instr::GetUpval { dst, idx: *idx });
                Ok(dst)
            }
            HirExpr::Vararg => {
                // Scalar context: take the first vararg. Build the table, then index it.
                // Tail-position contexts go through lower_expr_as_results_table directly
                // and never reach this arm.
                let tbl = self.fresh_local();
                self.emit(Instr::GetVarargs { dst: tbl });
                let one_const = self.intern_const(Constant::Number(1.0));
                let key = self.fresh_local();
                self.emit(Instr::LoadConst { dst: key, src: one_const });
                let dst = self.fresh_local();
                self.emit(Instr::GetIndex {
                    dst,
                    obj: Value::VLocal(tbl),
                    key: Value::VLocal(key),
                });
                Ok(dst)
            }
        }
    }

    /// Lower an HIR expression list (for call args, return values, or assign RHS).
    /// Returns the leading non-spread values and an optional spread-tail VLocal
    /// (a results table) if the final HIR expression can spread.
    pub(crate) fn lower_expr_list(
        &mut self,
        exprs: &[HirExpr],
    ) -> Result<(Vec<Value>, Option<VLocal>), MirError> {
        let mut values = Vec::with_capacity(exprs.len());
        let mut spread: Option<VLocal> = None;
        if exprs.is_empty() {
            return Ok((values, spread));
        }
        let last_idx = exprs.len() - 1;
        for (i, e) in exprs.iter().enumerate() {
            if i == last_idx && expr_can_spread(e) {
                let tail = self.lower_expr_as_results_table(e)?;
                spread = Some(tail);
            } else {
                values.push(Value::VLocal(self.lower_expr(e)?));
            }
        }
        Ok((values, spread))
    }

    /// Lower an HIR expression in "multi-result" position. The result is a
    /// results-table VLocal. Used for the tail of an arg list, return list,
    /// or multi-binding RHS.
    pub(crate) fn lower_expr_as_results_table(
        &mut self,
        e: &HirExpr,
    ) -> Result<VLocal, MirError> {
        match e {
            HirExpr::Call { callee, args } => {
                let callee_v = self.lower_expr(callee)?;
                let (arg_vs, spread) = self.lower_expr_list(args)?;
                let dst = self.fresh_local();
                self.emit(Instr::CallVar {
                    dst: Some(dst),
                    callee: Value::VLocal(callee_v),
                    args: arg_vs,
                    spread_tail: spread,
                    mode: CallMode::Multi,
                });
                Ok(dst)
            }
            HirExpr::MethodCall { obj, method, args } => {
                let obj_v = self.lower_expr(obj)?;
                let key_const = self.intern_const(Constant::String(method.clone()));
                let key_v = self.fresh_local();
                self.emit(Instr::LoadConst { dst: key_v, src: key_const });
                let fn_v = self.fresh_local();
                self.emit(Instr::GetIndex {
                    dst: fn_v,
                    obj: Value::VLocal(obj_v),
                    key: Value::VLocal(key_v),
                });
                let mut arg_vs = Vec::with_capacity(args.len() + 1);
                arg_vs.push(Value::VLocal(obj_v));
                let (rest_args, spread) = self.lower_expr_list(args)?;
                arg_vs.extend(rest_args);
                let dst = self.fresh_local();
                self.emit(Instr::CallVar {
                    dst: Some(dst),
                    callee: Value::VLocal(fn_v),
                    args: arg_vs,
                    spread_tail: spread,
                    mode: CallMode::Multi,
                });
                Ok(dst)
            }
            HirExpr::Vararg => {
                let dst = self.fresh_local();
                self.emit(Instr::GetVarargs { dst });
                Ok(dst)
            }
            // Non-spread expressions: wrap in a 1-element results table.
            other => {
                let v = self.lower_expr(other)?;
                let dst = self.fresh_local();
                self.emit(Instr::BuildResults {
                    dst,
                    values: vec![Value::VLocal(v)],
                    spread_tail: None,
                });
                Ok(dst)
            }
        }
    }

    fn lower_table(&mut self, entries: &[luau_hir::TableEntry]) -> Result<VLocal, MirError> {
        // Detect a trailing spread: the last entry is `Array(<spread-able expr>)`.
        let trailing_spread_idx = match entries.last() {
            Some(luau_hir::TableEntry::Array(e)) if expr_can_spread(e) => {
                Some(entries.len() - 1)
            }
            _ => None,
        };

        let dst = self.fresh_local();
        self.emit(Instr::NewTable { dst });
        let mut array_idx: i64 = 1;
        let end = trailing_spread_idx.unwrap_or(entries.len());
        for entry in &entries[..end] {
            match entry {
                luau_hir::TableEntry::Array(e) => {
                    let v = self.lower_expr(e)?;
                    let key_const = self.intern_const(Constant::Number(array_idx as f64));
                    let key_v = self.fresh_local();
                    self.emit(Instr::LoadConst { dst: key_v, src: key_const });
                    self.emit(Instr::SetIndex {
                        obj: Value::VLocal(dst),
                        key: Value::VLocal(key_v),
                        value: Value::VLocal(v),
                    });
                    array_idx += 1;
                }
                luau_hir::TableEntry::Field(name, e) => {
                    let v = self.lower_expr(e)?;
                    let key_const = self.intern_const(Constant::String(name.clone()));
                    let key_v = self.fresh_local();
                    self.emit(Instr::LoadConst { dst: key_v, src: key_const });
                    self.emit(Instr::SetIndex {
                        obj: Value::VLocal(dst),
                        key: Value::VLocal(key_v),
                        value: Value::VLocal(v),
                    });
                }
                luau_hir::TableEntry::Keyed(k, v) => {
                    let key_v = self.lower_expr(k)?;
                    let val_v = self.lower_expr(v)?;
                    self.emit(Instr::SetIndex {
                        obj: Value::VLocal(dst),
                        key: Value::VLocal(key_v),
                        value: Value::VLocal(val_v),
                    });
                }
            }
        }
        if let Some(idx) = trailing_spread_idx {
            let luau_hir::TableEntry::Array(e) = &entries[idx] else { unreachable!() };
            let spread_tbl = self.lower_expr_as_results_table(e)?;
            // Build a fresh shallow copy of spread_tbl, then index-copy into dst[array_idx..].
            let tmp = self.fresh_local();
            self.emit(Instr::BuildResults {
                dst: tmp,
                values: vec![],
                spread_tail: Some(spread_tbl),
            });
            self.copy_results_into_table(tmp, dst, array_idx)?;
        }
        Ok(dst)
    }

    /// Emit MIR for `for i = 1, src.n do tgt[base_idx + i - 1] = src[i] end`.
    fn copy_results_into_table(
        &mut self,
        src: VLocal,
        tgt: VLocal,
        base_idx: i64,
    ) -> Result<(), MirError> {
        // Read src.n.
        let n_key_const = self.intern_const(Constant::String("n".into()));
        let n_key = self.fresh_local();
        self.emit(Instr::LoadConst { dst: n_key, src: n_key_const });
        let n = self.fresh_local();
        self.emit(Instr::GetIndex {
            dst: n,
            obj: Value::VLocal(src),
            key: Value::VLocal(n_key),
        });
        // i_slot = 1
        let one_const = self.intern_const(Constant::Number(1.0));
        let i_slot = self.fresh_local();
        self.emit(Instr::LoadConst { dst: i_slot, src: one_const });
        // base_const for the offset computation (base_idx - 1, so tgt[base_idx + i - 1])
        let base_const_id = self.intern_const(Constant::Number((base_idx - 1) as f64));
        let base_const_v = self.fresh_local();
        self.emit(Instr::LoadConst { dst: base_const_v, src: base_const_id });
        let step_const = self.intern_const(Constant::Number(1.0));
        let step_v = self.fresh_local();
        self.emit(Instr::LoadConst { dst: step_v, src: step_const });

        let header = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.set_terminator(Terminator::Goto(header));
        self.switch_to(header);
        let cmp = self.fresh_local();
        self.emit(Instr::BinOp {
            dst: cmp,
            op: luau_hir::BinOp::Le,
            lhs: Value::VLocal(i_slot),
            rhs: Value::VLocal(n),
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::VLocal(cmp),
            then_block: body_block,
            else_block: exit,
        });
        self.switch_to(body_block);
        // tgt[base_const_v + i_slot] = src[i_slot]
        let src_val = self.fresh_local();
        self.emit(Instr::GetIndex {
            dst: src_val,
            obj: Value::VLocal(src),
            key: Value::VLocal(i_slot),
        });
        let tgt_key = self.fresh_local();
        self.emit(Instr::BinOp {
            dst: tgt_key,
            op: luau_hir::BinOp::Add,
            lhs: Value::VLocal(base_const_v),
            rhs: Value::VLocal(i_slot),
        });
        self.emit(Instr::SetIndex {
            obj: Value::VLocal(tgt),
            key: Value::VLocal(tgt_key),
            value: Value::VLocal(src_val),
        });
        // i = i + 1
        let new_i = self.fresh_local();
        self.emit(Instr::BinOp {
            dst: new_i,
            op: luau_hir::BinOp::Add,
            lhs: Value::VLocal(i_slot),
            rhs: Value::VLocal(step_v),
        });
        self.emit(Instr::Move { dst: i_slot, src: new_i });
        self.set_terminator(Terminator::Goto(header));
        self.switch_to(exit);
        Ok(())
    }

    fn lower_short_circuit(
        &mut self,
        op: BinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> Result<VLocal, MirError> {
        let result = self.fresh_local();
        let lhs_v = self.lower_expr(lhs)?;
        self.emit(Instr::Move { dst: result, src: lhs_v });
        let eval_rhs = self.new_block();
        let join = self.new_block();
        let (then_b, else_b) = match op {
            BinOp::And => (eval_rhs, join),
            BinOp::Or => (join, eval_rhs),
            _ => unreachable!(),
        };
        self.set_terminator(Terminator::Branch {
            cond: Value::VLocal(lhs_v),
            then_block: then_b,
            else_block: else_b,
        });
        self.switch_to(eval_rhs);
        let rhs_v = self.lower_expr(rhs)?;
        self.emit(Instr::Move { dst: result, src: rhs_v });
        self.set_terminator(Terminator::Goto(join));
        self.switch_to(join);
        Ok(result)
    }
}

pub fn lower(hir: &HirProgram) -> Result<MirProgram, MirError> {
    let mut program_functions: Vec<MirFunction> = Vec::new();
    let mut next_function: u32 = 0;
    let mut pending: Vec<(FunctionId, HirFunction)> = Vec::new();

    // Main chunk: wrap top-level stmts as a parameterless function.
    let main_id = FunctionId(next_function);
    next_function += 1;
    pending.push((main_id, HirFunction { params: Vec::new(), body: hir.main.clone(), upvalues: Vec::new(), is_vararg: false }));

    while let Some((fid, fhir)) = pending.pop() {
        let mut builder =
            FnBuilder::new(fid, &hir.symbols, &mut pending, &mut next_function);
        let entry = builder.new_block();
        builder.switch_to(entry);
        for p in &fhir.params {
            let v = builder.fresh_local();
            builder.symbols.insert(*p, v);
            builder.params.push(v);
        }
        crate::lower_stmts::lower(&mut builder, &fhir.body)?;

        program_functions.push(MirFunction {
            id: fid,
            params: builder.params,
            blocks: builder.blocks,
            consts: builder.consts,
            n_locals: builder.next_local,
            upvalues: fhir.upvalues.clone(),
            is_vararg: fhir.is_vararg,
        });
    }
    program_functions.sort_by_key(|f| f.id.0);
    Ok(MirProgram { functions: program_functions })
}

fn expr_can_spread(e: &HirExpr) -> bool {
    matches!(e, HirExpr::Call { .. } | HirExpr::MethodCall { .. } | HirExpr::Vararg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::lower::lower as hir_lower;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).expect("parse");
        let hir = hir_lower(&ast).expect("hir");
        super::lower(&hir).expect("mir")
    }

    #[test]
    fn empty_program_has_main() {
        let p = mir_of("");
        assert_eq!(p.functions.len(), 1);
        assert_eq!(p.main().blocks.len(), 1);
        assert!(matches!(p.main().blocks[0].terminator, Terminator::Return(None)));
    }

    #[test]
    fn local_decl_emits_load_then_move() {
        let p = mir_of("local x = 1");
        let entry = &p.main().blocks[0];
        assert!(matches!(entry.instrs[0], Instr::LoadConst { .. }));
    }

    #[test]
    fn if_else_creates_three_extra_blocks() {
        let p = mir_of("if x then y = 1 else y = 2 end");
        // entry + then + else + join = 4 blocks.
        assert_eq!(p.main().blocks.len(), 4);
    }

    #[test]
    fn while_creates_header_and_exit() {
        let p = mir_of("while x do y = 1 end");
        // entry + header + body + exit = 4 blocks.
        assert_eq!(p.main().blocks.len(), 4);
        let header = &p.main().blocks[1];
        assert!(matches!(header.terminator, Terminator::Branch { .. }));
    }

    #[test]
    fn function_decl_queues_function() {
        let p = mir_of("function f(x) return x end");
        assert_eq!(p.functions.len(), 2);
        let f = &p.functions[1];
        assert_eq!(f.params.len(), 1);
        assert!(matches!(f.blocks[0].terminator, Terminator::Return(Some(_))));
    }

    #[test]
    fn return_after_unreachable_creates_dead_block() {
        let p = mir_of("function f() return 1 end");
        let f = &p.functions[1];
        assert!(f.blocks.len() >= 2, "expected dead-block sentinel after return");
    }

    #[test]
    fn lowers_empty_table_to_new_table() {
        let p = mir_of("local t = {}");
        let has_new_table = p
            .main()
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::NewTable { .. }));
        assert!(has_new_table);
    }

    #[test]
    fn lowers_table_field_to_set_index() {
        let p = mir_of("local t = {x = 1}");
        let has_set_index = p
            .main()
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::SetIndex { .. }));
        assert!(has_set_index);
    }

    #[test]
    fn lowers_index_read_to_get_index() {
        let p = mir_of("local t = {} local x = t[1]");
        let has_get_index = p
            .main()
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::GetIndex { .. }));
        assert!(has_get_index);
    }

    #[test]
    fn lowers_method_call_to_get_index_plus_call() {
        let p = mir_of("local t = {} t:f()");
        let instrs: Vec<&Instr> =
            p.main().blocks.iter().flat_map(|b| b.instrs.iter()).collect();
        let has_get_index = instrs.iter().any(|i| matches!(i, Instr::GetIndex { .. }));
        let has_call = instrs.iter().any(|i| matches!(i, Instr::Call { .. }));
        assert!(has_get_index && has_call);
    }

    #[test]
    fn lowers_index_assign() {
        let p = mir_of("local t = {} t.x = 1");
        let has_set_index = p
            .main()
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::SetIndex { .. }));
        assert!(has_set_index);
    }

    #[test]
    fn rejects_numeric_for_with_negative_literal_step() {
        let ast = luau_parse::parse("for i = 10, 1, -1 do x = i end").unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        let err = super::lower(&hir).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("non-positive step"), "got: {msg}");
    }

    #[test]
    fn rejects_numeric_for_with_zero_literal_step() {
        let ast = luau_parse::parse("for i = 1, 10, 0 do x = i end").unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        let err = super::lower(&hir).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("non-positive step"), "got: {msg}");
    }

    #[test]
    fn accepts_numeric_for_with_dynamic_step() {
        // Dynamic step (variable) compiles; correctness at runtime is the user's problem.
        let p = mir_of("local s = 1 for i = 1, 10, s do x = i end");
        assert!(!p.main().blocks.is_empty());
    }

    #[test]
    fn closure_capture_emits_get_upval() {
        let p = mir_of("local x = 1 local f = function() return x end");
        let inner = &p.functions[1];
        let has_get_upval = inner.blocks.iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::GetUpval { .. }));
        assert!(has_get_upval);
        assert_eq!(inner.upvalues.len(), 1);
    }

    #[test]
    fn closure_creation_emits_make_closure_with_upvalues() {
        let p = mir_of("local x = 1 local f = function() return x end");
        let main = p.main();
        let mc = main.blocks.iter().flat_map(|b| b.instrs.iter()).find_map(|i| {
            if let Instr::MakeClosure { upvalues, .. } = i { Some(upvalues) } else { None }
        }).expect("found MakeClosure");
        assert_eq!(mc.len(), 1);
    }

    #[test]
    fn upvalue_write_emits_set_upval() {
        let p = mir_of("local x = 1 local f = function() x = 2 end");
        let inner = &p.functions[1];
        let has_set_upval = inner.blocks.iter()
            .flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::SetUpval { .. }));
        assert!(has_set_upval);
    }

    #[test]
    fn break_in_while_jumps_to_exit() {
        let p = mir_of("while true do break end");
        let func = p.main();
        // The function should contain a Branch whose else_block is also targeted by a Goto somewhere.
        let header_else = func
            .blocks
            .iter()
            .find_map(|bb| match &bb.terminator {
                Terminator::Branch { else_block, .. } => Some(*else_block),
                _ => None,
            })
            .expect("found while header");
        let break_goto_exists = func.blocks.iter().any(|bb| {
            matches!(&bb.terminator, Terminator::Goto(target) if *target == header_else)
        });
        assert!(break_goto_exists, "expected a Goto to the loop exit");
    }
}
