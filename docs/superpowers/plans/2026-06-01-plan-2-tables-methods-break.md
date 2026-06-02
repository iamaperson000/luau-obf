# Luau Obfuscator — Plan 2: Tables, Methods, Break

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the obfuscator's supported Luau subset with tables (constructors, indexing, indexed assignment), method calls and method declarations, anonymous function expressions, and the `break` statement. Closures still capture nothing (Plan 3 adds upvalues), and multi-return / varargs / for-in / continue remain deferred.

**Architecture:** Pure extension of the existing pipeline — every layer (HIR types/lowering, MIR types/lowering, LIR types/lowering, opcode map, VM template) gets new variants for the new constructs, and `FnBuilder` grows a `loop_exits: Vec<BlockId>` stack to thread `break` through nested loops. No structural reshape.

**Tech Stack:** Same as Plan 1 — `full_moon` for parsing, `minijinja` for the VM template, `rand_chacha` for seeded randomness, `thiserror` for errors. Differential tests via the `luau` CLI.

**Plan 2 language additions** (relative to Plan 1's subset):

| Newly supported | Still deferred |
|---|---|
| Table constructors: `{}`, `{1, 2, 3}`, `{x = 1}`, mixed `{1, x = 2}`, computed `{[k] = v}` | Closures capturing parent locals / `local function` (Plan 3) |
| Indexed read: `t[k]`, `t.field`, chained `a.b.c` | Multi-return, varargs (Plan 3) |
| Indexed assignment: `t[k] = v`, `t.field = v` | `for k, v in pairs(t)` / generalized iteration (Plan 3) |
| Anonymous function expressions: `local f = function() end` (no upvalue capture) | `continue`, compound assignment, string interpolation (Plan 4) |
| Method calls: `obj:method(args)` | Metatables, `__index`/`__newindex` (Plan 5) |
| Method declarations: `function obj:method() ... end` | `vector`, `buffer`, `bit32` (Plan 6) |
| Dotted function declarations: `function t.f() ... end`, `function a.b.c() ... end` | |
| `break` in `while`/`repeat`/`numeric for` | |
| Tables as function-call arguments: `f{1,2,3}` | |
| Length operator `#t` on tables (the op already exists; just gains semantic meaning) | |

**Out-of-scope reminder:** anonymous function bodies still cannot reference enclosing locals — Plan 1's HIR scope resolution will treat such names as globals. The HIR resolver behavior is unchanged. Plan 3 fixes this with proper upvalue capture.

---

## Implementation notes

**full_moon API:** Plan 2's code blocks name specific full_moon types — `Suffix::Index`, `Index::Brackets`, `Index::Dot`, `Call::MethodCall`, `MethodCall::name()`, `FunctionName::names()`, `FunctionName::method_name()`, `Expression::Function`, `TableConstructor::fields()`, `Field::ExpressionKey`, `Field::NameKey`, `Field::NoKey`, etc. These reflect full_moon 1.1's public API. If a method name or variant shape disagrees with the version on `docs.rs/full_moon`, **adapt** — the structural shape (recursive descent over prefix + suffix chains, field iteration) is what matters. Run `cargo doc --open -p full_moon` and pattern-match against the real types.

**Stub `todo!()` macros:** None this plan. Every task ships a working layer.

**Refactor before extending:** Task 1 reshapes the HIR lowerer so suffix handling (call/index) goes through one helper that both `lower_call` and the new variable-expression path can call. This is a pure restructure with no behavioral change — Plan 1's existing tests must still pass after Task 1 before any new functionality lands.

**Engineer-typed code blocks:** Each task shows the content of the files it touches. Where a task says "replace function X" or "modify function Y," make sure to replace the existing definition rather than appending; overlapping `fn` definitions will cause compile errors. Where a task says "append to," add at the end without disturbing earlier content.

**Determinism:** Plan 2 introduces no new randomization — all new behavior is deterministic, so `cargo test` on the differential harness should pass with the existing fixed-seed corpus path.

---

## File Structure (changes only)

```
luau-obf/
├── crates/
│   ├── luau-hir/src/
│   │   ├── types.rs                 # MODIFY: HirExpr variants, HirStmt variants, TableEntry
│   │   └── lower.rs                 # MODIFY: refactor + table/index/method/break/anonymous-fn lowering
│   ├── luau-mir/src/
│   │   ├── types.rs                 # MODIFY: Instr::NewTable, GetIndex, SetIndex
│   │   ├── lower.rs                 # MODIFY: FnBuilder gets loop_exits; expression lowering for new HIR
│   │   └── lower_stmts.rs           # MODIFY: IndexAssign + Break + loop_exits push/pop
│   ├── luau-lir/src/
│   │   ├── types.rs                 # MODIFY: OpKind::NewTable, GetTable, SetTable
│   │   ├── regalloc.rs              # MODIFY: visit_vlocals handles new instrs
│   │   └── lower.rs                 # MODIFY: lower_instr handles new MIR instrs
│   ├── luau-emit/src/
│   │   ├── opmap.rs                 # MODIFY: ALL_OPS + forward[N] sizing
│   │   └── render.rs                # MODIFY: opname() entries for new ops
│   └── luau-runtime/assets/
│       └── vm.luau.j2               # MODIFY: handlers for NewTable, GetTable, SetTable
├── runtime-tests/corpus/
│   ├── tables.luau                  # CREATE
│   ├── methods.luau                 # CREATE
│   └── break.luau                   # CREATE
├── docs/superpowers/plans/
│   └── 2026-06-01-plan-2-tables-methods-break.md   # this plan
└── README.md                        # MODIFY: status line
```

---

## Task 1: HIR lowerer refactor — shared suffix handling

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

Plan 1's `lower_call` and `lower_var` both walked prefix + suffix chains in full_moon, but with separate, partially duplicated code paths. Plan 2 needs to handle index suffixes and method-call suffixes on both paths, so they share one `lower_suffix` helper.

This task is a **pure refactor**: every existing test must still pass with no behavior change. The new helper handles only the cases Plan 1 already supported; new suffix variants come in Task 2 and onward.

- [ ] **Step 1: Read the existing `crates/luau-hir/src/lower.rs`** to understand the current `lower_call` / `lower_var` shape.

- [ ] **Step 2: Replace `lower_var` and `lower_call` in `crates/luau-hir/src/lower.rs`** with the refactored versions below, and add the new `lower_suffix` and `lower_var_expression` helpers. Place these next to each other.

```rust
fn lower_var(
    lowerer: &mut Lowerer,
    var: &full_moon::ast::Var,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Var;
    match var {
        Var::Name(token) => {
            let name = token.token().to_string();
            Ok(HirExpr::Symbol(lowerer.resolve(&name)))
        }
        Var::Expression(ve) => lower_var_expression(lowerer, ve),
        other => Err(HirError::Unsupported(format!("var form {other:?}"))),
    }
}

fn lower_var_expression(
    lowerer: &mut Lowerer,
    ve: &full_moon::ast::VarExpression,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Prefix;
    let mut current = match ve.prefix() {
        Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())),
        Prefix::Expression(e) => lower_expr(lowerer, e)?,
        other => return Err(HirError::Unsupported(format!("var prefix {other:?}"))),
    };
    for suffix in ve.suffixes() {
        current = lower_suffix(lowerer, current, suffix)?;
    }
    Ok(current)
}

fn lower_call(
    lowerer: &mut Lowerer,
    call: &full_moon::ast::FunctionCall,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Prefix;
    let mut current = match call.prefix() {
        Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())),
        Prefix::Expression(e) => lower_expr(lowerer, e)?,
        other => return Err(HirError::Unsupported(format!("call prefix {other:?}"))),
    };
    for suffix in call.suffixes() {
        current = lower_suffix(lowerer, current, suffix)?;
    }
    Ok(current)
}

/// Apply one suffix (call or index) to the expression accumulated so far.
/// Plan 1 only supports anonymous calls; later tasks add Index and MethodCall.
fn lower_suffix(
    lowerer: &mut Lowerer,
    current: HirExpr,
    suffix: &full_moon::ast::Suffix,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::{Call, Suffix};
    match suffix {
        Suffix::Call(Call::AnonymousCall(args)) => {
            let args = lower_call_args(lowerer, args)?;
            Ok(HirExpr::Call { callee: Box::new(current), args })
        }
        Suffix::Call(Call::MethodCall(_)) => {
            Err(HirError::Unsupported("method call (lowered in Task 4)".into()))
        }
        Suffix::Index(_) => {
            Err(HirError::Unsupported("index suffix (lowered in Task 2)".into()))
        }
        other => Err(HirError::Unsupported(format!("call suffix {other:?}"))),
    }
}
```

- [ ] **Step 3: Run the existing HIR tests to confirm zero regression**

Run: `cargo test -p luau-hir`
Expected: all Plan 1 tests still pass.

- [ ] **Step 4: Run the full workspace tests to confirm zero regression across the pipeline**

Run: `cargo test --workspace --release`
Expected: all Plan 1 tests still pass, including the differential harness if `luau` is on PATH.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "refactor(hir): share suffix lowering between call and var paths"
```

---

## Task 2: HIR — table constructors and indexed reads

**Files:**
- Modify: `crates/luau-hir/src/types.rs`
- Modify: `crates/luau-hir/src/lower.rs`

This task introduces the HIR-level data for tables and the lowering for `{...}` constructors plus `t[k]` / `t.field` reads.

- [ ] **Step 1: Append the new types in `crates/luau-hir/src/types.rs`**

Add `TableEntry` next to `HirLiteral`:

```rust
#[derive(Debug, Clone)]
pub enum TableEntry {
    /// `value` — positional, integer-keyed at the next slot (1-based).
    Array(HirExpr),
    /// `name = value` — string-keyed shorthand.
    Field(String, HirExpr),
    /// `[expr] = value`.
    Keyed(HirExpr, HirExpr),
}
```

- [ ] **Step 2: Extend `HirExpr` in `crates/luau-hir/src/types.rs`**

Replace the existing `HirExpr` enum with:

```rust
#[derive(Debug, Clone)]
pub enum HirExpr {
    Literal(HirLiteral),
    /// Reference to a resolved symbol.
    Symbol(SymbolId),
    BinOp(BinOp, Box<HirExpr>, Box<HirExpr>),
    UnOp(UnOp, Box<HirExpr>),
    /// f(args...). Function expression is `callee`; arg list is positional.
    Call { callee: Box<HirExpr>, args: Vec<HirExpr> },
    /// `{ ... }` table constructor.
    Table(Vec<TableEntry>),
    /// `obj[key]` or `obj.name` (the latter has a String-literal key).
    Index { obj: Box<HirExpr>, key: Box<HirExpr> },
    /// `obj:method(args)` — kept distinct from Call so MIR doesn't double-eval `obj`.
    MethodCall { obj: Box<HirExpr>, method: String, args: Vec<HirExpr> },
    /// `function(params) body end` — anonymous function expression.
    /// Plan 2: cannot capture parent locals (references become globals).
    Function(HirFunction),
}
```

- [ ] **Step 3: Add table constructor lowering in `crates/luau-hir/src/lower.rs`**

Add `lower_table_ctor` near the other helpers (above the `#[cfg(test)]` block):

```rust
fn lower_table_ctor(
    lowerer: &mut Lowerer,
    tc: &full_moon::ast::TableConstructor,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Field;
    let mut entries = Vec::new();
    for field in tc.fields() {
        match field {
            Field::ExpressionKey { key, value, .. } => {
                let k = lower_expr(lowerer, key)?;
                let v = lower_expr(lowerer, value)?;
                entries.push(crate::TableEntry::Keyed(k, v));
            }
            Field::NameKey { key, value, .. } => {
                let name = key.token().to_string();
                let v = lower_expr(lowerer, value)?;
                entries.push(crate::TableEntry::Field(name, v));
            }
            Field::NoKey(value) => {
                let v = lower_expr(lowerer, value)?;
                entries.push(crate::TableEntry::Array(v));
            }
            other => return Err(HirError::Unsupported(format!("table field {other:?}"))),
        }
    }
    Ok(HirExpr::Table(entries))
}
```

- [ ] **Step 4: Wire table constructors into expression lowering**

In `lower_expr`, in `crates/luau-hir/src/lower.rs`, find the `match expr { ... }` and add a `TableConstructor` arm. Replace the closing wildcard arm to keep it last:

```rust
        E::TableConstructor(tc) => lower_table_ctor(lowerer, tc),
        E::Function(_) => Err(HirError::Unsupported(
            "anonymous function expression (added in Task 3)".into(),
        )),
        other => Err(HirError::Unsupported(format!("expression form {other:?}"))),
```

Add the `E::TableConstructor(tc) => …` and `E::Function(_) => …` arms right above the existing `other =>` line. The `E::Function(_)` arm is a placeholder so the compiler doesn't error out; Task 3 fills it in.

- [ ] **Step 5: Add Index suffix support in `lower_suffix`**

Replace the body of `lower_suffix` in `crates/luau-hir/src/lower.rs` with:

```rust
fn lower_suffix(
    lowerer: &mut Lowerer,
    current: HirExpr,
    suffix: &full_moon::ast::Suffix,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::{Call, Index, Suffix};
    match suffix {
        Suffix::Call(Call::AnonymousCall(args)) => {
            let args = lower_call_args(lowerer, args)?;
            Ok(HirExpr::Call { callee: Box::new(current), args })
        }
        Suffix::Call(Call::MethodCall(_)) => {
            Err(HirError::Unsupported("method call (lowered in Task 4)".into()))
        }
        Suffix::Index(Index::Brackets { expression, .. }) => {
            let key = lower_expr(lowerer, expression)?;
            Ok(HirExpr::Index { obj: Box::new(current), key: Box::new(key) })
        }
        Suffix::Index(Index::Dot { name, .. }) => {
            let key = HirExpr::Literal(HirLiteral::String(name.token().to_string()));
            Ok(HirExpr::Index { obj: Box::new(current), key: Box::new(key) })
        }
        other => Err(HirError::Unsupported(format!("suffix form {other:?}"))),
    }
}
```

- [ ] **Step 6: Allow tables in call-args**

Replace `lower_call_args` in `crates/luau-hir/src/lower.rs`:

```rust
fn lower_call_args(
    lowerer: &mut Lowerer,
    args: &full_moon::ast::FunctionArgs,
) -> Result<Vec<HirExpr>, HirError> {
    use full_moon::ast::FunctionArgs;
    match args {
        FunctionArgs::Parentheses { arguments, .. } => arguments
            .iter()
            .map(|e| lower_expr(lowerer, e))
            .collect(),
        FunctionArgs::String(s) => {
            let raw = s.token().to_string();
            Ok(vec![HirExpr::Literal(HirLiteral::String(strip_string_quotes(&raw)?))])
        }
        FunctionArgs::TableConstructor(tc) => {
            Ok(vec![lower_table_ctor(lowerer, tc)?])
        }
        other => Err(HirError::Unsupported(format!("call args {other:?}"))),
    }
}
```

- [ ] **Step 7: Add tests at the bottom of `crates/luau-hir/src/lower.rs` (inside the `tests` mod)**

```rust
    #[test]
    fn lowers_empty_table() {
        let e = lower_one_expr("{}");
        let HirExpr::Table(entries) = e else { panic!() };
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn lowers_array_table() {
        let e = lower_one_expr("{1, 2, 3}");
        let HirExpr::Table(entries) = e else { panic!() };
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            assert!(matches!(entry, TableEntry::Array(_)));
        }
    }

    #[test]
    fn lowers_hash_table() {
        let e = lower_one_expr("{x = 1, y = 2}");
        let HirExpr::Table(entries) = e else { panic!() };
        assert_eq!(entries.len(), 2);
        assert!(matches!(&entries[0], TableEntry::Field(n, _) if n == "x"));
        assert!(matches!(&entries[1], TableEntry::Field(n, _) if n == "y"));
    }

    #[test]
    fn lowers_computed_key() {
        let e = lower_one_expr("{[1+2] = 99}");
        let HirExpr::Table(entries) = e else { panic!() };
        assert!(matches!(&entries[0], TableEntry::Keyed(_, _)));
    }

    #[test]
    fn lowers_dot_index() {
        let e = lower_one_expr("t.field");
        let HirExpr::Index { obj, key } = e else { panic!() };
        assert!(matches!(*obj, HirExpr::Symbol(_)));
        assert!(matches!(*key, HirExpr::Literal(HirLiteral::String(ref s)) if s == "field"));
    }

    #[test]
    fn lowers_bracket_index() {
        let e = lower_one_expr("t[42]");
        let HirExpr::Index { obj, key } = e else { panic!() };
        assert!(matches!(*obj, HirExpr::Symbol(_)));
        assert!(matches!(*key, HirExpr::Literal(HirLiteral::Number(n)) if n == 42.0));
    }

    #[test]
    fn lowers_chained_index() {
        let e = lower_one_expr("a.b.c");
        let HirExpr::Index { obj, key } = e else { panic!() };
        assert!(matches!(*key, HirExpr::Literal(HirLiteral::String(ref s)) if s == "c"));
        let HirExpr::Index { key: inner_key, .. } = *obj else { panic!() };
        assert!(matches!(*inner_key, HirExpr::Literal(HirLiteral::String(ref s)) if s == "b"));
    }

    #[test]
    fn table_arg_to_call() {
        let e = lower_one_expr("f{1,2}");
        let HirExpr::Call { args, .. } = e else { panic!() };
        assert_eq!(args.len(), 1);
        assert!(matches!(&args[0], HirExpr::Table(_)));
    }
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p luau-hir`
Expected: all new tests pass + Plan 1 tests still pass.

- [ ] **Step 9: Commit**

```bash
git add crates/luau-hir/src/types.rs crates/luau-hir/src/lower.rs
git commit -m "feat(hir): table constructors and indexed reads"
```

---

## Task 3: HIR — anonymous function expressions

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

`function(args) body end` as an expression. Plan 2 lowers it the same way `function name() end` lowered in Plan 1: a fresh scope, no upvalue capture.

- [ ] **Step 1: Replace the placeholder `E::Function(_)` arm in `lower_expr`**

In `crates/luau-hir/src/lower.rs`, find the arm added in Task 2:

```rust
        E::Function(_) => Err(HirError::Unsupported(
            "anonymous function expression (added in Task 3)".into(),
        )),
```

Replace it with:

```rust
        E::Function(anon) => {
            let function = lower_function_body(lowerer, anon.body())?;
            Ok(HirExpr::Function(function))
        }
```

**Note on full_moon shape:** `Expression::Function` carries a `(TokenReference, FunctionBody)` tuple in some versions and a `Box<(TokenReference, FunctionBody)>` in others. If `anon.body()` doesn't compile, try `&anon.1` or `&anon.0.1` and adapt. The semantic ask: get a `&full_moon::ast::FunctionBody`.

- [ ] **Step 2: Add tests at the bottom of `crates/luau-hir/src/lower.rs` (inside the `tests` mod)**

```rust
    #[test]
    fn lowers_anonymous_function() {
        let e = lower_one_expr("function(x) return x + 1 end");
        let HirExpr::Function(f) = e else { panic!() };
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.body.len(), 1);
    }

    #[test]
    fn lowers_anonymous_function_in_table() {
        let e = lower_one_expr("{ fn = function() return 7 end }");
        let HirExpr::Table(entries) = e else { panic!() };
        let TableEntry::Field(name, val) = &entries[0] else { panic!() };
        assert_eq!(name, "fn");
        assert!(matches!(val, HirExpr::Function(_)));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p luau-hir`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): anonymous function expressions"
```

---

## Task 4: HIR — method calls

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

Replace the `Suffix::Call(Call::MethodCall(_))` placeholder with real lowering. `a:b(args)` becomes `HirExpr::MethodCall { obj: a, method: "b", args }`.

- [ ] **Step 1: Replace the method-call arm in `lower_suffix`**

In `crates/luau-hir/src/lower.rs`, change the `Suffix::Call(Call::MethodCall(_))` arm to:

```rust
        Suffix::Call(Call::MethodCall(mc)) => {
            let method = mc.name().token().to_string();
            let args = lower_call_args(lowerer, mc.args())?;
            Ok(HirExpr::MethodCall {
                obj: Box::new(current),
                method,
                args,
            })
        }
```

- [ ] **Step 2: Add tests at the bottom of `crates/luau-hir/src/lower.rs`**

```rust
    #[test]
    fn lowers_method_call() {
        let e = lower_one_expr("obj:method(1, 2)");
        let HirExpr::MethodCall { obj, method, args } = e else { panic!() };
        assert!(matches!(*obj, HirExpr::Symbol(_)));
        assert_eq!(method, "method");
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn lowers_chained_method_call() {
        let e = lower_one_expr("a.b:c()");
        let HirExpr::MethodCall { obj, method, .. } = e else { panic!() };
        assert!(matches!(*obj, HirExpr::Index { .. }));
        assert_eq!(method, "c");
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p luau-hir`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): method calls (a:b())"
```

---

## Task 5: HIR — indexed assignment and `break` statement

**Files:**
- Modify: `crates/luau-hir/src/types.rs`
- Modify: `crates/luau-hir/src/lower.rs`

Two HIR statements: `t.x = v` (indexed assignment) and `break`.

- [ ] **Step 1: Extend `HirStmt` in `crates/luau-hir/src/types.rs`**

Replace the existing `HirStmt` enum with:

```rust
#[derive(Debug, Clone)]
pub enum HirStmt {
    /// `local x = expr` (or `local x` with `expr = Literal(Nil)`).
    LocalDecl { symbol: SymbolId, value: HirExpr },
    /// `name = expr` where `name` is a resolved symbol.
    Assign { target: SymbolId, value: HirExpr },
    /// `obj[key] = value` or `obj.name = value`.
    IndexAssign { obj: HirExpr, key: HirExpr, value: HirExpr },
    /// `f(args...)` as a statement (return value discarded).
    ExprStmt(HirExpr),
    /// `if cond then then_body [else else_body] end`.
    If { cond: HirExpr, then_body: Vec<HirStmt>, else_body: Vec<HirStmt> },
    /// `while cond do body end`
    While { cond: HirExpr, body: Vec<HirStmt> },
    /// `repeat body until cond`.
    Repeat { cond: HirExpr, body: Vec<HirStmt> },
    /// `for var = start, stop[, step] do body end`. `step` defaults to 1.0.
    NumericFor {
        var: SymbolId,
        start: HirExpr,
        stop: HirExpr,
        step: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `return [expr]` — Plan 1/2 support zero or one return value.
    Return(Option<HirExpr>),
    /// `break` out of the nearest enclosing loop.
    Break,
    /// Global function declaration: `function name(args) body end`.
    /// For dotted / method declarations, lowered to IndexAssign in `lower_stmt`.
    FunctionDecl { name: SymbolId, function: HirFunction },
}
```

- [ ] **Step 2: Refactor the assignment lowering to route through a helper that picks `Assign` vs `IndexAssign`**

In `crates/luau-hir/src/lower.rs`, replace the existing `Stmt::Assignment(a) => { ... }` arm of `lower_stmt` with:

```rust
        Stmt::Assignment(a) => {
            let vars: Vec<&full_moon::ast::Var> = a.variables().iter().collect();
            let exprs: Vec<&full_moon::ast::Expression> = a.expressions().iter().collect();
            if vars.len() != exprs.len() {
                return Err(HirError::Unsupported(
                    "multi-assign with mismatched rhs (Plan 1 has no multi-return)".into(),
                ));
            }
            let mut out = Vec::with_capacity(vars.len());
            for (var, expr) in vars.iter().zip(exprs.iter()) {
                let value = lower_expr(lowerer, expr)?;
                out.push(lower_assign_target(lowerer, var, value)?);
            }
            Ok(out)
        }
```

Then add `lower_assign_target` next to `lower_var` (above the `#[cfg(test)]` block):

```rust
fn lower_assign_target(
    lowerer: &mut Lowerer,
    var: &full_moon::ast::Var,
    value: HirExpr,
) -> Result<HirStmt, HirError> {
    use full_moon::ast::{Index, Prefix, Suffix, Var};
    match var {
        Var::Name(t) => {
            let target = lowerer.resolve(&t.token().to_string());
            Ok(HirStmt::Assign { target, value })
        }
        Var::Expression(ve) => {
            let suffixes: Vec<&Suffix> = ve.suffixes().collect();
            if suffixes.is_empty() {
                return Err(HirError::Unsupported("empty VarExpression in assign LHS".into()));
            }
            let (last, rest) = suffixes.split_last().unwrap();
            let mut obj = match ve.prefix() {
                Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())),
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
                other => {
                    return Err(HirError::Unsupported(format!(
                        "assign LHS last suffix must be index, got {other:?}"
                    )))
                }
            };
            Ok(HirStmt::IndexAssign { obj, key, value })
        }
        other => Err(HirError::Unsupported(format!("assign target form {other:?}"))),
    }
}
```

- [ ] **Step 3: Replace the `LastStmt::Break(_)` arm**

In `lower_last_stmt` in `crates/luau-hir/src/lower.rs`, find:

```rust
        LastStmt::Break(_) => Err(HirError::Unsupported("break (Plan 2)".into())),
```

Replace with:

```rust
        LastStmt::Break(_) => Ok(HirStmt::Break),
```

- [ ] **Step 4: Add tests at the bottom of `crates/luau-hir/src/lower.rs`**

```rust
    #[test]
    fn lowers_dot_assign() {
        let p = lower_str("t.x = 5");
        let HirStmt::IndexAssign { obj, key, value } = &p.main[0] else { panic!() };
        assert!(matches!(obj, HirExpr::Symbol(_)));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "x"));
        assert!(matches!(value, HirExpr::Literal(HirLiteral::Number(n)) if *n == 5.0));
    }

    #[test]
    fn lowers_bracket_assign() {
        let p = lower_str("t[1] = 5");
        let HirStmt::IndexAssign { key, .. } = &p.main[0] else { panic!() };
        assert!(matches!(key, HirExpr::Literal(HirLiteral::Number(n)) if *n == 1.0));
    }

    #[test]
    fn lowers_chained_assign() {
        let p = lower_str("a.b.c = 7");
        let HirStmt::IndexAssign { obj, key, .. } = &p.main[0] else { panic!() };
        // obj should be the index `a.b`; key should be "c".
        assert!(matches!(obj, HirExpr::Index { .. }));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "c"));
    }

    #[test]
    fn lowers_break_in_while() {
        let p = lower_str("while x do break end");
        let HirStmt::While { body, .. } = &p.main[0] else { panic!() };
        assert!(matches!(&body[0], HirStmt::Break));
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p luau-hir`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-hir/src/types.rs crates/luau-hir/src/lower.rs
git commit -m "feat(hir): indexed assignment and break"
```

---

## Task 6: HIR — function declarations with dotted and method names

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

`function a.b.c() end` is sugar for `a.b.c = function() end`. `function a:b() end` is sugar for `a.b = function(self, ...) end`. Lower both into `IndexAssign` of an `HirExpr::Function` value. The simple single-name case keeps using `FunctionDecl`.

- [ ] **Step 1: Replace the `Stmt::FunctionDeclaration` arm**

In `crates/luau-hir/src/lower.rs`, replace:

```rust
        Stmt::FunctionDeclaration(fd) => {
            let name_path: Vec<&full_moon::tokenizer::TokenReference> =
                fd.name().names().iter().collect();
            if name_path.len() != 1 || fd.name().method_colon().is_some() {
                return Err(HirError::Unsupported(
                    "qualified or method-style function decl (Plan 1: top-level only)".into(),
                ));
            }
            let name = name_path[0].token().to_string();
            let symbol = lowerer.resolve(&name);
            let function = lower_function_body(lowerer, fd.body())?;
            Ok(vec![HirStmt::FunctionDecl { name: symbol, function }])
        }
```

with:

```rust
        Stmt::FunctionDeclaration(fd) => lower_function_decl(lowerer, fd),
```

- [ ] **Step 2: Add the new lowering helper above the `#[cfg(test)]` block**

```rust
fn lower_function_decl(
    lowerer: &mut Lowerer,
    fd: &full_moon::ast::FunctionDeclaration,
) -> Result<Vec<HirStmt>, HirError> {
    let names: Vec<String> = fd.name().names().iter().map(|t| t.token().to_string()).collect();
    let method_name: Option<String> = fd.name().method_name().map(|t| t.token().to_string());

    let function = lower_function_body_with_self(lowerer, fd.body(), method_name.is_some())?;

    // Simple top-level decl: single name, no method colon.
    if names.len() == 1 && method_name.is_none() {
        let symbol = lowerer.resolve(&names[0]);
        return Ok(vec![HirStmt::FunctionDecl { name: symbol, function }]);
    }

    // Dotted / method form. Walk the head as a Symbol, intermediate names as
    // Index reads, then emit IndexAssign(obj, key, function-as-value).
    let head_sym = lowerer.resolve(&names[0]);
    let mut obj = HirExpr::Symbol(head_sym);

    let (intermediate_end, key) = if let Some(m) = method_name {
        // All of names[1..] are intermediate index reads; method name is the final key.
        (names.len(), m)
    } else {
        // names[1..len-1] are intermediate; names[len-1] is the final key.
        (names.len() - 1, names[names.len() - 1].clone())
    };

    for i in 1..intermediate_end {
        obj = HirExpr::Index {
            obj: Box::new(obj),
            key: Box::new(HirExpr::Literal(HirLiteral::String(names[i].clone()))),
        };
    }

    Ok(vec![HirStmt::IndexAssign {
        obj,
        key: HirExpr::Literal(HirLiteral::String(key)),
        value: HirExpr::Function(function),
    }])
}

/// Lower a function body, optionally prepending an implicit `self` parameter.
fn lower_function_body_with_self(
    lowerer: &mut Lowerer,
    body: &full_moon::ast::FunctionBody,
    is_method: bool,
) -> Result<HirFunction, HirError> {
    lowerer.push_scope();
    let mut params = Vec::new();
    if is_method {
        params.push(lowerer.declare_local("self"));
    }
    for p in body.parameters() {
        use full_moon::ast::Parameter;
        match p {
            Parameter::Name(tok) => {
                params.push(lowerer.declare_local(&tok.token().to_string()));
            }
            Parameter::Ellipsis(_) => {
                lowerer.pop_scope();
                return Err(HirError::Unsupported("varargs `...` (Plan 3)".into()));
            }
            other => {
                lowerer.pop_scope();
                return Err(HirError::Unsupported(format!("parameter form {other:?}")));
            }
        }
    }
    let body = lower_block(lowerer, body.block())?;
    lowerer.pop_scope();
    Ok(HirFunction { params, body })
}
```

- [ ] **Step 3: Make `lower_function_body` delegate to the new helper**

Keep the older `lower_function_body` callable by anonymous-function lowering. Replace its body so it just calls `lower_function_body_with_self(lowerer, body, false)`:

```rust
fn lower_function_body(
    lowerer: &mut Lowerer,
    body: &full_moon::ast::FunctionBody,
) -> Result<HirFunction, HirError> {
    lower_function_body_with_self(lowerer, body, false)
}
```

- [ ] **Step 4: Add tests at the bottom of `crates/luau-hir/src/lower.rs`**

```rust
    #[test]
    fn lowers_simple_function_decl_unchanged() {
        let p = lower_str("function f(x) return x end");
        assert!(matches!(&p.main[0], HirStmt::FunctionDecl { .. }));
    }

    #[test]
    fn lowers_dotted_function_decl() {
        let p = lower_str("function t.f(x) return x end");
        let HirStmt::IndexAssign { obj, key, value } = &p.main[0] else { panic!() };
        assert!(matches!(obj, HirExpr::Symbol(_)));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "f"));
        let HirExpr::Function(f) = value else { panic!() };
        assert_eq!(f.params.len(), 1);
    }

    #[test]
    fn lowers_method_function_decl_adds_self() {
        let p = lower_str("function obj:greet(name) return name end");
        let HirStmt::IndexAssign { key, value, .. } = &p.main[0] else { panic!() };
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "greet"));
        let HirExpr::Function(f) = value else { panic!() };
        // self prepended → 2 params total.
        assert_eq!(f.params.len(), 2);
    }

    #[test]
    fn lowers_deep_dotted_function_decl() {
        let p = lower_str("function a.b.c() end");
        let HirStmt::IndexAssign { obj, key, .. } = &p.main[0] else { panic!() };
        // obj is Index(Symbol(a), "b"); key is "c".
        let HirExpr::Index { key: inner_key, .. } = obj else { panic!() };
        assert!(matches!(inner_key.as_ref(), HirExpr::Literal(HirLiteral::String(s)) if s == "b"));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "c"));
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p luau-hir`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): dotted and method-style function declarations"
```

---

## Task 7: MIR — table instructions in types

**Files:**
- Modify: `crates/luau-mir/src/types.rs`

Three new instructions: `NewTable` (allocate an empty table), `GetIndex` (read), `SetIndex` (write).

- [ ] **Step 1: Extend `Instr` in `crates/luau-mir/src/types.rs`**

Replace the existing `Instr` enum with:

```rust
#[derive(Debug, Clone)]
pub enum Instr {
    /// Load a constant into a VLocal.
    LoadConst { dst: VLocal, src: ConstId },
    /// Copy one VLocal to another.
    Move { dst: VLocal, src: VLocal },
    /// Binary op on two values (non-short-circuit only; and/or are lowered to CFG).
    BinOp { dst: VLocal, op: BinOp, lhs: Value, rhs: Value },
    /// Unary op.
    UnOp { dst: VLocal, op: UnOp, operand: Value },
    /// Read a global by name. The name is a string constant in the pool.
    GetGlobal { dst: VLocal, name: ConstId },
    /// Write to a global.
    SetGlobal { name: ConstId, value: Value },
    /// Call a function: `dst = callee(args...)`. Plan 1/2 always take 1 result.
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Create a closure from a function id, write to dst. Plan 2: no upvalues.
    MakeClosure { dst: VLocal, function: FunctionId },
    /// Allocate a fresh empty table.
    NewTable { dst: VLocal },
    /// `dst = obj[key]`.
    GetIndex { dst: VLocal, obj: Value, key: Value },
    /// `obj[key] = value` — no destination.
    SetIndex { obj: Value, key: Value, value: Value },
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p luau-mir`
Expected: compiles. (Existing match expressions still cover all old variants; new ones are unhandled but no `match` is exhaustive yet on `Instr` outside the lowering helpers we'll modify in Tasks 9–10.)

If a downstream crate fails to compile because it has an exhaustive match on `Instr`, add `_ => {}` arms or list the new variants — the regalloc and LIR-lowering crates will be patched in Tasks 11–12, so any breakage there is fine for now provided it doesn't cascade to luau-mir's own tests.

- [ ] **Step 3: Run MIR tests**

Run: `cargo test -p luau-mir`
Expected: passes (no new MIR test yet, existing ones still green).

- [ ] **Step 4: Commit**

```bash
git add crates/luau-mir/src/types.rs
git commit -m "feat(mir): NewTable, GetIndex, SetIndex instruction types"
```

---

## Task 8: MIR — lower table, index, method, and anonymous function expressions

**Files:**
- Modify: `crates/luau-mir/src/lower.rs`

Extend `FnBuilder::lower_expr` to handle the new HIR variants.

- [ ] **Step 1: Replace `FnBuilder::lower_expr` in `crates/luau-mir/src/lower.rs`**

Find the existing `pub(crate) fn lower_expr(&mut self, e: &HirExpr) -> Result<VLocal, MirError>` method and replace it with:

```rust
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
                let mut arg_vs = Vec::with_capacity(args.len());
                for a in args {
                    arg_vs.push(Value::VLocal(self.lower_expr(a)?));
                }
                let dst = self.fresh_local();
                self.emit(Instr::Call {
                    dst: Some(dst),
                    callee: Value::VLocal(callee_v),
                    args: arg_vs,
                });
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
                // Evaluate obj into a single VLocal so it isn't re-evaluated for the lookup.
                let obj_v = self.lower_expr(obj)?;
                // Lookup obj[method_name] into fn_v.
                let key_const = self.intern_const(Constant::String(method.clone()));
                let key_v = self.fresh_local();
                self.emit(Instr::LoadConst { dst: key_v, src: key_const });
                let fn_v = self.fresh_local();
                self.emit(Instr::GetIndex {
                    dst: fn_v,
                    obj: Value::VLocal(obj_v),
                    key: Value::VLocal(key_v),
                });
                // Build the args list: obj is the implicit first arg.
                let mut arg_vs = Vec::with_capacity(args.len() + 1);
                arg_vs.push(Value::VLocal(obj_v));
                for a in args {
                    arg_vs.push(Value::VLocal(self.lower_expr(a)?));
                }
                let dst = self.fresh_local();
                self.emit(Instr::Call {
                    dst: Some(dst),
                    callee: Value::VLocal(fn_v),
                    args: arg_vs,
                });
                Ok(dst)
            }
            HirExpr::Function(f) => {
                let fid = self.queue_function(f);
                let dst = self.fresh_local();
                self.emit(Instr::MakeClosure { dst, function: fid });
                Ok(dst)
            }
        }
    }

    fn lower_table(&mut self, entries: &[luau_hir::TableEntry]) -> Result<VLocal, MirError> {
        let dst = self.fresh_local();
        self.emit(Instr::NewTable { dst });
        let mut array_idx: i64 = 1;
        for entry in entries {
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
        Ok(dst)
    }
```

`HirFunction` references inside `Function(f)` — `queue_function` already accepts `&HirFunction`. No signature change required there.

- [ ] **Step 2: Run MIR tests**

Run: `cargo test -p luau-mir`
Expected: existing tests pass; the new variants compile.

- [ ] **Step 3: Add a smoke test at the bottom of `crates/luau-mir/src/lower.rs`'s `tests` module**

```rust
    #[test]
    fn lowers_empty_table_to_new_table() {
        let p = mir_of("local t = {}");
        let ops: Vec<&str> = p
            .main()
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .map(|i| match i {
                Instr::NewTable { .. } => "NewTable",
                Instr::LoadConst { .. } => "LoadConst",
                Instr::Move { .. } => "Move",
                _ => "other",
            })
            .collect();
        assert!(ops.contains(&"NewTable"));
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
```

- [ ] **Step 4: Run MIR tests**

Run: `cargo test -p luau-mir`
Expected: new + old tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-mir/src/lower.rs
git commit -m "feat(mir): lower table, index, method-call, and anonymous fn expressions"
```

---

## Task 9: MIR — lower `IndexAssign` and `Break`, with loop-exit tracking

**Files:**
- Modify: `crates/luau-mir/src/lower.rs`
- Modify: `crates/luau-mir/src/lower_stmts.rs`

`break` targets the exit block of the innermost loop. `FnBuilder` carries a stack of exit `BlockId`s, pushed at the start of each loop body lowering and popped on the way out.

- [ ] **Step 1: Add the `loop_exits` field to `FnBuilder`**

In `crates/luau-mir/src/lower.rs`, in the `FnBuilder` struct definition, add a new field:

```rust
pub(crate) struct FnBuilder<'a> {
    pub(crate) _id: FunctionId,
    pub(crate) next_local: u32,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) next_block: u32,
    pub(crate) current: Option<BlockId>,
    pub(crate) consts: Vec<Constant>,
    pub(crate) symbols: SymbolMap,
    pub(crate) all_symbols: &'a [Symbol],
    pub(crate) pending_functions: &'a mut Vec<(FunctionId, HirFunction)>,
    pub(crate) next_function: &'a mut u32,
    pub(crate) params: Vec<VLocal>,
    /// Stack of innermost-first loop-exit blocks. `Break` jumps to the top.
    pub(crate) loop_exits: Vec<BlockId>,
}
```

Then in `FnBuilder::new`, add the field initializer at the bottom of the struct expression: `loop_exits: Vec::new(),`. The full `new` becomes:

```rust
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
```

- [ ] **Step 2: Wrap loop body lowering in `crates/luau-mir/src/lower_stmts.rs`**

Replace the `HirStmt::While { ... }`, `HirStmt::Repeat { ... }`, and `HirStmt::NumericFor { ... }` arms of `lower_stmt` to push/pop `loop_exits` around their body lowering.

Replace the `While` arm with:

```rust
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
```

Replace the `Repeat` arm with:

```rust
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
```

Replace the `NumericFor` arm with:

```rust
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
```

- [ ] **Step 3: Add the `Break` and `IndexAssign` arms to `lower_stmt`**

Inside `lower_stmt` in `crates/luau-mir/src/lower_stmts.rs`, add new arms (just before the closing `}` of the `match stmt`):

```rust
        HirStmt::Break => {
            let exit = *b
                .loop_exits
                .last()
                .ok_or_else(|| MirError::Unsupported("break outside loop".into()))?;
            b.set_terminator(Terminator::Goto(exit));
            // Switch into a dead block so any trailing instructions (none expected
            // after `break` as a last_stmt, but defensive) have somewhere to go.
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
```

- [ ] **Step 4: Run MIR tests**

Run: `cargo test -p luau-mir`
Expected: existing tests still green; new tests below will be added next step.

- [ ] **Step 5: Add tests at the bottom of the `tests` module in `crates/luau-mir/src/lower.rs`**

```rust
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
    fn break_in_while_jumps_to_exit() {
        let p = mir_of("while true do break end");
        // The body block's terminator should be a Goto to a block that the
        // header also targets as `else_block`.
        let func = p.main();
        let header_else = func
            .blocks
            .iter()
            .find_map(|bb| match &bb.terminator {
                Terminator::Branch { else_block, .. } => Some(*else_block),
                _ => None,
            })
            .expect("found while header");
        let body_break = func
            .blocks
            .iter()
            .find_map(|bb| match &bb.terminator {
                Terminator::Goto(target)
                    if *target == header_else && !bb.instrs.is_empty() || *target == header_else =>
                {
                    Some(*target)
                }
                _ => None,
            });
        assert!(body_break.is_some(), "expected a Goto to the loop exit");
    }
```

(The second test is intentionally loose — it just asserts a Goto target equal to the while-header's else_block exists somewhere in the function, which is what `break` produces.)

- [ ] **Step 6: Run MIR tests**

Run: `cargo test -p luau-mir`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-mir/src/lower.rs crates/luau-mir/src/lower_stmts.rs
git commit -m "feat(mir): lower IndexAssign and break, with loop-exit tracking"
```

---

## Task 10: LIR — table opcodes and regalloc

**Files:**
- Modify: `crates/luau-lir/src/types.rs`
- Modify: `crates/luau-lir/src/regalloc.rs`

Add the three new opcodes and teach regalloc about the new MIR instructions.

- [ ] **Step 1: Extend `OpKind` in `crates/luau-lir/src/types.rs`**

Replace the existing `OpKind` enum with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    LoadNil,
    LoadTrue,
    LoadFalse,
    LoadConst,
    Move,
    Add, Sub, Mul, Div, Mod, Pow,
    Concat,
    Lt, Le, Eq,
    Not, Neg, Len,
    GetGlobal,
    SetGlobal,
    Call,
    Return,
    Jmp,
    JmpIfTrue,
    JmpIfFalse,
    Closure,
    NewTable,
    GetTable,
    SetTable,
}
```

- [ ] **Step 2: Update `visit_vlocals` in `crates/luau-lir/src/regalloc.rs`**

Replace the existing `visit_vlocals` function:

```rust
fn visit_vlocals(instr: &luau_mir::Instr, mut f: impl FnMut(VLocal)) {
    use luau_mir::{Instr, Value};
    fn visit_value(v: Value, f: &mut impl FnMut(VLocal)) {
        if let Value::VLocal(l) = v {
            f(l);
        }
    }
    match instr {
        Instr::LoadConst { dst, .. } => f(*dst),
        Instr::Move { dst, src } => { f(*dst); f(*src); }
        Instr::BinOp { dst, lhs, rhs, .. } => {
            f(*dst);
            visit_value(*lhs, &mut f);
            visit_value(*rhs, &mut f);
        }
        Instr::UnOp { dst, operand, .. } => {
            f(*dst);
            visit_value(*operand, &mut f);
        }
        Instr::GetGlobal { dst, .. } => f(*dst),
        Instr::SetGlobal { value, .. } => visit_value(*value, &mut f),
        Instr::Call { dst, callee, args } => {
            if let Some(d) = dst { f(*d); }
            visit_value(*callee, &mut f);
            for a in args { visit_value(*a, &mut f); }
        }
        Instr::MakeClosure { dst, .. } => f(*dst),
        Instr::NewTable { dst } => f(*dst),
        Instr::GetIndex { dst, obj, key } => {
            f(*dst);
            visit_value(*obj, &mut f);
            visit_value(*key, &mut f);
        }
        Instr::SetIndex { obj, key, value } => {
            visit_value(*obj, &mut f);
            visit_value(*key, &mut f);
            visit_value(*value, &mut f);
        }
    }
}
```

- [ ] **Step 3: Verify**

Run: `cargo check -p luau-lir`
Expected: compiles.

Run: `cargo test -p luau-lir`
Expected: existing tests pass; LIR lowering for the new variants will fail to compile until Task 11, so this should still pass (regalloc tests don't exercise the lower path).

If `cargo test -p luau-lir` reports failures in `lower.rs` because of an exhaustive match, that's OK — Task 11 immediately follows. You can skip to Task 11 and verify both together. (No commit between Tasks 10 and 11 if so.)

- [ ] **Step 4: Commit (only if `cargo check` passes; otherwise bundle with Task 11)**

```bash
git add crates/luau-lir/src/types.rs crates/luau-lir/src/regalloc.rs
git commit -m "feat(lir): table opcodes and regalloc visitors"
```

---

## Task 11: LIR — lower table instructions

**Files:**
- Modify: `crates/luau-lir/src/lower.rs`

Handle the three new MIR instructions.

- [ ] **Step 1: Add arms to `lower_instr` in `crates/luau-lir/src/lower.rs`**

Find the `match instr { ... }` inside `fn lower_instr`. Inside the match, just before its closing `}`, add:

```rust
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
```

- [ ] **Step 2: Add a test at the bottom of `tests` mod in `crates/luau-lir/src/lower.rs`**

```rust
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
```

- [ ] **Step 3: Run LIR tests**

Run: `cargo test -p luau-lir`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-lir/src/lower.rs
git commit -m "feat(lir): lower NewTable, GetIndex, SetIndex to opcodes"
```

---

## Task 12: Emit — opmap and render entries for the new ops

**Files:**
- Modify: `crates/luau-emit/src/opmap.rs`
- Modify: `crates/luau-emit/src/render.rs`

Plan 1's `OpMap.forward` is sized `[u8; 26]` and `ALL_OPS` has 26 entries. We need 29 (three new opcodes).

- [ ] **Step 1: Update `crates/luau-emit/src/opmap.rs`**

Replace the file's contents with:

```rust
//! Maps symbolic OpKinds to integer opcode bytes.
//!
//! In Plan 2 the mapping is still deterministic and seed-independent.
//! Plan 3+ introduces per-build randomization: keep the API stable so we
//! just swap implementations.

use luau_lir::OpKind;

pub struct OpMap {
    /// table[OpKind index] = opcode byte
    forward: [u8; 29],
}

impl OpMap {
    pub fn new(_seed: &[u8; 32]) -> Self {
        let mut forward = [0u8; 29];
        for (i, slot) in forward.iter_mut().enumerate() {
            *slot = (i + 1) as u8;
        }
        OpMap { forward }
    }

    pub fn opcode_of(&self, k: OpKind) -> u8 {
        let idx = op_index(k);
        self.forward[idx]
    }

    pub fn assigned_pairs(&self) -> Vec<(OpKind, u8)> {
        ALL_OPS.iter().map(|k| (*k, self.opcode_of(*k))).collect()
    }
}

pub const ALL_OPS: &[OpKind] = &[
    OpKind::LoadNil, OpKind::LoadTrue, OpKind::LoadFalse, OpKind::LoadConst,
    OpKind::Move, OpKind::Add, OpKind::Sub, OpKind::Mul, OpKind::Div, OpKind::Mod,
    OpKind::Pow, OpKind::Concat, OpKind::Lt, OpKind::Le, OpKind::Eq,
    OpKind::Not, OpKind::Neg, OpKind::Len,
    OpKind::GetGlobal, OpKind::SetGlobal, OpKind::Call, OpKind::Return,
    OpKind::Jmp, OpKind::JmpIfTrue, OpKind::JmpIfFalse, OpKind::Closure,
    OpKind::NewTable, OpKind::GetTable, OpKind::SetTable,
];

fn op_index(k: OpKind) -> usize {
    ALL_OPS.iter().position(|x| *x == k).expect("known op")
}
```

- [ ] **Step 2: Update `opname` in `crates/luau-emit/src/render.rs`**

Find the `fn opname(k: OpKind) -> &'static str { match k { ... } }` function. Replace it with:

```rust
fn opname(k: OpKind) -> &'static str {
    match k {
        OpKind::LoadNil => "LoadNil",
        OpKind::LoadTrue => "LoadTrue",
        OpKind::LoadFalse => "LoadFalse",
        OpKind::LoadConst => "LoadConst",
        OpKind::Move => "Move",
        OpKind::Add => "Add", OpKind::Sub => "Sub", OpKind::Mul => "Mul",
        OpKind::Div => "Div", OpKind::Mod => "Mod", OpKind::Pow => "Pow",
        OpKind::Concat => "Concat",
        OpKind::Lt => "Lt", OpKind::Le => "Le", OpKind::Eq => "Eq",
        OpKind::Not => "Not", OpKind::Neg => "Neg", OpKind::Len => "Len",
        OpKind::GetGlobal => "GetGlobal", OpKind::SetGlobal => "SetGlobal",
        OpKind::Call => "Call", OpKind::Return => "Return",
        OpKind::Jmp => "Jmp",
        OpKind::JmpIfTrue => "JmpIfTrue", OpKind::JmpIfFalse => "JmpIfFalse",
        OpKind::Closure => "Closure",
        OpKind::NewTable => "NewTable",
        OpKind::GetTable => "GetTable",
        OpKind::SetTable => "SetTable",
    }
}
```

- [ ] **Step 3: Run emit tests**

Run: `cargo test -p luau-emit`
Expected: existing tests pass. The VM template (still 26-op) renders fine because minijinja's `{% for opname, num in opcodes %}` loop expands to whatever `opcodes` carries — the chunk just adds three more `local OP_*` lines; the VM handlers come next.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit/src/opmap.rs crates/luau-emit/src/render.rs
git commit -m "feat(emit): register NewTable, GetTable, SetTable opcodes"
```

---

## Task 13: VM template — handlers for `NewTable`, `GetTable`, `SetTable`

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

Three new dispatch arms.

- [ ] **Step 1: Add handlers to the VM template**

In `crates/luau-runtime/assets/vm.luau.j2`, find the long `if op == OP_LoadNil then ... elseif op == OP_Closure then ... else _error(...)` chain. Right before the final `else _error("luau-obf: unknown opcode " .. _tostring(op))`, insert three new arms:

```luau
        elseif op == OP_NewTable then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = {}
        elseif op == OP_GetTable then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b][regs[c]]
        elseif op == OP_SetTable then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a][regs[b]] = regs[c]
```

- [ ] **Step 2: Run the full workspace tests**

Run: `cargo test --workspace --release`
Expected: all unit tests pass. The differential harness (if `luau` is on PATH) will run the Plan 1 corpus; existing programs don't use tables yet, so they should still work.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-runtime/assets/vm.luau.j2
git commit -m "feat(runtime): VM handlers for NewTable, GetTable, SetTable"
```

---

## Task 14: Corpus — differential tests for tables, methods, break

**Files:**
- Create: `runtime-tests/corpus/tables.luau`
- Create: `runtime-tests/corpus/methods.luau`
- Create: `runtime-tests/corpus/break.luau`

Three programs that exercise the new features end-to-end through the obfuscation pipeline.

- [ ] **Step 1: Create `runtime-tests/corpus/tables.luau`**

```luau
local t = {1, 2, 3}
print(t[1] + t[2] + t[3])
print(#t)
t[4] = 10
print(#t)
print(t[4])

local h = {x = 5, y = 7}
print(h.x + h.y)
h.z = h.x * h.y
print(h.z)

local mixed = {10, x = 20, 30}
print(mixed[1])
print(mixed[2])
print(mixed.x)

local k = "key"
local map = {[k] = 42}
print(map[k])
print(map["key"])
```

- [ ] **Step 2: Create `runtime-tests/corpus/methods.luau`**

```luau
local point = {x = 1, y = 2}
function point:magnitude()
    return self.x * self.x + self.y * self.y
end
function point:scale(k)
    self.x = self.x * k
    self.y = self.y * k
end
print(point:magnitude())
point:scale(3)
print(point.x)
print(point.y)
print(point:magnitude())

local counter = {n = 0}
function counter.inc(self)
    self.n = self.n + 1
end
counter.inc(counter)
counter.inc(counter)
counter.inc(counter)
print(counter.n)
```

- [ ] **Step 3: Create `runtime-tests/corpus/break.luau`**

```luau
local sum = 0
for i = 1, 100 do
    if i > 10 then break end
    sum = sum + i
end
print(sum)

local found = -1
local i = 1
while true do
    if i * i > 50 then
        found = i
        break
    end
    i = i + 1
end
print(found)

local j = 0
repeat
    j = j + 1
    if j >= 7 then break end
until j >= 100
print(j)
```

- [ ] **Step 4: Run the differential harness**

Run: `cargo test --workspace --release`
Expected: all corpus programs (Plan 1 + Plan 2 new) pass the differential check.

If `luau` isn't on PATH, the integration test is skipped; you can still run it directly:

```bash
cargo run -p runtime-tests --release
```

This should print `PASS  tables.luau`, `PASS  methods.luau`, `PASS  break.luau`, plus the existing Plan 1 corpus, all green.

- [ ] **Step 5: Commit**

```bash
git add runtime-tests/corpus/tables.luau runtime-tests/corpus/methods.luau runtime-tests/corpus/break.luau
git commit -m "test(corpus): tables, methods, and break programs"
```

---

## Task 15: README — bump status

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the status line in `README.md`**

Replace:

```
**Status:** Plan 1 — skeleton & identity pipeline. The obfuscator round-trips
a meaningful Luau subset through a custom VM with no obfuscation passes yet.
```

with:

```
**Status:** Plan 2 — tables, methods, and `break`. The obfuscator round-trips
an expanded Luau subset (tables, indexed access, method calls, dotted/method
function declarations, anonymous function expressions, `break`) through a
custom VM with no obfuscation passes yet.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 2 status"
```

---

## Verification checklist (run before declaring Plan 2 done)

- [ ] `cargo test --workspace --release` passes end-to-end.
- [ ] `cargo run -p runtime-tests --release` shows all corpus programs (Plan 1 + Plan 2) green.
- [ ] `cargo clippy --workspace --release -- -D warnings` clean (or only warnings already present from Plan 1).
- [ ] Determinism: re-running `cargo run -p luau-obf-cli -- runtime-tests/corpus/tables.luau --seed 0000000000000000000000000000000000000000000000000000000000000000 -o /tmp/a.luau` twice produces byte-identical output.
- [ ] HIR/MIR/LIR scope notes in code comments still reference the current plan correctly; no leftover `Plan 1` deferral comments for things that are now in.
