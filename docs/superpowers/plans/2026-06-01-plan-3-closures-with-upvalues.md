# Luau Obfuscator — Plan 3: Closures with Upvalues

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lift Plan 2's "function bodies cannot reference parent locals" restriction by implementing proper Lua-style upvalue capture, plus the `local function f() … end` form. After this plan, closures correctly capture and share parent-frame locals, including mutual sharing across multiple closures over the same variable.

**Architecture:**
- HIR gains `HirExpr::Upvalue(u32)` and `HirStmt::UpvalueAssign { upvalue, value }`. `HirFunction` gains a `upvalues: Vec<UpvalueSource>` field. Each `UpvalueSource` is either `ParentLocal(SymbolId)` or `ParentUpval(u32)`. The HIR lowerer detects cross-function-frame references and propagates the capture through every intermediate function as needed.
- MIR adds `Instr::GetUpval { dst, idx }` and `Instr::SetUpval { idx, value }`. `Instr::MakeClosure` grows a `upvalues: Vec<UpvalueSource>` field carrying the same structural info as HIR.
- LIR adds `OpKind::GetUpval`, `SetUpval`, and renames the existing `OpKind::Closure` semantics: closures now carry per-build upvalue source operands. The bytecode encoding gains a variable-length upvalue list immediately after the proto id.
- The VM template's closure values become `{ proto = id, upvals = array }`. Each upval slot is a `{ tbl, key }` cell — a reference into some frame's `regs` or some closure's `upvals`. `MakeClosure` builds the upvals from the current frame's regs/upvals according to the bytecode-encoded sources. `GetUpval` reads through the cell; `SetUpval` writes.

**Tech Stack:** Same as Plans 1-2.

**Plan 3 language additions:**

| Newly supported | Still deferred |
|---|---|
| `local function f() … end` (with body that may reference `f` recursively) | Multi-return, varargs (Plan 4) |
| `local f = function() return x end` capturing parent locals | `for k, v in pairs(t)` generalized iteration (Plan 4) |
| Multiple closures sharing a captured local: `local i; local g = function() i = i + 1 end; local h = function() return i end` — both see the same `i` | `continue`, compound assign, string interp (Plan 5) |
| Deeply nested capture: `function a() local x; return function() return function() return x end end end` | Metatables, vector/buffer/bit32 (Plan 6+) |

**Out-of-scope reminder:** Multi-return and varargs stay deferred. `for-in` loops therefore stay deferred (they need multi-return for the iterator triple).

---

## Implementation notes

**Capture model.** Standard Lua upvalue semantics: when a closure references a name from an outer function, the closure captures a *cell* — a `{ tbl, key }` pair pointing into the outer frame's locals. All closures over the same local share the same cell. Mutations propagate. The cell stays alive as long as any closure holds it.

**Chained capture.** If function `C` (nested in `B`, nested in `A`) captures `A`'s local `x`, then `B` also has an upvalue for `x` (because closures only see the immediately enclosing frame's regs/upvals at MakeClosure time). The HIR lowerer ensures the propagation: when resolving `x` in `C`, it walks up the function frames, adding an upvalue at every intermediate level.

**local function.** Sugar for `local f; f = function() … end` — but with an important subtlety: in Lua, `local function f` makes `f` visible *inside* the function's own body (for recursion), whereas `local f = function() … end` does not. We handle this at the HIR layer: declare `f` before lowering the body.

**Plan 2's capture check.** Plan 2 added a guard in `Lowerer::resolve` that errors when a name resolves to a Local declared in a shallower function frame. Plan 3 removes that guard and replaces it with real upvalue construction.

**Existing scope-state pre-conditions.** Plan 2 added `scope_function_depth: Vec<u32>` and `current_function_depth: u32` to `Lowerer`. Plan 3 adds a sibling `function_frames: Vec<FunctionFrame>` stack that tracks per-function upvalue lists.

---

## File Structure (changes only)

```
crates/luau-hir/src/
  types.rs            # MODIFY: UpvalueSource, HirFunction.upvalues, HirExpr::Upvalue, HirStmt::UpvalueAssign
  lower.rs            # MODIFY: function_frames stack, resolve → may upgrade to upvalue,
                      #         local function support, assignment routing for upvalues
crates/luau-mir/src/
  types.rs            # MODIFY: MirFunction.upvalues, Instr::GetUpval/SetUpval, MakeClosure.upvalues
  lower.rs            # MODIFY: track current function's upvalues; lower HirExpr::Upvalue;
                      #         build MakeClosure with upvalue sources
  lower_stmts.rs      # MODIFY: HirStmt::UpvalueAssign emits SetUpval
crates/luau-lir/src/
  types.rs            # MODIFY: OpKind::GetUpval, SetUpval; LirInstr can carry Operand::UpvalIdx;
                      #         LirFunction.num_upvals
  regalloc.rs         # MODIFY: visit_vlocals for GetUpval/SetUpval/MakeClosure-with-upvals
  lower.rs            # MODIFY: lower GetUpval/SetUpval; Closure carries upvalue source operands
crates/luau-emit/src/
  opmap.rs            # MODIFY: ALL_OPS grows; forward[N] size
  render.rs           # MODIFY: opname() entries; meta passes num_upvals
  encode.rs           # MODIFY: Closure instr has variable-length upvalue source list
crates/luau-runtime/assets/
  vm.luau.j2          # MODIFY: closure value layout; GetUpval/SetUpval handlers;
                      #         Closure handler builds upvals array
runtime-tests/corpus/
  closures.luau       # CREATE
  counter.luau        # CREATE
  nested_closures.luau # CREATE
README.md             # MODIFY: status line
```

---

## Task 1: HIR types — Upvalue and capture sources

**Files:**
- Modify: `crates/luau-hir/src/types.rs`

- [ ] **Step 1: Add `UpvalueSource` and extend `HirFunction` in `crates/luau-hir/src/types.rs`**

Add near `TableEntry`:

```rust
/// How a function's upvalue is sourced from its immediately-enclosing frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpvalueSource {
    /// The parent function's local (by SymbolId).
    ParentLocal(SymbolId),
    /// The parent function's upvalue (by index into its upvalues vec).
    ParentUpval(u32),
}
```

Replace `HirFunction` to add the upvalues field:

```rust
#[derive(Debug, Clone)]
pub struct HirFunction {
    pub params: Vec<SymbolId>,
    pub body: Vec<HirStmt>,
    /// Upvalue sources, in declaration order. The index in this vec is the
    /// upvalue identifier used by `HirExpr::Upvalue` and `HirStmt::UpvalueAssign`
    /// inside this function's body.
    pub upvalues: Vec<UpvalueSource>,
}
```

- [ ] **Step 2: Add upvalue variants to `HirExpr` and `HirStmt`**

In `HirExpr`, add (after `Function(HirFunction)`):

```rust
    /// Read this function's upvalue at the given index.
    Upvalue(u32),
```

In `HirStmt`, add (next to `Assign`):

```rust
    /// Write this function's upvalue at the given index.
    UpvalueAssign { upvalue: u32, value: HirExpr },
```

- [ ] **Step 3: Verify build**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo check -p luau-hir`
Expected: compiles. Downstream crates may fail if they exhaustively match HirExpr / HirStmt — that's expected. Tasks 4-6 fill them in.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir/src/types.rs
git commit -m "feat(hir): UpvalueSource, HirExpr::Upvalue, HirStmt::UpvalueAssign types"
```

---

## Task 2: HIR — capture analysis in lowerer

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

This is the big task: replace Plan 2's "error on cross-function-frame local capture" with real upvalue propagation.

- [ ] **Step 1: Add per-function-frame state to `Lowerer`**

Replace the `Lowerer` struct with:

```rust
struct Lowerer {
    symbols: Vec<Symbol>,
    /// Stack of lexical scopes. Each scope maps source name → SymbolId for locals.
    scopes: Vec<HashMap<String, SymbolId>>,
    /// Parallel to `scopes`: function-nesting depth of each scope.
    scope_function_depth: Vec<u32>,
    /// The function-nesting depth currently being lowered.
    current_function_depth: u32,
    /// Stack of function frames, deepest last. `function_frames.len() == current_function_depth + 1`.
    function_frames: Vec<FunctionFrame>,
}

#[derive(Default)]
struct FunctionFrame {
    /// Upvalues defined by THIS function (their declaration order matches the
    /// HirFunction.upvalues vec we'll build at function-finalization time).
    upvalues: Vec<UpvalueSource>,
    /// Quick lookup: source SymbolId (the original Local in some ancestor frame)
    /// → upvalue index in THIS frame. Used to deduplicate captures.
    /// For ParentLocal sources we key on the original Local SymbolId. For
    /// ParentUpval we key on the *parent frame's* upvalue index (encoded as
    /// a synthetic key — see SourceKey below).
    by_source: HashMap<SourceKey, u32>,
}

/// Key for FunctionFrame.by_source. Discriminates Local vs Upval to keep keys unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SourceKey {
    /// The original Local SymbolId in some ancestor frame.
    Local(SymbolId),
    /// A specific frame's specific upvalue slot.
    /// (frame_depth, parent_upval_idx) — frame_depth is the *parent* frame's depth.
    Upval(u32, u32),
}
```

- [ ] **Step 2: Update `Lowerer::new`**

```rust
    fn new() -> Self {
        Self {
            symbols: Vec::new(),
            scopes: vec![HashMap::new()],
            scope_function_depth: vec![0],
            current_function_depth: 0,
            function_frames: vec![FunctionFrame::default()],
        }
    }
```

- [ ] **Step 3: Update `enter_function` / `exit_function`**

```rust
    fn enter_function(&mut self) {
        self.current_function_depth += 1;
        self.push_scope();
        self.function_frames.push(FunctionFrame::default());
    }

    fn exit_function(&mut self) -> FunctionFrame {
        self.pop_scope();
        self.current_function_depth -= 1;
        self.function_frames.pop().expect("function_frames balanced")
    }
```

Note: `exit_function` now returns the popped frame so the caller can extract the upvalues list.

- [ ] **Step 4: Replace `resolve` with the upvalue-propagating version**

Resolve returns one of: a local SymbolId, an upvalue index in the current frame, or a global SymbolId. We unify the return type so callers don't have to know which:

```rust
/// Resolution outcome — either a direct symbol reference or an upvalue read.
#[derive(Debug, Clone, Copy)]
enum Resolved {
    /// A local of the current function frame, OR a global SymbolId.
    Symbol(SymbolId),
    /// An upvalue of the current function frame, at this index.
    Upvalue(u32),
}

impl Lowerer {
    fn resolve(&mut self, name: &str) -> Resolved {
        for i in (0..self.scopes.len()).rev() {
            if let Some(&id) = self.scopes[i].get(name) {
                let scope_depth = self.scope_function_depth[i];
                if scope_depth == self.current_function_depth {
                    return Resolved::Symbol(id);
                }
                // Cross-frame: thread an upvalue through every function frame
                // between `scope_depth` and `current_function_depth`.
                let idx = self.thread_upvalue(scope_depth, id);
                return Resolved::Upvalue(idx);
            }
        }
        Resolved::Symbol(self.declare_global(name))
    }

    /// Ensure each function frame between `source_depth+1` and `current_function_depth`
    /// has an upvalue capturing `source_local`. Returns the upvalue index in the
    /// CURRENT (deepest) frame.
    fn thread_upvalue(&mut self, source_depth: u32, source_local: SymbolId) -> u32 {
        // Walk shallow → deep, propagating.
        let mut last_idx: Option<u32> = None;
        for depth in (source_depth + 1)..=self.current_function_depth {
            // function_frames is indexed by depth (top-level is depth 0).
            let frame_idx = depth as usize;
            let key = match last_idx {
                None => SourceKey::Local(source_local),
                Some(parent_idx) => SourceKey::Upval(depth - 1, parent_idx),
            };
            let frame = &mut self.function_frames[frame_idx];
            let idx = if let Some(&existing) = frame.by_source.get(&key) {
                existing
            } else {
                let new_idx = frame.upvalues.len() as u32;
                let source = match last_idx {
                    None => UpvalueSource::ParentLocal(source_local),
                    Some(parent_idx) => UpvalueSource::ParentUpval(parent_idx),
                };
                frame.upvalues.push(source);
                frame.by_source.insert(key, new_idx);
                new_idx
            };
            last_idx = Some(idx);
        }
        last_idx.expect("at least one frame above source")
    }
}
```

- [ ] **Step 5: Update all `resolve` call sites**

Now `resolve` returns `Resolved`, not `Result<SymbolId, HirError>`. Every call site needs to handle both variants. There are roughly seven of them (per Plan 2's fix commit). Pattern:

```rust
// Old (Plan 2):
let target = lowerer.resolve(&t.token().to_string())?;
// New (Plan 3):
let resolved = lowerer.resolve(&t.token().to_string());
let target = match resolved { Resolved::Symbol(s) => s, Resolved::Upvalue(_) => panic!("not in assign target") };
```

But not every site can ignore upvalues. The four contexts:

1. **Variable read in expression** (`lower_var`, `lower_var_expression`, `lower_call`'s prefix, `lower_assign_target`'s `Var::Expression` prefix): convert `Resolved::Upvalue(idx)` into `HirExpr::Upvalue(idx)`.

2. **Assignment target — simple name** (`lower_assign_target`'s `Var::Name`): convert `Resolved::Upvalue(idx)` into `HirStmt::UpvalueAssign`. Otherwise `HirStmt::Assign`.

3. **Function declaration target name** (`lower_function_decl`'s head): if the name resolves to a `Resolved::Upvalue`, the user is writing `function f.x()` where `f` is an upvalue — that's a write through the upvalue. Emit an `IndexAssign` whose `obj` is `HirExpr::Upvalue(idx)`. For the simple single-name case (`function f()`), it becomes `UpvalueAssign { upvalue: idx, value: Function(...) }`.

4. **Symbol reference (already resolved)**: doesn't apply; resolve only happens at lookup time.

Add a tiny helper to convert a Resolved to an HirExpr (lvalue contexts handle this differently):

```rust
fn resolved_to_expr(r: Resolved) -> HirExpr {
    match r {
        Resolved::Symbol(s) => HirExpr::Symbol(s),
        Resolved::Upvalue(idx) => HirExpr::Upvalue(idx),
    }
}
```

Use this in `lower_var` (Var::Name), `lower_var_expression` (prefix Name), `lower_call` (prefix Name), and `lower_assign_target` (Var::Expression prefix Name).

For `lower_assign_target`'s Var::Name arm, branch:

```rust
Var::Name(t) => match lowerer.resolve(&t.token().to_string()) {
    Resolved::Symbol(target) => Ok(HirStmt::Assign { target, value }),
    Resolved::Upvalue(idx) => Ok(HirStmt::UpvalueAssign { upvalue: idx, value }),
}
```

For `lower_function_decl`:
- Simple single-name no-method case: resolve the name; if Upvalue, emit UpvalueAssign with the function as value. If Symbol, keep existing FunctionDecl path.
- Dotted/method case: resolve the head; convert via `resolved_to_expr`; rest of logic unchanged.

- [ ] **Step 6: Use the captured upvalues in `lower_function_body_with_self`**

Replace the function so it captures the `FunctionFrame` that comes back from `exit_function` and stuffs `upvalues` into the returned `HirFunction`:

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
    for p in body.parameters() {
        use full_moon::ast::Parameter;
        match p {
            Parameter::Name(tok) => {
                params.push(lowerer.declare_local(&tok.token().to_string()));
            }
            Parameter::Ellipsis(_) => {
                lowerer.exit_function();
                return Err(HirError::Unsupported("varargs `...` (Plan 4)".into()));
            }
            other => {
                lowerer.exit_function();
                return Err(HirError::Unsupported(format!("parameter form {other:?}")));
            }
        }
    }
    let body = lower_block(lowerer, body.block())?;
    let frame = lowerer.exit_function();
    Ok(HirFunction { params, body, upvalues: frame.upvalues })
}
```

- [ ] **Step 7: Initialize the new `upvalues` field everywhere `HirFunction` is constructed**

The `lower()` entry point may also construct an `HirFunction` for the top-level. Audit and ensure every construction sets `upvalues: Vec::new()` (top-level can never have upvalues).

- [ ] **Step 8: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-hir`
Expected: existing tests still pass. The five "rejects/accepts closure" tests Plan 2 added are now incorrect (they expected an Unsupported error). Delete or update them — see next step.

- [ ] **Step 9: Update Plan 2's closure-capture tests**

In the `tests` mod, find and replace these tests:

- `rejects_anonymous_closure_capturing_local` → delete (capture is now supported)
- `rejects_method_decl_capturing_outer_local` → delete

Add new positive tests:

```rust
    #[test]
    fn anonymous_closure_captures_local_as_upvalue() {
        let p = lower_str("local x = 1 local f = function() return x end");
        // Find the local decl for f, extract the Function literal.
        let func = p.main.iter().find_map(|s| {
            if let HirStmt::LocalDecl { value: HirExpr::Function(f), .. } = s {
                Some(f)
            } else { None }
        }).expect("found anon fn");
        assert_eq!(func.upvalues.len(), 1);
        assert!(matches!(func.upvalues[0], UpvalueSource::ParentLocal(_)));
    }

    #[test]
    fn deeply_nested_closure_threads_upvalue() {
        // function a() local x; function b() function c() return x end end end
        // Lowered as: x captured by b (ParentLocal), then by c (ParentUpval).
        // We can verify by digging into the HirFunctions.
        let p = lower_str("local x = 1 local b = function() local c = function() return x end return c end");
        let b = p.main.iter().find_map(|s| {
            if let HirStmt::LocalDecl { value: HirExpr::Function(f), .. } = s { Some(f) } else { None }
        }).expect("b");
        assert_eq!(b.upvalues.len(), 1);
        assert!(matches!(b.upvalues[0], UpvalueSource::ParentLocal(_)));
        // c is the local decl inside b.body
        let c = b.body.iter().find_map(|s| {
            if let HirStmt::LocalDecl { value: HirExpr::Function(f), .. } = s { Some(f) } else { None }
        }).expect("c");
        assert_eq!(c.upvalues.len(), 1);
        assert!(matches!(c.upvalues[0], UpvalueSource::ParentUpval(0)));
    }

    #[test]
    fn upvalue_assignment_lowers_to_upvalue_assign() {
        let p = lower_str("local x = 1 local f = function() x = 2 end");
        let f = p.main.iter().find_map(|s| {
            if let HirStmt::LocalDecl { value: HirExpr::Function(f), .. } = s { Some(f) } else { None }
        }).expect("f");
        assert_eq!(f.upvalues.len(), 1);
        assert!(matches!(&f.body[0], HirStmt::UpvalueAssign { upvalue: 0, .. }));
    }

    #[test]
    fn shared_upvalue_dedupes() {
        let p = lower_str("local x = 1 local f = function() x = x + 1 end");
        let f = p.main.iter().find_map(|s| {
            if let HirStmt::LocalDecl { value: HirExpr::Function(f), .. } = s { Some(f) } else { None }
        }).expect("f");
        // x is read AND written but should count as one upvalue.
        assert_eq!(f.upvalues.len(), 1);
    }
```

- [ ] **Step 10: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): upvalue propagation for cross-function-frame local captures"
```

---

## Task 3: HIR — `local function` support

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

`local function f() … end` — declare `f` as a local BEFORE lowering the body, so the body can recurse.

- [ ] **Step 1: Find the existing `Stmt::LocalFunction(_) => Err(...)` arm**

Replace with:

```rust
        Stmt::LocalFunction(lf) => {
            // Declare the local BEFORE lowering the body, so the body can recurse.
            let name = lf.name().token().to_string();
            let symbol = lowerer.declare_local(&name);
            let function = lower_function_body(lowerer, lf.body())?;
            // Lower as a LocalDecl whose value is the anonymous function literal.
            // Using LocalDecl (rather than FunctionDecl) is cleaner because the
            // symbol is already declared; the value is just the closure.
            Ok(vec![HirStmt::LocalDecl {
                symbol,
                value: HirExpr::Function(function),
            }])
        }
```

If full_moon's `LocalFunction::name()` returns something different, adapt — the structural ask is: a single TokenReference for the name.

- [ ] **Step 2: Add tests**

```rust
    #[test]
    fn lowers_local_function() {
        let p = lower_str("local function f(x) return x end");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        assert!(matches!(value, HirExpr::Function(_)));
    }

    #[test]
    fn local_function_can_recurse() {
        // Verifies the symbol is declared BEFORE the body is lowered.
        let p = lower_str("local function f(n) if n <= 0 then return 1 end return n * f(n-1) end");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        let HirExpr::Function(func) = value else { panic!() };
        // The recursive `f(n-1)` reference must resolve to the local f, not a global.
        // We check that no symbol named "f" was added to the symbols table with kind Global.
        let f_globals: Vec<&Symbol> = p.symbols.iter()
            .filter(|s| s.name == "f" && s.kind == SymbolKind::Global)
            .collect();
        assert!(f_globals.is_empty(), "f should be local, not global");
        // And its upvalues list should NOT include f as ParentLocal (since f is in the same frame).
        assert!(func.upvalues.is_empty() || !matches!(func.upvalues.first(), Some(UpvalueSource::ParentLocal(_))));
    }
```

- [ ] **Step 3: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-hir`

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir/src/lower.rs
git commit -m "feat(hir): local function decls (with self-recursion)"
```

---

## Task 4: MIR — upvalue instructions and MakeClosure source list

**Files:**
- Modify: `crates/luau-mir/src/types.rs`
- Modify: `crates/luau-mir/src/lower.rs`
- Modify: `crates/luau-mir/src/lower_stmts.rs`

- [ ] **Step 1: Extend MIR types in `crates/luau-mir/src/types.rs`**

Re-export `UpvalueSource` from luau-hir for convenience, then update Instr:

```rust
pub use luau_hir::UpvalueSource;
```

Replace `Instr` to update MakeClosure and add two new variants:

```rust
#[derive(Debug, Clone)]
pub enum Instr {
    LoadConst { dst: VLocal, src: ConstId },
    Move { dst: VLocal, src: VLocal },
    BinOp { dst: VLocal, op: BinOp, lhs: Value, rhs: Value },
    UnOp { dst: VLocal, op: UnOp, operand: Value },
    GetGlobal { dst: VLocal, name: ConstId },
    SetGlobal { name: ConstId, value: Value },
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Create a closure. `upvalues` lists how each of the new closure's upvalues
    /// is sourced from the *current* function's frame.
    MakeClosure { dst: VLocal, function: FunctionId, upvalues: Vec<UpvalueSource> },
    /// `dst = current_frame.upvals[idx]` (reads through the cell).
    GetUpval { dst: VLocal, idx: u32 },
    /// `current_frame.upvals[idx] = value` (writes through the cell).
    SetUpval { idx: u32, value: Value },
    NewTable { dst: VLocal },
    GetIndex { dst: VLocal, obj: Value, key: Value },
    SetIndex { obj: Value, key: Value, value: Value },
}
```

Also update `MirFunction` to carry upvalues:

```rust
#[derive(Debug, Clone)]
pub struct MirFunction {
    pub id: FunctionId,
    pub params: Vec<VLocal>,
    pub blocks: Vec<BasicBlock>,
    pub consts: Vec<Constant>,
    pub n_locals: u32,
    /// Upvalue sources for this function. Index = upvalue id used by GetUpval/SetUpval.
    pub upvalues: Vec<UpvalueSource>,
}
```

- [ ] **Step 2: Update MIR lowering to thread upvalues through**

In `crates/luau-mir/src/lower.rs`, the `lower` function constructs `MirFunction` records. The pending-functions queue stores `(FunctionId, HirFunction)`. Each `HirFunction` now carries `upvalues`. Stuff them into the `MirFunction`:

Find the line `program_functions.push(MirFunction { id: fid, params: builder.params, blocks: builder.blocks, consts: builder.consts, n_locals: builder.next_local });` and add a field:

```rust
program_functions.push(MirFunction {
    id: fid,
    params: builder.params,
    blocks: builder.blocks,
    consts: builder.consts,
    n_locals: builder.next_local,
    upvalues: fhir.upvalues.clone(),
});
```

- [ ] **Step 3: Add `HirExpr::Upvalue(idx)` lowering**

In `FnBuilder::lower_expr`, find the match on `HirExpr`. Add an arm:

```rust
            HirExpr::Upvalue(idx) => {
                let dst = self.fresh_local();
                self.emit(Instr::GetUpval { dst, idx: *idx });
                Ok(dst)
            }
```

- [ ] **Step 4: Update `MakeClosure` emission**

`HirExpr::Function(f)` currently emits `Instr::MakeClosure { dst, function: fid }`. Now it must also pass the upvalue sources:

```rust
            HirExpr::Function(f) => {
                let fid = self.queue_function(f);
                let dst = self.fresh_local();
                self.emit(Instr::MakeClosure {
                    dst,
                    function: fid,
                    upvalues: f.upvalues.clone(),
                });
                Ok(dst)
            }
```

- [ ] **Step 5: Add `HirStmt::UpvalueAssign` lowering**

In `crates/luau-mir/src/lower_stmts.rs`, add an arm:

```rust
        HirStmt::UpvalueAssign { upvalue, value } => {
            let v = b.lower_expr(value)?;
            b.emit(Instr::SetUpval { idx: *upvalue, value: Value::VLocal(v) });
            Ok(())
        }
```

- [ ] **Step 6: Add a couple of MIR tests**

In `crates/luau-mir/src/lower.rs`'s `tests` mod:

```rust
    #[test]
    fn closure_capture_emits_get_upval() {
        let p = mir_of("local x = 1 local f = function() return x end");
        // The closure-function (index 1) should contain a GetUpval instruction.
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
```

- [ ] **Step 7: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-mir`
Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Downstream LIR will need updates (Tasks 5-7). If those fail, ignore for now — just confirm `cargo test -p luau-mir` passes.

- [ ] **Step 8: Commit**

```bash
git add crates/luau-mir/src/types.rs crates/luau-mir/src/lower.rs crates/luau-mir/src/lower_stmts.rs
git commit -m "feat(mir): upvalue instructions and MakeClosure source list"
```

---

## Task 5: LIR — upvalue opcodes and operand kind

**Files:**
- Modify: `crates/luau-lir/src/types.rs`
- Modify: `crates/luau-lir/src/regalloc.rs`
- Modify: `crates/luau-lir/src/lower.rs`

- [ ] **Step 1: Add `OpKind::GetUpval`, `SetUpval`** in `crates/luau-lir/src/types.rs`

Replace `OpKind`:

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
}
```

- [ ] **Step 2: Add `Operand::UpvalIdx(u32)` and `LirFunction::num_upvals`** 

Replace `Operand`:

```rust
#[derive(Debug, Clone, Copy)]
pub enum Operand {
    Reg(Reg),
    Const(ConstIdx),
    Proto(ProtoId),
    JmpTarget(BlockLabel),
    SmallInt(i16),
    UpvalIdx(u16),
}
```

Replace `LirFunction`:

```rust
#[derive(Debug, Clone)]
pub struct LirFunction {
    pub id: ProtoId,
    pub num_params: u16,
    pub num_regs: u16,
    /// Number of upvalues this function reads. Renders into the proto meta.
    pub num_upvals: u16,
    pub consts: Vec<Constant>,
    pub instrs: Vec<LirInstr>,
    pub label_positions: Vec<(BlockLabel, u32)>,
    /// Closure-creation upvalue source lists. `upval_sources[i]` is the list of
    /// (source_kind, src_idx) pairs for the i-th `Closure` instruction in this
    /// function's instruction stream. We store them outside `LirInstr` because
    /// they're variable-length and the existing `Operand` enum encodes operands
    /// uniformly. The encoder consumes this in order.
    pub closure_upval_sources: Vec<Vec<UpvalSource>>,
}

#[derive(Debug, Clone, Copy)]
pub enum UpvalSource {
    /// Capture from the current frame's local at this register.
    LocalReg(u16),
    /// Re-capture from the current frame's upvalue at this index.
    ParentUpval(u16),
}
```

- [ ] **Step 3: Update `visit_vlocals` in `crates/luau-lir/src/regalloc.rs`**

Add arms for the new instructions; the new MakeClosure still has `dst` to visit:

```rust
fn visit_vlocals(instr: &luau_mir::Instr, mut f: impl FnMut(VLocal)) {
    use luau_mir::{Instr, Value};
    fn visit_value(v: Value, f: &mut impl FnMut(VLocal)) {
        if let Value::VLocal(l) = v { f(l); }
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
        Instr::GetUpval { dst, .. } => f(*dst),
        Instr::SetUpval { value, .. } => visit_value(*value, &mut f),
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

- [ ] **Step 4: Update `lower_function` in `crates/luau-lir/src/lower.rs`**

Find the construction of `LirFunction { ... }` and add the new fields. `closure_upval_sources` is built as we lower `MakeClosure` instructions.

Add a mutable `Vec<Vec<UpvalSource>>` next to `instrs` while lowering, push to it on each MakeClosure (see Step 5), and use it in the final `LirFunction { ... }`.

Add the `num_upvals: f.upvalues.len() as u16` field.

- [ ] **Step 5: Lower the new MIR instructions in `lower_instr`**

Add arms:

```rust
        MInstr::GetUpval { dst, idx } => {
            out.push(LirInstr {
                op: OpKind::GetUpval,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::UpvalIdx(*idx as u16),
                ],
            });
        }
        MInstr::SetUpval { idx, value } => {
            let r = val_to_reg(*value, rm);
            out.push(LirInstr {
                op: OpKind::SetUpval,
                operands: vec![Operand::UpvalIdx(*idx as u16), Operand::Reg(r)],
            });
        }
```

Update the existing `MInstr::MakeClosure { dst, function }` arm to also handle the new `upvalues` field. Convert each `UpvalueSource` into an `UpvalSource`:

- `UpvalueSource::ParentLocal(sym)` — the current function captures `sym` as one of its own locals. Use the SymbolMap to look up the source register: but the MIR Builder's `local_for(sym)` mapped that to a `VLocal`. We need that `VLocal`'s register from `rm`. To find it: the parent frame is the CURRENT MIR function being lowered, and the symbol is one of its locals. Use `f.params` and walk symbols, OR change the MIR `MakeClosure` `upvalues` field to carry VLocal references (`Vec<UpvalSourceMir>`) at MIR time instead of `UpvalueSource` ids — see the IMPORTANT note below.

**IMPORTANT — design adjustment:** At MIR time, we know VLocals, not original SymbolIds. To make LIR lowering tractable, change MIR's `MakeClosure.upvalues` from `Vec<UpvalueSource>` to a new MIR-local source enum:

```rust
// In crates/luau-mir/src/types.rs
#[derive(Debug, Clone, Copy)]
pub enum MirUpvalSource {
    /// Capture the current function's VLocal at this slot (after regalloc → a Reg).
    Local(VLocal),
    /// Re-capture the current function's upvalue at this index.
    ParentUpval(u32),
}
```

…and update `Instr::MakeClosure` to use `Vec<MirUpvalSource>`. The HIR→MIR lowering produces these by translating each `HirFunction.upvalues` element:

- `UpvalueSource::ParentLocal(sym)` → `MirUpvalSource::Local(builder.local_for(sym))`
- `UpvalueSource::ParentUpval(idx)` → `MirUpvalSource::ParentUpval(idx)`

This change must happen in Task 4. Adjust Task 4's MIR `Instr::MakeClosure` field type from `Vec<UpvalueSource>` to `Vec<MirUpvalSource>` and add the conversion in `lower_expr`.

Then in LIR lowering, the arm becomes:

```rust
        MInstr::MakeClosure { dst, function, upvalues } => {
            let lir_upvals: Vec<UpvalSource> = upvalues.iter().map(|s| match s {
                MirUpvalSource::Local(v) => UpvalSource::LocalReg(rm.get(*v)),
                MirUpvalSource::ParentUpval(idx) => UpvalSource::ParentUpval(*idx as u16),
            }).collect();
            // Stash for the encoder.
            let closure_idx = closure_upval_sources.len();
            closure_upval_sources.push(lir_upvals);
            out.push(LirInstr {
                op: OpKind::Closure,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Proto(ProtoId(function.0 as u16)),
                    Operand::SmallInt(closure_idx as i16),
                ],
            });
        }
```

(The `SmallInt(closure_idx)` operand is a back-reference the encoder uses to find the upvalue source list for this Closure. The encoder emits the operand bytes plus a length-prefixed source list right after.)

- [ ] **Step 6: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-lir`

LIR's own tests should pass. Workspace tests likely still fail because Emit/VM aren't updated yet.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-lir/src/types.rs crates/luau-lir/src/regalloc.rs crates/luau-lir/src/lower.rs
git add crates/luau-mir/src/types.rs crates/luau-mir/src/lower.rs
git commit -m "feat(lir): upvalue opcodes and closure source threading"
```

---

## Task 6: Emit — opmap, encode, render

**Files:**
- Modify: `crates/luau-emit/src/opmap.rs`
- Modify: `crates/luau-emit/src/encode.rs`
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Update `opmap.rs`**

Resize the `forward` array from 29 → 31. Add `GetUpval` and `SetUpval` to `ALL_OPS`.

```rust
forward: [u8; 31],
```

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
];
```

And update the array init to `[0u8; 31]`.

- [ ] **Step 2: Update `render.rs`**

Add to `opname`:

```rust
        OpKind::GetUpval => "GetUpval",
        OpKind::SetUpval => "SetUpval",
```

Update the `MetaEntry` struct to include `num_upvals`:

```rust
#[derive(Serialize)]
struct MetaEntry {
    num_params: u16,
    num_regs: u16,
    num_upvals: u16,
}
```

In the `meta` construction:

```rust
let meta: Vec<MetaEntry> = program
    .functions
    .iter()
    .map(|f| MetaEntry {
        num_params: f.num_params,
        num_regs: std::cmp::max(f.num_regs, f.num_params),
        num_upvals: f.num_upvals,
    })
    .collect();
```

- [ ] **Step 3: Update `encode.rs` for the new operand kind and Closure layout**

The existing `instr_size` and the encoding loop assume each operand is 2 bytes. Add a special case for `Closure`: after the standard operands, emit `num_upvals` (1 byte? or u16?) followed by the upvalue sources.

Each upvalue source is encoded as:
- 1 byte kind discriminator: 0 = LocalReg, 1 = ParentUpval
- 2 bytes payload (u16)

The encoder needs access to `LirFunction.closure_upval_sources`. Pass it through to `encode_function`:

Replace `encode_function`:

```rust
pub fn encode_function(f: &LirFunction, opmap: &OpMap) -> Vec<u8> {
    // First pass: compute byte offsets (size depends on Closure since its variable-length).
    let mut instr_byte_offsets: Vec<u32> = Vec::with_capacity(f.instrs.len() + 1);
    let mut offset: u32 = 0;
    for instr in &f.instrs {
        instr_byte_offsets.push(offset);
        offset += instr_size_for(instr, f);
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

    let mut out: Vec<u8> = Vec::with_capacity(offset as usize);
    for (i, instr) in f.instrs.iter().enumerate() {
        out.push(opmap.opcode_of(instr.op));
        let after_this = instr_byte_offsets[i] + instr_size_for(instr, f);
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
        // Closure carries a trailing variable-length upvalue source list.
        if instr.op == OpKind::Closure {
            let closure_idx = match instr.operands.get(2) {
                Some(Operand::SmallInt(n)) => *n as usize,
                _ => panic!("Closure missing closure_idx operand"),
            };
            let sources = &f.closure_upval_sources[closure_idx];
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
        }
    }
    out
}

fn instr_size_for(i: &LirInstr, f: &LirFunction) -> u32 {
    let base = 1 + (i.operands.len() as u32) * 2;
    if i.op == OpKind::Closure {
        let closure_idx = match i.operands.get(2) {
            Some(Operand::SmallInt(n)) => *n as usize,
            _ => return base,
        };
        let sources = &f.closure_upval_sources[closure_idx];
        // 2 bytes for the length prefix + (1 + 2) per source.
        return base + 2 + (sources.len() as u32) * 3;
    }
    base
}
```

Add an import for `UpvalSource` at the top of `encode.rs`:

```rust
use luau_lir::{BlockLabel, LirFunction, LirInstr, Operand, OpKind, UpvalSource};
```

(`UpvalSource` is re-exported from `luau_lir`'s lib — verify the lib `pub use` re-exports it. Add `pub use types::*;` or similar if needed.)

- [ ] **Step 4: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-emit`

- [ ] **Step 5: Commit**

```bash
git add crates/luau-emit/src/opmap.rs crates/luau-emit/src/encode.rs crates/luau-emit/src/render.rs
git commit -m "feat(emit): GetUpval/SetUpval opcodes and Closure upval source encoding"
```

---

## Task 7: VM template — upvalue handlers and closure machinery

**Files:**
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

The VM needs three changes:
1. Each frame gets an `upvals` array — passed in from the closure that called it.
2. The `Closure` handler builds a fresh upvals array from the current frame's regs/upvals according to the encoded source list.
3. `GetUpval` / `SetUpval` read/write through `upvals[idx].tbl[upvals[idx].key]`.

The cell representation: each upvalue is a `{ tbl, key }` table where `tbl[key]` is the live value. For a captured local at register `r` in the parent frame, the cell is `{ tbl = parent_regs, key = r }`. For a re-captured upvalue at parent index `u`, the cell is the same table object as `parent_upvals[u]` — sharing semantics for free.

- [ ] **Step 1: Update `vm_call` signature and frame initialization**

The `vm_call` function takes `proto_id`, `args`, `nargs`. Add an `upvals` parameter:

```luau
local vm_call
vm_call = function(proto_id, args, nargs, upvals)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local meta = META[proto_id + 1]
    local num_params = meta[1]
    local num_regs = meta[2]
    -- meta[3] is num_upvals; we don't need it inside the frame (upvals is passed in)
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    upvals = upvals or {}  -- bootstrap: main proto has no upvals
    local pc = 1
    local code_len = #code
    -- ... existing dispatch loop ...
end
```

- [ ] **Step 2: Update the Closure handler**

Replace the existing `elseif op == OP_Closure then ... end` arm with:

```luau
        elseif op == OP_Closure then
            local a = read_u16(code, pc); pc = pc + 2
            local pid = read_u16(code, pc); pc = pc + 2
            -- Skip the closure_idx operand (used only by Rust-side encoding bookkeeping).
            pc = pc + 2
            local n_upvals = read_u16(code, pc); pc = pc + 2
            local new_upvals = {}
            for u = 1, n_upvals do
                local kind = _sbyte(code, pc); pc = pc + 1
                local payload = read_u16(code, pc); pc = pc + 2
                if kind == 0 then
                    -- LocalReg: cell references the current frame's regs at register `payload`.
                    new_upvals[u] = { tbl = regs, key = payload }
                else
                    -- ParentUpval: re-share the current frame's existing upvalue cell.
                    new_upvals[u] = upvals[payload + 1]
                end
            end
            local captured = new_upvals
            regs[a] = function(...)
                local pa = { ... }
                local n = _select("#", ...)
                return vm_call(pid, pa, n, captured)
            end
```

- [ ] **Step 3: Add `GetUpval` and `SetUpval` handlers**

Immediately after the existing `elseif op == OP_SetTable then ... end` arm, insert:

```luau
        elseif op == OP_GetUpval then
            local a = read_u16(code, pc); pc = pc + 2
            local idx = read_u16(code, pc); pc = pc + 2
            local cell = upvals[idx + 1]
            regs[a] = cell.tbl[cell.key]
        elseif op == OP_SetUpval then
            local idx = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local cell = upvals[idx + 1]
            cell.tbl[cell.key] = regs[b]
```

- [ ] **Step 4: Update the bootstrap call**

The bottom-of-file bootstrap currently calls `vm_call(0, main_args, main_n)`. Pass an empty upvals table:

```luau
return vm_call(0, main_args, main_n, {})
```

- [ ] **Step 5: Update META loop in template if num_upvals is added**

If render.rs's `MetaEntry` grew a `num_upvals` field (Task 6), update the template's META loop:

```luau
{% for m in meta %}    { {{ m.num_params }}, {{ m.num_regs }}, {{ m.num_upvals }} },
{% endfor %}
```

- [ ] **Step 6: Run the full workspace tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Plan 1+2 corpus programs use no upvalues; they should still pass. New corpus programs come in Task 8.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-runtime/assets/vm.luau.j2
git commit -m "feat(runtime): closure upval cells and GetUpval/SetUpval handlers"
```

---

## Task 8: Corpus — closure differential tests

**Files:**
- Create: `runtime-tests/corpus/closures.luau`
- Create: `runtime-tests/corpus/counter.luau`
- Create: `runtime-tests/corpus/nested_closures.luau`

- [ ] **Step 1: Create `runtime-tests/corpus/closures.luau`**

```luau
-- Basic capture: function reads outer local.
local x = 42
local read_x = function() return x end
print(read_x())

-- Mutation through upvalue.
local n = 0
local bump = function() n = n + 1 end
bump()
bump()
bump()
print(n)

-- Two closures sharing the same upvalue.
local shared = 100
local add = function(k) shared = shared + k end
local get = function() return shared end
add(5)
add(7)
print(get())
```

- [ ] **Step 2: Create `runtime-tests/corpus/counter.luau`**

```luau
local function make_counter()
    local n = 0
    local function inc()
        n = n + 1
        return n
    end
    return inc
end

local c1 = make_counter()
local c2 = make_counter()
print(c1())
print(c1())
print(c1())
print(c2())
print(c1())
print(c2())
```

(Expected output: `1, 2, 3, 1, 4, 2` — c1 and c2 have independent state, recursion via local function.)

- [ ] **Step 3: Create `runtime-tests/corpus/nested_closures.luau`**

```luau
-- Triple-nested closure threading an upvalue.
local function outer()
    local x = 10
    local function middle()
        local function inner()
            return x
        end
        return inner
    end
    return middle
end

local m = outer()
local i = m()
print(i())

-- Inner closure can mutate the outermost local.
local function bumper()
    local n = 0
    local function step()
        local function actual()
            n = n + 1
            return n
        end
        return actual
    end
    return step
end

local s = bumper()
local a1 = s()
local a2 = s()
print(a1())
print(a2())
print(a1())
```

(Expected: `10`, `1`, `2`, `3` — `a1` and `a2` share the same `n`.)

- [ ] **Step 4: Run the differential harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Or directly: `PATH="$HOME/.cargo/bin:$PATH" cargo run -p runtime-tests --release`

All Plan 1-2 corpus programs + the three new ones should pass.

- [ ] **Step 5: Commit**

```bash
git add runtime-tests/corpus/closures.luau runtime-tests/corpus/counter.luau runtime-tests/corpus/nested_closures.luau
git commit -m "test(corpus): closure capture, counters, nested closures"
```

---

## Task 9: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the Plan 2 status line**

```
**Status:** Plan 3 — closures with upvalues. The obfuscator round-trips an
expanded Luau subset (Plan 2 features + `local function`, anonymous functions
capturing parent locals, mutual sharing across closures, deeply nested
captures) through a custom VM with no obfuscation passes yet.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 3 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All corpus programs (Plan 1 + 2 + 3) pass differential testing.
- [ ] `cargo clippy --workspace --tests` clean (or no new warnings beyond Plan 2).
- [ ] Determinism: same seed → same output for closures.luau.
- [ ] No `panic!("...")` or `todo!()` left in the new code.
