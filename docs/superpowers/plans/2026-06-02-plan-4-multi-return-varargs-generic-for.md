# Luau Obfuscator — Plan 4: Multi-Return, Varargs, Generic For

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lift Plan 3's "single-return, no varargs, no generic-for" restrictions by adding the three Lua features that interlock with each other: multi-value returns from calls, the `...` varargs parameter and expression, and the `for k, v in expr_list do … end` generic-for loop. After this plan, programs can return multiple values, declare and forward varargs, and iterate over `pairs(t)` / `ipairs(t)` / custom iterators.

**Architecture:**
- Multi-value plumbing flows through a **results-table** representation: a Luau table `{v1, v2, …, n=count}` that carries an arbitrary number of values plus an explicit count. This avoids fighting Luau's own multi-return semantics on every opcode dispatch.
- HIR gains `HirExpr::Vararg`, `HirFunction.is_vararg`, `HirStmt::GenericFor`, `HirStmt::ReturnMulti`, `HirStmt::LocalDeclMulti`, `HirStmt::AssignMulti`. Single-value `Return`/`LocalDecl`/`Assign` stay as the fast path; the `Multi` variants only appear when there are 2+ targets or a spread tail.
- MIR adds `Instr::CallVar` (call with optional spread-arg and a mode flag for "no result / scalar / results table"), `Instr::BuildResults` (build a results table from explicit values plus optional spread tail), `Instr::GetVarargs` (read frame varargs as a results table), and `Terminator::ReturnMulti(VLocal)` (return the spread of a results table). Scalar `Instr::Call` stays unchanged.
- LIR mirrors MIR: new opcodes `CallVar`, `BuildResults`, `Vararg`, `ReturnMulti`. The existing `Call` opcode is unchanged. The proto meta grows a fourth field, `is_vararg`.
- The VM template gains a frame-level `varargs` table built at entry from extra args, handlers for the four new opcodes, and a `_tpack` helper for capturing multi-return into a results table.

**Tech Stack:** Same as Plans 1-3.

**Plan 4 language additions:**

| Newly supported | Still deferred |
|---|---|
| `return a, b, c` and `return f()` (spread all of f's results) | `continue`, compound assign, string interp (Plan 5) |
| `local a, b = f()`, `a, b = f()` (spread call into multi-bind) | Metatables, `__index`/`__newindex` chasing (Plan 6+) |
| `function f(...)`, `...` in expr/arg/return position, `{...}` collection | Vector / buffer / bit32 builtins (Plan 6+) |
| `for k, v in pairs(t) do … end`, `for i, v in ipairs(t) do … end`, custom iterators with the (iter, state, ctrl) protocol | Negative-step numeric for (still deferred to a later plan) |

**Out-of-scope reminder:** No metatable support — `pairs(t)` and `ipairs(t)` are passed through as ordinary globals; whatever the host Luau VM does, we do. We are not implementing `__pairs` lookups.

---

## Implementation notes

**Results table.** A "results table" is a Luau table with integer keys `1..n` and an `n` field. Created by `_tpack(...)`, by the new `BuildResults` op, or by `GetVarargs`. Consumed by `CallVar`'s spread-tail, `ReturnMulti`, and table-index ops.

**Multi-return capture in the VM.** Every call site that wants multi-results uses `CallVar` with mode = 2. The handler executes `local results = _tpack(fn(_tunpack(call_args, 1, n)))` so `results.n` is the real result count regardless of trailing nils. Scalar callers keep using the existing `Call` opcode, which is faster and stays unchanged.

**Spread tail in arg lists.** A `Call`/`CallVar` carries an optional `spread_tail` register pointing to a results table. The VM appends every entry `1..n` to the actual argument list before invoking the callee. Used for:
- `g(x, f())` — f lowered via `CallVar` (mode 2); g's call gets spread_tail = f's results.
- `g(x, ...)` — varargs lowered via `Vararg`; g's call gets spread_tail = the varargs table.

**Lua "adjust" rule for multi-bind.** In `local a, b, c = x, y` Lua adjusts the RHS to LHS length:
- If RHS shorter than LHS and the last RHS is a call or `...`, the last RHS's multi-results fill the remaining LHS slots; further missing slots get nil.
- If RHS shorter than LHS and the last RHS is *not* a call/vararg, missing slots get nil.
- If RHS equal-or-longer, each RHS is single-valued and bound to its LHS slot positionally; extras are discarded.

The HIR lowerer detects the "last is call/vararg" case and emits `HirStmt::LocalDeclMulti` / `HirStmt::AssignMulti`. The equal-or-longer case is split into N single-bindings (the existing path).

**Generic-for semantics.** `for v1, …, vN in e1, …, eM do body end` is lowered as:

```
iter, state, ctrl = adjust3(e1, …, eM)
while true do
    r = iter(state, ctrl)            -- r is a results table
    nv = r[1]                         -- new control
    if nv == nil then break end
    ctrl = nv
    v1, v2, …, vN = r[1], r[2], …, r[N]
    body
end
```

`adjust3` is the same "last expr can spread" rule above, narrowed to 3 targets. In practice almost all `for-in` loops have `M == 1` and that one expression is a call to `pairs`/`ipairs` whose first three results are exactly iter/state/ctrl.

**Varargs and the frame.** When a function declares `...`, the VM build a `varargs` results table at frame entry from any args past the explicit parameter count. The `Vararg` opcode returns that table; reused across calls within the same frame is safe because no opcode mutates it.

**Status of Plan 3 corpus.** All Plan 1–3 corpus programs use single-return and have no varargs / no `for in`. They must continue to pass unchanged.

---

## File Structure (changes only)

```
crates/luau-hir/src/
  types.rs            # MODIFY: HirExpr::Vararg, HirFunction.is_vararg,
                      #         HirStmt::ReturnMulti/LocalDeclMulti/AssignMulti/GenericFor,
                      #         AssignTarget enum for multi-assign LHS
  lower.rs            # MODIFY: lower multi-return / multi-decl / multi-assign / for-in / ...
crates/luau-mir/src/
  types.rs            # MODIFY: Instr::CallVar/BuildResults/GetVarargs,
                      #         Terminator::ReturnMulti, MirFunction.is_vararg
  lower.rs            # MODIFY: lower Vararg/CallMulti / BuildResults in lower_expr;
                      #         lower_call_args with spread detection
  lower_stmts.rs      # MODIFY: ReturnMulti, LocalDeclMulti, AssignMulti, GenericFor
crates/luau-lir/src/
  types.rs            # MODIFY: OpKind::CallVar/BuildResults/Vararg/ReturnMulti;
                      #         LirFunction.is_vararg
  regalloc.rs         # MODIFY: visit_vlocals for the new instructions
  lower.rs            # MODIFY: lower CallVar/BuildResults/GetVarargs/ReturnMulti;
                      #         num_regs accounting for spread tails
crates/luau-emit/src/
  opmap.rs            # MODIFY: ALL_OPS grows by 4; forward[N] size
  render.rs           # MODIFY: opname() entries; MetaEntry.is_vararg
  encode.rs           # MODIFY: emit BuildResults's variable-length value list
crates/luau-runtime/assets/
  vm.luau.j2          # MODIFY: _tpack helper, frame varargs init, handlers for
                      #         CallVar/BuildResults/Vararg/ReturnMulti, meta grows
runtime-tests/corpus/
  multi_return.luau   # CREATE
  generic_for.luau    # CREATE
  varargs.luau        # CREATE
README.md             # MODIFY: status line
```

---

## Task 1: HIR types — multi-value, varargs, generic-for

**Files:**
- Modify: `crates/luau-hir/src/types.rs`

- [ ] **Step 1: Add `AssignTarget` for multi-assign LHS**

Multi-assign LHS positions are not all `SymbolId` — some are index expressions. Add near `HirExpr`:

```rust
/// One position on the LHS of a multi-assignment.
#[derive(Debug, Clone)]
pub enum AssignTarget {
    /// `name = …` where `name` resolved to a local or global symbol.
    Symbol(SymbolId),
    /// `name = …` where `name` resolved to an upvalue of the current function.
    Upvalue(u32),
    /// `obj[key] = …`.
    Index { obj: HirExpr, key: HirExpr },
}
```

- [ ] **Step 2: Add `HirExpr::Vararg`**

In `HirExpr`, after `Upvalue(u32)`:

```rust
    /// `...` — the current function's varargs as a multi-value expression.
    /// In most positions only the first value is used; in tail position of a
    /// call/return/decl/assign expression list, all values spread.
    Vararg,
```

- [ ] **Step 3: Add `HirFunction.is_vararg`**

Replace `HirFunction`:

```rust
#[derive(Debug, Clone)]
pub struct HirFunction {
    pub params: Vec<SymbolId>,
    pub body: Vec<HirStmt>,
    pub upvalues: Vec<UpvalueSource>,
    /// True if the function declared `...` in its parameter list.
    /// Inside the body, `HirExpr::Vararg` is only valid when this is true.
    pub is_vararg: bool,
}
```

- [ ] **Step 4: Add the multi-value statement variants**

In `HirStmt`, after the existing variants:

```rust
    /// `return e1, e2, …, eN` with N >= 2, OR with N == 1 where the single expression
    /// is a call or `Vararg` that should spread. (Single non-spread returns continue
    /// to use `Return(Some(_))`.)
    ReturnMulti(Vec<HirExpr>),
    /// `local a, b, … = e1, e2, …` with at least one symbol position and at least
    /// one expression. The lowerer only emits this form when the RHS may spread
    /// (last expr is a call or `Vararg`). Equal-length non-spread RHS is lowered
    /// as N single `LocalDecl` statements.
    LocalDeclMulti { symbols: Vec<SymbolId>, exprs: Vec<HirExpr> },
    /// `a, b, … = e1, e2, …`. Same rationale as `LocalDeclMulti`.
    AssignMulti { targets: Vec<AssignTarget>, exprs: Vec<HirExpr> },
    /// `for v1, …, vN in e1, …, eM do body end`. The lowerer enforces N >= 1
    /// and M >= 1 (full_moon should guarantee this at parse time).
    GenericFor {
        vars: Vec<SymbolId>,
        exprs: Vec<HirExpr>,
        body: Vec<HirStmt>,
    },
```

- [ ] **Step 5: Verify build**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo check -p luau-hir`

Expected: compiles. Downstream crates may break on exhaustive matches over `HirStmt` / `HirExpr` — that's expected and fixed in Tasks 5–7.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-hir/src/types.rs
git commit -m "feat(hir): types for multi-return, varargs, generic-for"
```

---

## Task 2: HIR lowering — varargs in declarations and `is_vararg`

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

Plan 3's `lower_function_body_with_self` errored on `Parameter::Ellipsis`. We now accept it and record `is_vararg` on the resulting `HirFunction`.

- [ ] **Step 1: Replace `lower_function_body_with_self`**

Replace the function so trailing `...` becomes a flag rather than an error:

```rust
fn lower_function_body_with_self(
    lowerer: &mut Lowerer,
    body: &full_moon::ast::FunctionBody,
    is_method: bool,
) -> Result<HirFunction, HirError> {
    lowerer.enter_function();
    let mut params = Vec::new();
    if is_method {
        params.push(lowerer.declare_local("self"));
    }
    let mut is_vararg = false;
    let params_list: Vec<&full_moon::ast::Parameter> = body.parameters().iter().collect();
    for (i, p) in params_list.iter().enumerate() {
        use full_moon::ast::Parameter;
        match p {
            Parameter::Name(tok) => {
                params.push(lowerer.declare_local(&tok.token().to_string()));
            }
            Parameter::Ellipsis(_) => {
                if i != params_list.len() - 1 {
                    lowerer.exit_function();
                    return Err(HirError::Unsupported(
                        "`...` must be the last parameter".into(),
                    ));
                }
                is_vararg = true;
            }
            other => {
                lowerer.exit_function();
                return Err(HirError::Unsupported(format!("parameter form {other:?}")));
            }
        }
    }
    let body = lower_block(lowerer, body.block())?;
    let frame = lowerer.exit_function();
    Ok(HirFunction {
        params,
        body,
        upvalues: frame.upvalues,
        is_vararg,
    })
}
```

- [ ] **Step 2: Update every other `HirFunction { … }` construction**

Search for `HirFunction {` in `crates/luau-hir/src/lower.rs`. The only place that constructs `HirFunction` directly is `lower_function_body_with_self` (already updated). The main-chunk construction lives in `crates/luau-mir/src/lower.rs`; we'll touch that in Task 5.

- [ ] **Step 3: Add a test for varargs in parameter list**

In `crates/luau-hir/src/lower.rs`'s `tests` mod, add:

```rust
    #[test]
    fn lowers_vararg_parameter() {
        let p = lower_str("local f = function(...) end");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        let HirExpr::Function(f) = value else { panic!() };
        assert!(f.is_vararg);
        assert!(f.params.is_empty());
    }

    #[test]
    fn lowers_mixed_params_and_vararg() {
        let p = lower_str("local f = function(a, b, ...) end");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        let HirExpr::Function(f) = value else { panic!() };
        assert!(f.is_vararg);
        assert_eq!(f.params.len(), 2);
    }

    #[test]
    fn rejects_vararg_in_middle_of_params() {
        let ast = luau_parse::parse("local f = function(a, ..., b) end").unwrap();
        let err = lower(&ast).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`...` must be the last"), "got: {msg}");
    }
```

- [ ] **Step 4: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-hir`

Expected: the three new tests pass. The Plan 3 test `rejects_anonymous_closure_capturing_local` (if any remained) was already removed; if you find a leftover test expecting `Parameter::Ellipsis(_)` to error, delete it.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): accept `...` parameter, record is_vararg on HirFunction"
```

---

## Task 3: HIR lowering — `...` expression, multi-return, multi-decl, multi-assign

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

- [ ] **Step 1: Lower `...` in expression position**

Find the `lower_expr` function and add a `Symbol` case for `"..."` (full_moon represents `...` as a symbol token in expression context — confirm by checking the parser's enum). The cleanest hook is to extend the existing `E::Symbol(sym)` match:

```rust
        E::Symbol(sym) => {
            let s = sym.token().to_string();
            match s.as_str() {
                "nil" => Ok(HirExpr::Literal(HirLiteral::Nil)),
                "true" => Ok(HirExpr::Literal(HirLiteral::Bool(true))),
                "false" => Ok(HirExpr::Literal(HirLiteral::Bool(false))),
                "..." => Ok(HirExpr::Vararg),
                other => Err(HirError::Unsupported(format!(
                    "symbol expression {other:?}"
                ))),
            }
        }
```

If full_moon exposes `...` through a different variant (e.g., `E::VarArgs(_)` or similar), match that variant instead. The test in Step 8 will fail loudly if you pick the wrong path.

- [ ] **Step 2: Replace `lower_last_stmt` to support multi-return**

The current implementation errors on 2+ return expressions. Replace:

```rust
fn lower_last_stmt(
    lowerer: &mut Lowerer,
    stmt: &full_moon::ast::LastStmt,
) -> Result<HirStmt, HirError> {
    use full_moon::ast::LastStmt;
    match stmt {
        LastStmt::Return(ret) => {
            let exprs: Vec<&full_moon::ast::Expression> = ret.returns().iter().collect();
            match exprs.len() {
                0 => Ok(HirStmt::Return(None)),
                1 => {
                    let e = lower_expr(lowerer, exprs[0])?;
                    if expr_can_spread(&e) {
                        Ok(HirStmt::ReturnMulti(vec![e]))
                    } else {
                        Ok(HirStmt::Return(Some(e)))
                    }
                }
                _ => {
                    let lowered: Result<Vec<HirExpr>, _> = exprs.iter()
                        .map(|e| lower_expr(lowerer, e))
                        .collect();
                    Ok(HirStmt::ReturnMulti(lowered?))
                }
            }
        }
        LastStmt::Break(_) => Ok(HirStmt::Break),
        LastStmt::Continue(_) => Err(HirError::Unsupported("continue (Plan 5)".into())),
        other => Err(HirError::Unsupported(format!("last stmt form {other:?}"))),
    }
}

/// True if the expression, when in tail position of an expression list,
/// can produce more than one value.
fn expr_can_spread(e: &HirExpr) -> bool {
    matches!(e, HirExpr::Call { .. } | HirExpr::MethodCall { .. } | HirExpr::Vararg)
}
```

Note the Plan 3 error message for `continue` mentioned Plan 4 — we now reschedule it to Plan 5.

- [ ] **Step 3: Replace the `LocalAssignment` arm in `lower_stmt`**

The Plan 3 arm errored on multi-decl with mismatched RHS. Replace with the adjust-rule lowering:

```rust
        Stmt::LocalAssignment(la) => {
            let names: Vec<&full_moon::tokenizer::TokenReference> = la.names().iter().collect();
            let exprs: Vec<&full_moon::ast::Expression> = la.expressions().iter().collect();
            // Lower the expressions first (so any names they reference resolve to
            // outer scopes, not the new locals).
            let lowered_exprs: Vec<HirExpr> = exprs.iter()
                .map(|e| lower_expr(lowerer, e))
                .collect::<Result<_, _>>()?;
            // Declare the locals AFTER lowering the RHS.
            let symbols: Vec<SymbolId> = names.iter()
                .map(|t| lowerer.declare_local(&t.token().to_string()))
                .collect();
            Ok(lower_multi_binding_locals(symbols, lowered_exprs))
        }
```

Add the helper:

```rust
/// Lower an N-target, M-expression local declaration applying Lua's adjust rule.
/// Emits either N single `LocalDecl`s (no-spread fast path) or one `LocalDeclMulti`
/// (when the RHS may spread).
fn lower_multi_binding_locals(symbols: Vec<SymbolId>, exprs: Vec<HirExpr>) -> Vec<HirStmt> {
    if exprs.is_empty() {
        // `local a, b, c` — all nil.
        return symbols.into_iter()
            .map(|s| HirStmt::LocalDecl { symbol: s, value: HirExpr::Literal(HirLiteral::Nil) })
            .collect();
    }
    let last_can_spread = expr_can_spread(exprs.last().unwrap());
    let n = symbols.len();
    let m = exprs.len();
    // Lua "adjust" rule: spread fires only when there are fewer RHS than LHS
    // AND the last RHS can produce multiple values. At m == n each RHS gives
    // exactly one value, even if the last one is a call.
    if last_can_spread && m < n {
        return vec![HirStmt::LocalDeclMulti { symbols, exprs }];
    }
    // No spread: positionally bind, padding with nil if m < n, discarding if m > n.
    let mut out = Vec::with_capacity(n);
    let mut it = exprs.into_iter();
    for s in symbols.into_iter() {
        let v = it.next().unwrap_or(HirExpr::Literal(HirLiteral::Nil));
        out.push(HirStmt::LocalDecl { symbol: s, value: v });
    }
    // Remaining exprs are evaluated for side effects then discarded — but our
    // expr lowering already evaluated them above, so this is moot at the HIR
    // level. The MIR layer will emit their instructions before the moves.
    // To preserve evaluation order, we wrap discards as ExprStmts so MIR sees
    // them in source order. But since we lowered them already as values and
    // their side effects are baked into the HIR tree (e.g. nested calls inside
    // the expr), there's nothing to do: emitting LocalDecl with the value
    // already includes the side effects.
    let _ = it; // Remaining exprs are dropped; side effects baked into prior moves.
    out
}
```

- [ ] **Step 4: Replace the `Assignment` arm in `lower_stmt`**

```rust
        Stmt::Assignment(a) => {
            let vars: Vec<&full_moon::ast::Var> = a.variables().iter().collect();
            let exprs: Vec<&full_moon::ast::Expression> = a.expressions().iter().collect();
            let lowered_exprs: Vec<HirExpr> = exprs.iter()
                .map(|e| lower_expr(lowerer, e))
                .collect::<Result<_, _>>()?;
            // Lower each LHS to an `AssignTarget`.
            let mut targets = Vec::with_capacity(vars.len());
            for v in &vars {
                targets.push(lower_assign_lhs(lowerer, v)?);
            }
            let n = targets.len();
            let m = lowered_exprs.len();
            let last_can_spread = lowered_exprs.last().map(expr_can_spread).unwrap_or(false);
            // Lua adjust rule: spread only when m < n (see lower_multi_binding_locals).
            if last_can_spread && m < n {
                return Ok(vec![HirStmt::AssignMulti { targets, exprs: lowered_exprs }]);
            }
            // No spread: positional. Pad with nil; discard extras.
            let mut out = Vec::with_capacity(n);
            let mut it = lowered_exprs.into_iter();
            for t in targets.into_iter() {
                let v = it.next().unwrap_or(HirExpr::Literal(HirLiteral::Nil));
                out.push(assign_one(t, v));
            }
            let _ = it;
            Ok(out)
        }
```

Add the two helpers below `lower_assign_target`:

```rust
/// Lower the LHS of a single assignment slot into an `AssignTarget`.
fn lower_assign_lhs(
    lowerer: &mut Lowerer,
    var: &full_moon::ast::Var,
) -> Result<AssignTarget, HirError> {
    use full_moon::ast::{Index, Prefix, Suffix, Var};
    match var {
        Var::Name(t) => match lowerer.resolve(&t.token().to_string()) {
            Resolved::Symbol(s) => Ok(AssignTarget::Symbol(s)),
            Resolved::Upvalue(idx) => Ok(AssignTarget::Upvalue(idx)),
        },
        Var::Expression(ve) => {
            let suffixes: Vec<&Suffix> = ve.suffixes().collect();
            if suffixes.is_empty() {
                return Err(HirError::Unsupported("empty VarExpression in assign LHS".into()));
            }
            let (last, rest) = suffixes.split_last().unwrap();
            let mut obj = match ve.prefix() {
                Prefix::Name(t) => resolved_to_expr(lowerer.resolve(&t.token().to_string())),
                Prefix::Expression(e) => lower_expr(lowerer, e)?,
                other => return Err(HirError::Unsupported(format!("assign prefix {other:?}"))),
            };
            for s in rest {
                obj = lower_suffix(lowerer, obj, s)?;
            }
            let key = match last {
                Suffix::Index(Index::Brackets { expression, .. }) => lower_expr(lowerer, expression)?,
                Suffix::Index(Index::Dot { name, .. }) => {
                    HirExpr::Literal(HirLiteral::String(name.token().to_string()))
                }
                other => return Err(HirError::Unsupported(format!(
                    "assign LHS last suffix must be index, got {other:?}"
                ))),
            };
            Ok(AssignTarget::Index { obj, key })
        }
        other => Err(HirError::Unsupported(format!("assign target form {other:?}"))),
    }
}

/// Build a single-slot assignment statement from a target and a value.
fn assign_one(target: AssignTarget, value: HirExpr) -> HirStmt {
    match target {
        AssignTarget::Symbol(s) => HirStmt::Assign { target: s, value },
        AssignTarget::Upvalue(idx) => HirStmt::UpvalueAssign { upvalue: idx, value },
        AssignTarget::Index { obj, key } => HirStmt::IndexAssign { obj, key, value },
    }
}
```

Keep the existing `lower_assign_target` function — it's still used elsewhere but you can delete it once you've migrated all callers. Search for `lower_assign_target` and confirm it's no longer referenced after this task; if so, delete it.

- [ ] **Step 5: Add tests**

In the `tests` mod, append:

```rust
    #[test]
    fn lowers_multi_return_two_values() {
        let p = lower_str("function f() return 1, 2 end");
        let HirStmt::FunctionDecl { function, .. } = &p.main[0] else { panic!() };
        match &function.body[0] {
            HirStmt::ReturnMulti(es) => assert_eq!(es.len(), 2),
            other => panic!("expected ReturnMulti, got {other:?}"),
        }
    }

    #[test]
    fn lowers_return_call_as_returnmulti() {
        let p = lower_str("function f() return g() end");
        let HirStmt::FunctionDecl { function, .. } = &p.main[0] else { panic!() };
        match &function.body[0] {
            HirStmt::ReturnMulti(es) => {
                assert_eq!(es.len(), 1);
                assert!(matches!(es[0], HirExpr::Call { .. }));
            }
            other => panic!("expected ReturnMulti for `return g()`, got {other:?}"),
        }
    }

    #[test]
    fn lowers_local_multi_decl_with_call_rhs() {
        let p = lower_str("local a, b = f()");
        match &p.main[0] {
            HirStmt::LocalDeclMulti { symbols, exprs } => {
                assert_eq!(symbols.len(), 2);
                assert_eq!(exprs.len(), 1);
                assert!(matches!(exprs[0], HirExpr::Call { .. }));
            }
            other => panic!("expected LocalDeclMulti, got {other:?}"),
        }
    }

    #[test]
    fn lowers_local_multi_decl_equal_length_as_single_decls() {
        // Equal-length non-spread RHS: lowered as two LocalDecls, not LocalDeclMulti.
        let p = lower_str("local a, b = 1, 2");
        assert_eq!(p.main.len(), 2);
        assert!(matches!(p.main[0], HirStmt::LocalDecl { .. }));
        assert!(matches!(p.main[1], HirStmt::LocalDecl { .. }));
    }

    #[test]
    fn lowers_multi_assign_with_call_rhs() {
        let p = lower_str("a, b = f()");
        match &p.main[0] {
            HirStmt::AssignMulti { targets, exprs } => {
                assert_eq!(targets.len(), 2);
                assert_eq!(exprs.len(), 1);
            }
            other => panic!("expected AssignMulti, got {other:?}"),
        }
    }

    #[test]
    fn lowers_local_decl_short_rhs_pads_with_nil() {
        // Three names, two non-spread exprs: a=1, b=2, c=nil — three LocalDecls.
        let p = lower_str("local a, b, c = 1, 2");
        assert_eq!(p.main.len(), 3);
        let HirStmt::LocalDecl { value, .. } = &p.main[2] else { panic!() };
        assert!(matches!(value, HirExpr::Literal(HirLiteral::Nil)));
    }

    #[test]
    fn lowers_vararg_expression() {
        let p = lower_str("local f = function(...) local t = ... end");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        let HirExpr::Function(f) = value else { panic!() };
        assert!(f.is_vararg);
        let HirStmt::LocalDecl { value: inner, .. } = &f.body[0] else { panic!() };
        assert!(matches!(inner, HirExpr::Vararg));
    }
```

- [ ] **Step 6: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-hir`

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): multi-return, multi-bind, vararg expression lowering"
```

---

## Task 4: HIR lowering — generic for

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

- [ ] **Step 1: Add a `Stmt::GenericFor` arm to `lower_stmt`**

After the `Stmt::NumericFor` arm, add:

```rust
        Stmt::GenericFor(gf) => {
            let exprs_iter: Vec<&full_moon::ast::Expression> = gf.expressions().iter().collect();
            let lowered_exprs: Vec<HirExpr> = exprs_iter.iter()
                .map(|e| lower_expr(lowerer, e))
                .collect::<Result<_, _>>()?;
            lowerer.push_scope();
            let names: Vec<&full_moon::tokenizer::TokenReference> = gf.names().iter().collect();
            let vars: Vec<SymbolId> = names.iter()
                .map(|t| lowerer.declare_local(&t.token().to_string()))
                .collect();
            let body = lower_block(lowerer, gf.block())?;
            lowerer.pop_scope();
            Ok(vec![HirStmt::GenericFor {
                vars,
                exprs: lowered_exprs,
                body,
            }])
        }
```

If full_moon names the variant differently (e.g., `Stmt::ForIn`), adapt — the structural ask is: a list of name tokens, a list of expressions, and a block body.

- [ ] **Step 2: Add tests**

```rust
    #[test]
    fn lowers_generic_for_two_vars() {
        let p = lower_str("for k, v in pairs(t) do x = k end");
        let HirStmt::GenericFor { vars, exprs, body } = &p.main[0] else {
            panic!("expected GenericFor");
        };
        assert_eq!(vars.len(), 2);
        assert_eq!(exprs.len(), 1);
        assert!(matches!(exprs[0], HirExpr::Call { .. }));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn lowers_generic_for_single_var() {
        let p = lower_str("for k in pairs(t) do x = k end");
        let HirStmt::GenericFor { vars, .. } = &p.main[0] else { panic!() };
        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn generic_for_vars_are_local_to_body() {
        // The for-vars must not leak into the surrounding scope.
        let p = lower_str("for k in pairs(t) do end k = 1");
        // The second statement is a global assignment (k was scoped to the loop).
        assert!(matches!(&p.main[1], HirStmt::Assign { .. }));
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-hir`

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): generic-for lowering"
```

---

## Task 5: MIR types — multi-result instructions and terminator

**Files:**
- Modify: `crates/luau-mir/src/types.rs`

- [ ] **Step 1: Add `CallMode` and the new instructions**

Near the existing `Value` enum, add:

```rust
/// Result-collection mode for a `CallVar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallMode {
    /// Discard all results (statement-position call).
    None,
    /// Take exactly the first result; writes into `dst` (which must be Some).
    Scalar,
    /// Pack all results into a fresh results table; writes the table into `dst`.
    Multi,
}
```

Extend `Instr`:

```rust
#[derive(Debug, Clone)]
pub enum Instr {
    LoadConst { dst: VLocal, src: ConstId },
    Move { dst: VLocal, src: VLocal },
    BinOp { dst: VLocal, op: BinOp, lhs: Value, rhs: Value },
    UnOp { dst: VLocal, op: UnOp, operand: Value },
    GetGlobal { dst: VLocal, name: ConstId },
    SetGlobal { name: ConstId, value: Value },
    /// Scalar call: at most one result, no argument spread. Unchanged from Plan 3.
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Variadic call: supports an optional spread-tail argument and/or a
    /// results-table mode. `dst` is required when `mode != CallMode::None`.
    CallVar {
        dst: Option<VLocal>,
        callee: Value,
        args: Vec<Value>,
        spread_tail: Option<VLocal>,
        mode: CallMode,
    },
    /// Build a results table `{v1, v2, …, n=N}` from explicit values, optionally
    /// extended with every element of a spread-tail table.
    BuildResults {
        dst: VLocal,
        values: Vec<Value>,
        spread_tail: Option<VLocal>,
    },
    /// Read the current frame's varargs as a fresh results table.
    GetVarargs { dst: VLocal },
    MakeClosure { dst: VLocal, function: FunctionId, upvalues: Vec<MirUpvalSource> },
    GetUpval { dst: VLocal, idx: u32 },
    SetUpval { idx: u32, value: Value },
    NewTable { dst: VLocal },
    GetIndex { dst: VLocal, obj: Value, key: Value },
    SetIndex { obj: Value, key: Value, value: Value },
}
```

- [ ] **Step 2: Extend `Terminator`**

```rust
#[derive(Debug, Clone)]
pub enum Terminator {
    Goto(BlockId),
    Branch { cond: Value, then_block: BlockId, else_block: BlockId },
    /// Return zero or one explicit value. Multi-value returns use `ReturnMulti`.
    Return(Option<Value>),
    /// Return the spread of a results table — `_tunpack(tbl, 1, tbl.n)`.
    ReturnMulti(Value),
}
```

- [ ] **Step 3: Extend `MirFunction.is_vararg`**

```rust
#[derive(Debug, Clone)]
pub struct MirFunction {
    pub id: FunctionId,
    pub params: Vec<VLocal>,
    pub blocks: Vec<BasicBlock>,
    pub consts: Vec<Constant>,
    pub n_locals: u32,
    pub upvalues: Vec<UpvalueSource>,
    /// True if the function declared `...`.
    pub is_vararg: bool,
}
```

- [ ] **Step 4: Verify build**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo check -p luau-mir`

Expected: compiles, but downstream `cargo check --workspace` will fail until later tasks.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-mir/src/types.rs
git commit -m "feat(mir): CallVar, BuildResults, GetVarargs, ReturnMulti, is_vararg"
```

---

## Task 6: MIR lowering — calls and call-arg spread

**Files:**
- Modify: `crates/luau-mir/src/lower.rs`

The harder MIR work: detect call-arg spread, lower `HirExpr::Vararg`, thread `is_vararg` through.

- [ ] **Step 1: Thread `is_vararg` through the main and pending-function loops**

Find the line `pending.push((main_id, HirFunction { params: Vec::new(), body: hir.main.clone(), upvalues: Vec::new() }));` and add `is_vararg: false`:

```rust
    pending.push((main_id, HirFunction {
        params: Vec::new(),
        body: hir.main.clone(),
        upvalues: Vec::new(),
        is_vararg: false,
    }));
```

Find the `MirFunction { … }` construction at the end of the `while let Some((fid, fhir)) …` loop and add the field:

```rust
        program_functions.push(MirFunction {
            id: fid,
            params: builder.params,
            blocks: builder.blocks,
            consts: builder.consts,
            n_locals: builder.next_local,
            upvalues: fhir.upvalues.clone(),
            is_vararg: fhir.is_vararg,
        });
```

- [ ] **Step 2: Add a helper that lowers call/method args with spread detection**

Add inside `impl<'a> FnBuilder<'a>`:

```rust
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
                // Lower the last expr in multi-result mode.
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
```

Add the file-local helper `expr_can_spread` (mirrors the HIR one — needed because MIR is a separate crate):

```rust
fn expr_can_spread(e: &HirExpr) -> bool {
    matches!(e, HirExpr::Call { .. } | HirExpr::MethodCall { .. } | HirExpr::Vararg)
}
```

Import `CallMode` at the top:

```rust
use crate::{
    BasicBlock, BlockId, CallMode, Constant, ConstId, FunctionId, Instr, MirError,
    MirFunction, MirProgram, SymbolMap, Terminator, VLocal, Value,
};
```

- [ ] **Step 3: Rewrite `HirExpr::Call` and `HirExpr::MethodCall` in `lower_expr` to use `lower_expr_list`**

Replace the two arms:

```rust
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
```

- [ ] **Step 4: Lower `HirExpr::Vararg`**

In `lower_expr`, after the `HirExpr::Upvalue(idx)` arm:

```rust
            HirExpr::Vararg => {
                // Scalar context: take the first vararg. Build the table, then index it.
                // (Tail-position contexts go through lower_expr_as_results_table directly
                // and never reach this arm.)
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
```

- [ ] **Step 5: Update the `lower_table` helper to handle `Array(Vararg)` as a trailing spread**

Replace the function:

```rust
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
            // Append spread_tbl[1..n] into dst[array_idx..].
            // We model this with a small inline loop in MIR: NumericFor-style.
            // Simpler approach: emit a runtime helper via `BuildResults` + copy.
            // But to keep MIR ops small, we materialize as: read n from the table,
            // then `for i = 1, n do dst[array_idx + i - 1] = spread_tbl[i] end`.
            //
            // Implementing that as MIR instructions here is ugly. Instead, fold
            // the spread into BuildResults: build a fresh results table that
            // unions existing entries + spread, then copy back. Even simpler:
            // call into a tiny runtime helper. We'll emit a `BuildResults` and
            // then index-copy from it.
            //
            // For Plan 4 simplicity, we lower this as:
            //   tmp = BuildResults { values: [], spread_tail: spread_tbl }  -- (= shallow copy)
            //   for i = 1, tmp.n: dst[array_idx + i - 1] = tmp[i]
            // Done at the MIR layer via NumericFor expansion.
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
        // base_const for the offset computation
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
            op: BinOp::Le,
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
            op: BinOp::Add,
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
            op: BinOp::Add,
            lhs: Value::VLocal(i_slot),
            rhs: Value::VLocal(step_v),
        });
        self.emit(Instr::Move { dst: i_slot, src: new_i });
        self.set_terminator(Terminator::Goto(header));
        self.switch_to(exit);
        Ok(())
    }
```

- [ ] **Step 6: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-mir`

Most existing tests should still pass — we only changed `lower_table` and `lower_expr` for `Call`/`MethodCall`/`Vararg`, all of which were either unaffected by the test corpus or have been generalized.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-mir/src/lower.rs
git commit -m "feat(mir): expr-list spread detection, GetVarargs, BuildResults, CallVar"
```

---

## Task 7: MIR lowering — statements (return-multi, multi-bind, generic-for)

**Files:**
- Modify: `crates/luau-mir/src/lower_stmts.rs`

- [ ] **Step 1: Update imports**

```rust
use crate::{
    lower::FnBuilder, CallMode, Constant, Instr, MirError, Terminator, Value, VLocal,
};
use luau_hir::{AssignTarget, HirExpr, HirStmt, UpvalueSource};
```

- [ ] **Step 2: Update the existing `HirStmt::Return(maybe)` arm**

`Return(Some(e))` where `e` is spreadable was rewritten to `ReturnMulti(vec![e])` in the HIR layer (Task 3) — so this arm only handles the non-spread cases:

The existing arm stays the same; no change needed. But verify the existing arm matches `HirStmt::Return(maybe) => …` and doesn't fall through wrong.

- [ ] **Step 2b: Rewrite the `HirStmt::ExprStmt` arm to route call args through `lower_expr_list`**

The Plan 3 arm emits `Instr::Call` directly with each arg lowered via `b.lower_expr`. That loses spread semantics: `print(f())` and `print(...)` would only get the first value. Replace the arm:

```rust
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
```

- [ ] **Step 3: Add the `HirStmt::ReturnMulti` arm**

After the existing `HirStmt::Return(maybe)` arm:

```rust
        HirStmt::ReturnMulti(exprs) => {
            // Build a results table from the explicit values + optional spread tail,
            // then emit a ReturnMulti terminator.
            let (values, spread_tail) = b.lower_expr_list(exprs)?;
            let dst = b.fresh_local();
            b.emit(Instr::BuildResults { dst, values, spread_tail });
            b.set_terminator(Terminator::ReturnMulti(Value::VLocal(dst)));
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
```

- [ ] **Step 4: Add the `HirStmt::LocalDeclMulti` arm**

After the existing `HirStmt::LocalDecl { … }` arm:

```rust
        HirStmt::LocalDeclMulti { symbols, exprs } => {
            // exprs ends in a spread-capable expression. Build a results table
            // and then index into it for each symbol.
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
```

Add the helper at the bottom of the file:

```rust
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
```

- [ ] **Step 5: Add the `HirStmt::AssignMulti` arm**

```rust
        HirStmt::AssignMulti { targets, exprs } => {
            // Pre-compute each Index target's `obj` and `key` BEFORE the RHS
            // evaluation — matches Lua's semantics: LHS subexpressions evaluate
            // left-to-right before the RHS list, but assignment writes happen
            // last and left-to-right. (We keep it simple: evaluate LHS first,
            // then RHS, then writes; close enough for non-aliasing cases.)
            //
            // Build the LHS write closures first.
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
```

- [ ] **Step 6: Add the `HirStmt::GenericFor` arm**

```rust
        HirStmt::GenericFor { vars, exprs, body } => {
            // Compute iter/state/ctrl from exprs, applying the "last expr can spread" rule.
            // We always materialize a 3-element triple via a BuildResults table sized
            // to at least 3, then index out iter/state/ctrl.
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
            // Compare new_ctrl to nil. We emit: cond = (new_ctrl ~= nil) via Eq+Not.
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
            // Bind each loop var = step[i+1] (1-based; step[1] is already new_ctrl).
            for (i, var) in vars.iter().enumerate() {
                let val = read_idx(b, step_tbl, (i + 1) as i64)?;
                let slot = b.local_for(*var);
                b.emit(Instr::Move { dst: slot, src: val });
            }
            b.loop_exits.push(exit);
            crate::lower_stmts::lower(b, body)?;
            b.loop_exits.pop();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
```

Add the helper below the function:

```rust
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
```

- [ ] **Step 7: Add MIR tests**

Append to the `tests` mod in `crates/luau-mir/src/lower.rs`:

```rust
    #[test]
    fn multi_return_emits_build_results_and_return_multi() {
        let p = mir_of("function f() return 1, 2 end");
        let f = &p.functions[1];
        let has_build = f.blocks.iter().flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::BuildResults { .. }));
        let has_return_multi = f.blocks.iter()
            .any(|b| matches!(b.terminator, Terminator::ReturnMulti(_)));
        assert!(has_build);
        assert!(has_return_multi);
    }

    #[test]
    fn multi_decl_with_call_uses_build_results() {
        let p = mir_of("local a, b = f()");
        let main = p.main();
        let has_callvar = main.blocks.iter().flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::CallVar { .. }));
        let has_build = main.blocks.iter().flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::BuildResults { .. }));
        assert!(has_callvar);
        assert!(has_build);
    }

    #[test]
    fn vararg_in_function_uses_get_varargs() {
        let p = mir_of("local f = function(...) return ... end");
        let inner = &p.functions[1];
        assert!(inner.is_vararg);
        let has_va = inner.blocks.iter().flat_map(|b| b.instrs.iter())
            .any(|i| matches!(i, Instr::GetVarargs { .. }));
        assert!(has_va);
    }

    #[test]
    fn generic_for_uses_callvar_multi() {
        let p = mir_of("for k, v in pairs(t) do x = k end");
        let main = p.main();
        let callvar_multi_count = main.blocks.iter().flat_map(|b| b.instrs.iter())
            .filter(|i| matches!(i, Instr::CallVar { mode: CallMode::Multi, .. }))
            .count();
        // One for pairs(t), one for each loop step.
        assert!(callvar_multi_count >= 2, "got {}", callvar_multi_count);
    }
```

- [ ] **Step 8: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-mir`

Downstream `cargo test --workspace --release` will still fail until LIR/emit/VM update.

- [ ] **Step 9: Commit**

```bash
git add crates/luau-mir/src/lower_stmts.rs crates/luau-mir/src/lower.rs
git commit -m "feat(mir): multi-return, multi-bind, generic-for statement lowering"
```

---

## Task 8: LIR — new opcodes and operand kinds

**Files:**
- Modify: `crates/luau-lir/src/types.rs`
- Modify: `crates/luau-lir/src/regalloc.rs`
- Modify: `crates/luau-lir/src/lower.rs`

- [ ] **Step 1: Extend `OpKind` and `LirFunction`**

In `crates/luau-lir/src/types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    LoadNil, LoadTrue, LoadFalse, LoadConst,
    Move,
    Add, Sub, Mul, Div, Mod, Pow,
    Concat,
    Lt, Le, Eq,
    Not, Neg, Len,
    GetGlobal, SetGlobal,
    Call, Return,
    Jmp, JmpIfTrue, JmpIfFalse,
    Closure,
    NewTable, GetTable, SetTable,
    GetUpval, SetUpval,
    // Plan 4 additions:
    CallVar,
    BuildResults,
    Vararg,
    ReturnMulti,
}
```

Extend `LirFunction`:

```rust
#[derive(Debug, Clone)]
pub struct LirFunction {
    pub id: ProtoId,
    pub num_params: u16,
    pub num_regs: u16,
    pub num_upvals: u16,
    /// True if this function declared `...` and reads varargs from the frame.
    pub is_vararg: bool,
    pub consts: Vec<Constant>,
    pub instrs: Vec<LirInstr>,
    pub label_positions: Vec<(BlockLabel, u32)>,
    pub closure_upval_sources: Vec<Vec<UpvalSource>>,
    /// Per-`BuildResults`-instruction value list. The encoder consumes these in
    /// order — variable-length, encoded inline like `closure_upval_sources`.
    pub build_results_values: Vec<Vec<Operand>>,
}
```

Each entry in `build_results_values` is a `Vec<Operand>` containing only `Operand::Reg` items (the literal value operands), in declaration order.

- [ ] **Step 2: Add a sentinel for "no spread" register**

The LIR encoding uses `0xFFFF` as a sentinel for "no destination register" (already used by Call). Use the same sentinel for "no spread tail":

```rust
pub const NO_REG: u16 = 0xFFFF;
```

Place it next to `Reg` in `types.rs` and export from `lib.rs`.

- [ ] **Step 3: Update `regalloc.rs::visit_vlocals`**

Add arms for the new MIR instructions:

```rust
        Instr::CallVar { dst, callee, args, spread_tail, .. } => {
            if let Some(d) = dst { f(*d); }
            visit_value(*callee, &mut f);
            for a in args { visit_value(*a, &mut f); }
            if let Some(s) = spread_tail { f(*s); }
        }
        Instr::BuildResults { dst, values, spread_tail } => {
            f(*dst);
            for v in values { visit_value(*v, &mut f); }
            if let Some(s) = spread_tail { f(*s); }
        }
        Instr::GetVarargs { dst } => f(*dst),
```

And update `visit_terminator_vlocals` to handle `ReturnMulti`:

```rust
        Terminator::ReturnMulti(v) => {
            if let Value::VLocal(l) = v { f(*l); }
        }
```

- [ ] **Step 4: Update `lower.rs::lower_function` to set `is_vararg` and init `build_results_values`**

```rust
fn lower_function(f: &MirFunction) -> Result<LirFunction, LirError> {
    let rm = RegMap::assign_all(f);
    let mut instrs: Vec<LirInstr> = Vec::new();
    let mut label_positions: Vec<(BlockLabel, u32)> = Vec::new();
    let mut closure_upval_sources: Vec<Vec<UpvalSource>> = Vec::new();
    let mut build_results_values: Vec<Vec<Operand>> = Vec::new();

    let label_of = |b: BlockId| BlockLabel(b.0);

    // Compute max explicit args across both Call and CallVar.
    let max_call_args: u16 = f.blocks.iter().flat_map(|b| &b.instrs)
        .filter_map(|instr| match instr {
            MInstr::Call { args, .. } => Some(args.len() as u16),
            MInstr::CallVar { args, .. } => Some(args.len() as u16),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let scratch_base: u16 = rm.num_regs();
    let has_any_call = f.blocks.iter().flat_map(|b| &b.instrs).any(|i| {
        matches!(i, MInstr::Call { .. } | MInstr::CallVar { .. })
    });
    let final_num_regs: u16 = if has_any_call || max_call_args > 0 {
        scratch_base + 1 + max_call_args
    } else {
        rm.num_regs()
    };

    for (idx, block) in f.blocks.iter().enumerate() {
        label_positions.push((label_of(block.id), instrs.len() as u32));
        for instr in &block.instrs {
            lower_instr(
                instr,
                &rm,
                scratch_base,
                &mut instrs,
                &mut closure_upval_sources,
                &mut build_results_values,
            );
        }
        let next_block = f.blocks.get(idx + 1).map(|b| b.id);
        lower_terminator(&block.terminator, &rm, &mut instrs, next_block, label_of);
    }

    Ok(LirFunction {
        id: ProtoId(f.id.0 as u16),
        num_params: f.params.len() as u16,
        num_regs: final_num_regs,
        num_upvals: f.upvalues.len() as u16,
        is_vararg: f.is_vararg,
        consts: f.consts.clone(),
        instrs,
        label_positions,
        closure_upval_sources,
        build_results_values,
    })
}
```

- [ ] **Step 5: Lower the new MIR instructions**

In `lower_instr`, add arms before the existing `MInstr::NewTable` arm. Update the signature:

```rust
fn lower_instr(
    instr: &MInstr,
    rm: &RegMap,
    scratch_base: u16,
    out: &mut Vec<LirInstr>,
    closure_upval_sources: &mut Vec<Vec<UpvalSource>>,
    build_results_values: &mut Vec<Vec<Operand>>,
) {
```

Add the new arms inside the match:

```rust
        MInstr::CallVar { dst, callee, args, spread_tail, mode } => {
            let callee_src = val_to_reg(*callee, rm);
            let n = args.len() as u16;
            let scratch_callee = Reg(scratch_base);
            if scratch_callee != callee_src {
                out.push(LirInstr {
                    op: OpKind::Move,
                    operands: vec![Operand::Reg(scratch_callee), Operand::Reg(callee_src)],
                });
            }
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
            let spread_reg = match spread_tail {
                Some(v) => Reg(rm.get(*v)).0,
                None => crate::NO_REG,
            };
            let mode_byte: i16 = match mode {
                CallMode::None => 0,
                CallMode::Scalar => 1,
                CallMode::Multi => 2,
            };
            let dst_reg = match dst {
                Some(d) => Reg(rm.get(*d)),
                None => Reg(crate::NO_REG),
            };
            out.push(LirInstr {
                op: OpKind::CallVar,
                operands: vec![
                    Operand::Reg(dst_reg),
                    Operand::Reg(scratch_callee),
                    Operand::SmallInt(n as i16),
                    Operand::Reg(Reg(spread_reg)),
                    Operand::SmallInt(mode_byte),
                ],
            });
        }
        MInstr::BuildResults { dst, values, spread_tail } => {
            // Encode the variable-length value list separately, like Closure's upval list.
            let val_ops: Vec<Operand> = values.iter().map(|v| Operand::Reg(val_to_reg(*v, rm))).collect();
            build_results_values.push(val_ops);
            let br_idx = (build_results_values.len() - 1) as i16;
            let spread_reg = match spread_tail {
                Some(v) => Reg(rm.get(*v)).0,
                None => crate::NO_REG,
            };
            out.push(LirInstr {
                op: OpKind::BuildResults,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Reg(Reg(spread_reg)),
                    Operand::SmallInt(br_idx),
                ],
            });
        }
        MInstr::GetVarargs { dst } => {
            out.push(LirInstr {
                op: OpKind::Vararg,
                operands: vec![Operand::Reg(Reg(rm.get(*dst)))],
            });
        }
```

Use `CallMode` via:

```rust
use luau_mir::CallMode;
```

- [ ] **Step 6: Add a `Terminator::ReturnMulti` arm**

In `lower_terminator`:

```rust
        Terminator::ReturnMulti(v) => {
            let r = match v {
                MValue::VLocal(l) => Reg(rm.get(*l)),
                MValue::Const(_) => panic!("ReturnMulti Const"),
            };
            out.push(LirInstr {
                op: OpKind::ReturnMulti,
                operands: vec![Operand::Reg(r)],
            });
        }
```

- [ ] **Step 7: Add LIR tests**

In the `tests` mod, append:

```rust
    #[test]
    fn multi_return_lowers_to_return_multi_op() {
        let p = lir_of("function f() return 1, 2 end");
        let f = &p.functions[1];
        let has_rm = f.instrs.iter().any(|i| i.op == OpKind::ReturnMulti);
        assert!(has_rm);
    }

    #[test]
    fn build_results_records_value_list() {
        let p = lir_of("function f() return 1, 2, 3 end");
        let f = &p.functions[1];
        assert!(!f.build_results_values.is_empty());
        // The BuildResults for `1, 2, 3` should have 3 values.
        assert!(f.build_results_values.iter().any(|v| v.len() == 3));
    }

    #[test]
    fn vararg_function_sets_is_vararg() {
        let p = lir_of("local f = function(...) end");
        // f is functions[1]; main is functions[0].
        let f = &p.functions[1];
        assert!(f.is_vararg);
    }

    #[test]
    fn generic_for_uses_callvar_op() {
        let p = lir_of("for k in pairs(t) do x = k end");
        let main = &p.functions[0];
        let has_callvar = main.instrs.iter().any(|i| i.op == OpKind::CallVar);
        assert!(has_callvar);
    }
```

- [ ] **Step 8: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-lir`

- [ ] **Step 9: Commit**

```bash
git add crates/luau-lir/src/types.rs crates/luau-lir/src/regalloc.rs crates/luau-lir/src/lower.rs crates/luau-lir/src/lib.rs
git commit -m "feat(lir): CallVar, BuildResults, Vararg, ReturnMulti opcodes"
```

---

## Task 9: Emit — opmap, encode, render

**Files:**
- Modify: `crates/luau-emit/src/opmap.rs`
- Modify: `crates/luau-emit/src/encode.rs`
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Update `opmap.rs`**

Resize `forward` from `[u8; 31]` → `[u8; 35]`. Append the four new opcodes to `ALL_OPS`:

```rust
pub const ALL_OPS: &[OpKind] = &[
    OpKind::LoadNil, OpKind::LoadTrue, OpKind::LoadFalse, OpKind::LoadConst,
    OpKind::Move,
    OpKind::Add, OpKind::Sub, OpKind::Mul, OpKind::Div, OpKind::Mod, OpKind::Pow,
    OpKind::Concat,
    OpKind::Lt, OpKind::Le, OpKind::Eq,
    OpKind::Not, OpKind::Neg, OpKind::Len,
    OpKind::GetGlobal, OpKind::SetGlobal,
    OpKind::Call, OpKind::Return,
    OpKind::Jmp, OpKind::JmpIfTrue, OpKind::JmpIfFalse,
    OpKind::Closure,
    OpKind::NewTable, OpKind::GetTable, OpKind::SetTable,
    OpKind::GetUpval, OpKind::SetUpval,
    OpKind::CallVar, OpKind::BuildResults, OpKind::Vararg, OpKind::ReturnMulti,
];
```

Update both `[u8; 31]` references to `[u8; 35]` (struct field and `forward` init in `OpMap::new`).

- [ ] **Step 2: Update `render.rs`**

Extend `opname`:

```rust
        OpKind::CallVar => "CallVar",
        OpKind::BuildResults => "BuildResults",
        OpKind::Vararg => "Vararg",
        OpKind::ReturnMulti => "ReturnMulti",
```

Update `MetaEntry` and its construction:

```rust
#[derive(Serialize)]
struct MetaEntry {
    num_params: u16,
    num_regs: u16,
    num_upvals: u16,
    is_vararg: u8,  // 0 or 1, since minijinja's int/bool boundary is annoying
}
```

```rust
    let meta: Vec<MetaEntry> = program
        .functions
        .iter()
        .map(|f| MetaEntry {
            num_params: f.num_params,
            num_regs: std::cmp::max(f.num_regs, f.num_params),
            num_upvals: f.num_upvals,
            is_vararg: if f.is_vararg { 1 } else { 0 },
        })
        .collect();
```

- [ ] **Step 3: Update `encode.rs` to handle the variable-length `BuildResults` value list**

`BuildResults` carries a trailing variable-length list of register operands, similar to `Closure`'s upvalue source list. Replace the `encode.rs` content:

```rust
use crate::opmap::OpMap;
use luau_lir::{BlockLabel, LirFunction, LirInstr, OpKind, Operand, UpvalSource};
use std::collections::HashMap;

pub fn encode_function(f: &LirFunction, opmap: &OpMap) -> Vec<u8> {
    let mut instr_byte_offsets: Vec<u32> = Vec::with_capacity(f.instrs.len() + 1);
    let mut offset: u32 = 0;
    let mut closure_counter: usize = 0;
    let mut build_results_counter: usize = 0;
    for instr in &f.instrs {
        instr_byte_offsets.push(offset);
        offset += instr_size_for(instr, f, &mut closure_counter, &mut build_results_counter);
    }
    instr_byte_offsets.push(offset);

    let mut label_byte: HashMap<BlockLabel, u32> = HashMap::new();
    for (label, instr_idx) in &f.label_positions {
        let byte_off = if (*instr_idx as usize) < instr_byte_offsets.len() {
            instr_byte_offsets[*instr_idx as usize]
        } else {
            *instr_byte_offsets.last().unwrap()
        };
        label_byte.insert(*label, byte_off);
    }

    let mut closure_emit: usize = 0;
    let mut br_emit: usize = 0;
    let mut out: Vec<u8> = Vec::with_capacity(offset as usize);
    for (i, instr) in f.instrs.iter().enumerate() {
        out.push(opmap.opcode_of(instr.op));
        let after_this = instr_byte_offsets[i] + instr_size_for_at(instr, f, closure_emit, br_emit);
        for operand in &instr.operands {
            match operand {
                Operand::Reg(r) => push_u16(&mut out, r.0),
                Operand::Const(c) => push_u16(&mut out, c.0),
                Operand::Proto(p) => push_u16(&mut out, p.0),
                Operand::SmallInt(n) => push_u16(&mut out, *n as u16),
                Operand::UpvalIdx(u) => push_u16(&mut out, *u),
                Operand::JmpTarget(label) => {
                    let target_byte = *label_byte.get(label).expect("label");
                    let delta = target_byte as i32 - after_this as i32;
                    push_u16(&mut out, delta as i16 as u16);
                }
            }
        }
        if instr.op == OpKind::Closure {
            let sources = &f.closure_upval_sources[closure_emit];
            closure_emit += 1;
            push_u16(&mut out, sources.len() as u16);
            for src in sources {
                match src {
                    UpvalSource::LocalReg(r) => {
                        out.push(0);
                        push_u16(&mut out, *r);
                    }
                    UpvalSource::ParentUpval(u) => {
                        out.push(1);
                        push_u16(&mut out, *u);
                    }
                }
            }
        } else if instr.op == OpKind::BuildResults {
            let values = &f.build_results_values[br_emit];
            br_emit += 1;
            push_u16(&mut out, values.len() as u16);
            for v in values {
                let Operand::Reg(r) = v else { panic!("BuildResults value must be a Reg") };
                push_u16(&mut out, r.0);
            }
        }
    }
    out
}

fn instr_size_for(
    i: &LirInstr,
    f: &LirFunction,
    closure_counter: &mut usize,
    br_counter: &mut usize,
) -> u32 {
    let base = 1 + (i.operands.len() as u32) * 2;
    if i.op == OpKind::Closure {
        let sources = &f.closure_upval_sources[*closure_counter];
        *closure_counter += 1;
        return base + 2 + (sources.len() as u32) * 3;
    }
    if i.op == OpKind::BuildResults {
        let values = &f.build_results_values[*br_counter];
        *br_counter += 1;
        return base + 2 + (values.len() as u32) * 2;
    }
    base
}

fn instr_size_for_at(
    i: &LirInstr,
    f: &LirFunction,
    closure_idx: usize,
    br_idx: usize,
) -> u32 {
    let base = 1 + (i.operands.len() as u32) * 2;
    if i.op == OpKind::Closure {
        let sources = &f.closure_upval_sources[closure_idx];
        return base + 2 + (sources.len() as u32) * 3;
    }
    if i.op == OpKind::BuildResults {
        let values = &f.build_results_values[br_idx];
        return base + 2 + (values.len() as u32) * 2;
    }
    base
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.push((v & 0xFF) as u8);
    out.push(((v >> 8) & 0xFF) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opmap::OpMap;
    use luau_lir::{LirFunction, LirInstr, OpKind, Operand, ProtoId, Reg};

    #[test]
    fn encodes_single_return_instruction() {
        let f = LirFunction {
            id: ProtoId(0),
            num_params: 0,
            num_regs: 0,
            num_upvals: 0,
            is_vararg: false,
            consts: vec![],
            instrs: vec![LirInstr { op: OpKind::Return, operands: vec![Operand::Reg(Reg(0xFFFF))] }],
            label_positions: vec![],
            closure_upval_sources: vec![],
            build_results_values: vec![],
        };
        let opmap = OpMap::new(&[0u8; 32]);
        let bytes = encode_function(&f, &opmap);
        assert_eq!(bytes.len(), 3);
        assert_eq!(bytes[0], opmap.opcode_of(OpKind::Return));
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0xFF);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-emit`

- [ ] **Step 5: Commit**

```bash
git add crates/luau-emit/src/opmap.rs crates/luau-emit/src/encode.rs crates/luau-emit/src/render.rs
git commit -m "feat(emit): opmap/encoder for CallVar, BuildResults, Vararg, ReturnMulti"
```

---

## Task 10: VM template — multi-return, varargs, results tables

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

The VM gains: a `_tpack` helper, per-frame `varargs` setup, and handlers for the four new opcodes. The META table grows a fourth element.

- [ ] **Step 1: Add the `_tpack` helper**

Near the other `local _foo = …` declarations at the top of the template, add:

```luau
local _tpack = table.pack or function(...)
    local t = {...}
    t.n = _select("#", ...)
    return t
end
```

- [ ] **Step 2: Update the META loop to include `is_vararg`**

Replace the existing META loop:

```luau
-- Proto metadata: { num_params, num_regs, num_upvals, is_vararg }
local META = {
{% for m in meta %}    { {{ m.num_params }}, {{ m.num_regs }}, {{ m.num_upvals }}, {{ m.is_vararg }} },
{% endfor %}}
```

- [ ] **Step 3: Update `vm_call` to build the frame's varargs table**

Replace the frame-setup section of `vm_call` (just after `regs = _tcreate(num_regs + 1)`):

```luau
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local meta = META[proto_id + 1]
    local num_params = meta[1]
    local num_regs = meta[2]
    local is_vararg = meta[4] == 1
    upvals = upvals or {}
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local frame_varargs
    if is_vararg then
        local vn = nargs - num_params
        if vn < 0 then vn = 0 end
        frame_varargs = {}
        for i = 1, vn do
            frame_varargs[i] = args[num_params + i]
        end
        frame_varargs.n = vn
    end
```

- [ ] **Step 4: Add the four new opcode handlers**

Insert immediately after the existing `OP_SetUpval` handler:

```luau
        elseif op == OP_CallVar then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local n = read_i16(code, pc); pc = pc + 2
            local sp = read_u16(code, pc); pc = pc + 2
            local mode = read_i16(code, pc); pc = pc + 2
            local fn = regs[b]
            local call_args = {}
            local total_n = n
            for i = 1, n do
                call_args[i] = regs[b + i]
            end
            if sp ~= 0xFFFF then
                local sp_tbl = regs[sp]
                local m = sp_tbl.n or 0
                for i = 1, m do
                    call_args[total_n + 1] = sp_tbl[i]
                    total_n = total_n + 1
                end
            end
            if mode == 0 then
                fn(_tunpack(call_args, 1, total_n))
            elseif mode == 1 then
                local r = fn(_tunpack(call_args, 1, total_n))
                if a ~= 0xFFFF then
                    regs[a] = r
                end
            else
                local results = _tpack(fn(_tunpack(call_args, 1, total_n)))
                regs[a] = results
            end
        elseif op == OP_BuildResults then
            local a = read_u16(code, pc); pc = pc + 2
            local sp = read_u16(code, pc); pc = pc + 2
            pc = pc + 2  -- skip the br_idx operand (Rust-side encoding bookkeeping)
            local n_values = read_u16(code, pc); pc = pc + 2
            local tbl = {}
            local count = 0
            for _ = 1, n_values do
                local r = read_u16(code, pc); pc = pc + 2
                count = count + 1
                tbl[count] = regs[r]
            end
            if sp ~= 0xFFFF then
                local sp_tbl = regs[sp]
                local m = sp_tbl.n or 0
                for i = 1, m do
                    count = count + 1
                    tbl[count] = sp_tbl[i]
                end
            end
            tbl.n = count
            regs[a] = tbl
        elseif op == OP_Vararg then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = frame_varargs
        elseif op == OP_ReturnMulti then
            local a = read_u16(code, pc); pc = pc + 2
            local tbl = regs[a]
            return _tunpack(tbl, 1, tbl.n)
```

Note the `BuildResults` handler reads its trailing value list via `read_u16` per slot, matching the encoder's `push_u16` per register operand.

- [ ] **Step 5: Run the full workspace tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

All Plan 1–3 corpus programs should still pass. The Plan 4 corpus is added next.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-runtime/assets/vm.luau.j2
git commit -m "feat(runtime): varargs frame, CallVar/BuildResults/Vararg/ReturnMulti"
```

---

## Task 11: Corpus — differential tests for multi-return, varargs, generic-for

**Files:**
- Create: `runtime-tests/corpus/multi_return.luau`
- Create: `runtime-tests/corpus/generic_for.luau`
- Create: `runtime-tests/corpus/varargs.luau`

- [ ] **Step 1: Create `runtime-tests/corpus/multi_return.luau`**

```luau
local function pair_of(a, b)
    return a, b
end

local x, y = pair_of(3, 7)
print(x)
print(y)

-- Spread call into multi-bind.
local function triple()
    return 10, 20, 30
end

local a, b, c = triple()
print(a)
print(b)
print(c)

-- `return f()` spreads f's results.
local function forward()
    return triple()
end

local p, q, r = forward()
print(p)
print(q)
print(r)

-- Multi-bind with non-spread RHS pads with nil.
local m, n = 1
print(m)
print(n)

-- Multi-assign through globals.
g1, g2 = pair_of(100, 200)
print(g1)
print(g2)
```

- [ ] **Step 2: Create `runtime-tests/corpus/generic_for.luau`**

```luau
local t = { a = 1, b = 2, c = 3 }
local sum = 0
for k, v in pairs(t) do
    sum = sum + v
end
print(sum)  -- 6 regardless of iteration order

local arr = { 10, 20, 30, 40 }
local total = 0
for i, v in ipairs(arr) do
    total = total + i * v
end
print(total)  -- 1*10 + 2*20 + 3*30 + 4*40 = 10 + 40 + 90 + 160 = 300

-- Custom iterator with the (iter, state, ctrl) protocol.
local function range_iter(state, ctrl)
    local nxt = ctrl + 1
    if nxt > state then return nil end
    return nxt, nxt * nxt
end

local function range(stop)
    return range_iter, stop, 0
end

local squares_sum = 0
for i, sq in range(5) do
    squares_sum = squares_sum + sq
end
print(squares_sum)  -- 1+4+9+16+25 = 55
```

- [ ] **Step 3: Create `runtime-tests/corpus/varargs.luau`**

```luau
local function count(...)
    return select("#", ...)
end
print(count())
print(count(1))
print(count(1, 2, 3))

-- Forwarding varargs.
local function fwd(...)
    return count(...)
end
print(fwd(7, 8, 9, 10))

-- Collecting into a table.
local function pack_them(...)
    local t = {...}
    local s = 0
    for i = 1, #t do
        s = s + t[i]
    end
    return s
end
print(pack_them(1, 2, 3, 4, 5))  -- 15

-- `select(i, ...)` returns the i-th vararg and everything after.
-- We collect into a table so we can pick out individual positions —
-- paren-truncation (`(select(i, ...))` taking only the first value) is a
-- Plan 5+ refinement; for now keep all values reachable explicitly.
local function rest(i, ...)
    local vals = {select(i, ...)}
    return vals[1], vals[2]
end
local r1, r2 = rest(2, "x", "y", "z")
print(r1)  -- "y"
print(r2)  -- "z"
```

- [ ] **Step 4: Run the differential harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Or directly: `PATH="$HOME/.cargo/bin:$PATH" cargo run -p runtime-tests --release`

All Plan 1–3 + Plan 4 corpus programs should pass.

If `generic_for.luau` fails because `pairs(t)` iteration order differs between plain Luau and the obfuscated VM — that *shouldn't* happen because both run on the same Luau host VM, so order is identical between the two runs. Should the test be flaky, sort the output deterministically: instead of `print(sum)`, sum into a single number (already done above) — no ordering observable.

- [ ] **Step 5: Commit**

```bash
git add runtime-tests/corpus/multi_return.luau runtime-tests/corpus/generic_for.luau runtime-tests/corpus/varargs.luau
git commit -m "test(corpus): multi-return, generic-for, varargs"
```

---

## Task 12: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the Plan 3 status line**

```
**Status:** Plan 4 — multi-return, varargs, generic-for. The obfuscator round-trips
an expanded Luau subset (Plan 3 features + multi-value `return a, b`, multi-bind
`local a, b = f()`, varargs `...` parameters and spread, `for k, v in pairs(t)`
and other generic-for iterators) through a custom VM with no obfuscation passes
yet.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 4 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All corpus programs (Plan 1 + 2 + 3 + 4) pass differential testing.
- [ ] `cargo clippy --workspace --tests` clean (or no new warnings beyond Plan 3).
- [ ] Determinism: same seed → same output for `multi_return.luau`, `generic_for.luau`, `varargs.luau`.
- [ ] No `panic!("...")` or `todo!()` left in the new code, except where the existing pattern uses `panic!` for "should be Const'd" guards.
- [ ] HIR `lower_assign_target` (Plan 3's helper) is either still in use or has been deleted — no dead code.
- [ ] The VM template's bootstrap `return vm_call(0, main_args, main_n, {})` still works — the main proto's `is_vararg` is `false`, so `frame_varargs` is `nil` and unused.
