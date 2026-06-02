# Luau Obfuscator — Plan 1: Skeleton & Identity Pipeline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the end-to-end Rust pipeline (parse → HIR → MIR → passes → LIR → emit) plus a templated Luau VM runtime, with no actual obfuscation. The output is a self-executing Luau chunk that runs the user's program through a custom VM. Differential execution test harness in place.

**Architecture:** Rust workspace with one crate per pipeline stage. Each stage has its own IR type. Obfuscation passes are MIR→MIR transforms; for Plan 1 we ship only an identity pass. The VM is templated Luau source rendered by `luau-emit`. The CLI is a thin shell over the `luau-obf` facade crate.

**Tech Stack:** Rust 1.78+, `full_moon` (Luau parser), `minijinja` (template), `rand_chacha` (seeded RNG), `clap` (CLI), `thiserror` (errors), `insta` (snapshots), `assert_cmd` (CLI tests). Luau standalone (`luau` CLI) for execution tests.

**Plan 1 language subset** (defer rest to later plans):

| Supported | Deferred |
|---|---|
| `nil`, `true`, `false`, number, string literals | Tables, table constructors |
| Local variables (single & list decl) | Closures with upvalues (no `local function`) |
| Globals (read/write via fenv) | Methods (`a:b()`) |
| Arithmetic `+ - * / % ^`, unary `-` | Multi-return, varargs |
| Comparison `== ~= < > <= >=` | Metatables |
| Logical `and or not` (short-circuit) | `continue`, compound assign, string interp |
| String concat `..`, length `#` | Type annotations are **stripped**, not parsed deeply |
| `if/elseif/else`, `while`, `repeat` | `for-in`, generalized iteration |
| Numeric `for` | vector, buffer, bit32 |
| Global `function name(args)…end` (recursion via global) | |
| Function calls with 0+ positional args, single return used | |
| `return` (0 or 1 value) | |
| Comments | |

This subset is enough to write a meaningful test corpus (factorial, fib, fizzbuzz, string-builder loop, recursive descent on numbers). Plan 2 expands the subset; nothing in Plan 1 should block that expansion.

---

## Implementation notes

**full_moon API:** the plan's HIR-lowering code (Tasks 4 and 5) names specific full_moon types and methods — `Expression::Number`, `Stmt::LocalAssignment`, `If::condition()`, etc. These reflect the public API as of full_moon 1.1, but minor naming differences (`names()` vs `name_list()`, Punctuated iteration shape, optional-field accessor signatures) are common across versions. If a method name in the plan doesn't match the version on `docs.rs/full_moon`, adapt — the structural shape (recursive visit of expressions and statements, produce HIR) is what matters. When in doubt, run `cargo doc --open -p full_moon` and pattern-match against the real enums.

**Stub `todo!()` macros:** Several tasks introduce a stub function (e.g., `lower()` in `luau-hir/src/lower.rs`) that's filled in by a later task. These are intentional scaffolding — calling them at the wrong time will panic at runtime, but `cargo check` will pass. Each stub names the task that replaces it.

**Engineer-typed code blocks:** Each task shows the full content of the files it touches. Copy-paste is fine. Where a task says "replace X with Y," make sure to replace, not append — overlapping definitions will cause compile errors.

---

## File Structure

```
luau-obf/
├── Cargo.toml                       # workspace manifest
├── rust-toolchain.toml              # pin a Rust version
├── .gitignore
├── crates/
│   ├── luau-parse/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs               # parse(src) -> Result<Ast, ParseError>
│   ├── luau-hir/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs             # SymbolId, Scope, HirExpr, HirStmt, HirFunction
│   │       ├── lower.rs             # ast -> hir
│   │       └── resolve.rs           # name resolution (assign SymbolIds)
│   ├── luau-mir/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs             # MirProgram, MirFunction, BasicBlock, Instr, Terminator
│   │       └── lower.rs             # hir -> mir, desugar control flow
│   ├── luau-passes/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # Pass trait, PassPlan, run_plan
│   │       └── identity.rs          # no-op pass (Plan 1's only pass)
│   ├── luau-lir/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs             # LirProgram, LirFunction, LirInstr, OpKind, Reg
│   │       ├── regalloc.rs          # simple linear allocator
│   │       └── lower.rs             # mir -> lir
│   ├── luau-emit/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # emit(lir, seed) -> String
│   │       ├── opmap.rs             # symbolic OpKind -> opcode byte (deterministic in Plan 1)
│   │       ├── encode.rs            # bytecode + constant pool encoding
│   │       └── render.rs            # minijinja template render
│   ├── luau-runtime/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs               # exposes VM_TEMPLATE: &str
│   │   └── assets/vm.luau.j2        # the Luau VM template (minijinja syntax)
│   ├── luau-obf/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs               # Options, Error, obfuscate(src, opts)
│   └── luau-obf-cli/
│       ├── Cargo.toml
│       └── src/main.rs              # clap CLI
└── runtime-tests/
    ├── Cargo.toml
    ├── src/main.rs                  # differential test harness binary
    └── corpus/
        ├── arithmetic.luau
        ├── if_else.luau
        ├── while_loop.luau
        ├── numeric_for.luau
        ├── recursion.luau
        └── string_ops.luau
```

---

## Task 1: Initialize workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`

- [ ] **Step 1: Create `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.78.0"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Create `.gitignore`**

```
/target
**/*.rs.bk
Cargo.lock.bak
```

- [ ] **Step 3: Create workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/luau-parse",
    "crates/luau-hir",
    "crates/luau-mir",
    "crates/luau-passes",
    "crates/luau-lir",
    "crates/luau-emit",
    "crates/luau-runtime",
    "crates/luau-obf",
    "crates/luau-obf-cli",
    "runtime-tests",
]

[workspace.package]
edition = "2021"
rust-version = "1.78"
license = "Apache-2.0"

[workspace.dependencies]
full_moon = { version = "1.1", features = ["luau"] }
thiserror = "1"
minijinja = "2"
rand = "0.8"
rand_chacha = "0.3"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
insta = { version = "1", features = ["yaml"] }
assert_cmd = "2"
tempfile = "3"
```

- [ ] **Step 4: Run cargo check (will fail — no member crates yet)**

Run: `cargo check --workspace`
Expected: failure (missing members). This is fine — proves the manifest parses.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore
git commit -m "chore: initialize workspace"
```

---

## Task 2: luau-parse crate

**Files:**
- Create: `crates/luau-parse/Cargo.toml`
- Create: `crates/luau-parse/src/lib.rs`
- Test: `crates/luau-parse/src/lib.rs` (in-file `#[cfg(test)]`)

- [ ] **Step 1: Create `crates/luau-parse/Cargo.toml`**

```toml
[package]
name = "luau-parse"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
full_moon = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2: Write failing test in `crates/luau-parse/src/lib.rs`**

```rust
//! Thin wrapper around full_moon's Luau parser.

use thiserror::Error;

pub use full_moon::ast::Ast;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    FullMoon(String),
}

pub fn parse(source: &str) -> Result<Ast, ParseError> {
    full_moon::parse(source).map_err(|errs| {
        ParseError::FullMoon(
            errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_program() {
        let ast = parse("").expect("empty source should parse");
        assert!(ast.nodes().stmts().next().is_none());
    }

    #[test]
    fn parses_simple_assignment() {
        let ast = parse("local x = 1").expect("simple local should parse");
        assert_eq!(ast.nodes().stmts().count(), 1);
    }

    #[test]
    fn reports_parse_error() {
        let err = parse("local =").unwrap_err();
        let ParseError::FullMoon(msg) = err;
        assert!(!msg.is_empty(), "error message should be non-empty");
    }
}
```

- [ ] **Step 3: Run tests, expect compile error**

Run: `cargo test -p luau-parse`
Expected: compiles & passes. (full_moon is the heavy lifter; this crate is mechanical.)

- [ ] **Step 4: If failures, fix and re-run until green**

- [ ] **Step 5: Commit**

```bash
git add crates/luau-parse
git commit -m "feat(parse): wrap full_moon Luau parser"
```

---

## Task 3: luau-hir types

**Files:**
- Create: `crates/luau-hir/Cargo.toml`
- Create: `crates/luau-hir/src/lib.rs`
- Create: `crates/luau-hir/src/types.rs`

- [ ] **Step 1: Create `crates/luau-hir/Cargo.toml`**

```toml
[package]
name = "luau-hir"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-parse = { path = "../luau-parse" }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-hir/src/types.rs`**

```rust
//! High-level IR. Luau-shaped, but with name resolution applied and types stripped.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub fn fresh() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        SymbolId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Local variable (parameter or `local x`).
    Local,
    /// Global (looked up in fenv at runtime).
    Global,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: SymbolId,
    pub kind: SymbolKind,
    /// Original source name. For globals, used as the fenv key. For locals,
    /// only used for diagnostics; the SymbolId is what subsequent stages key on.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirLiteral {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Concat,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Len,
}

#[derive(Debug, Clone)]
pub enum HirExpr {
    Literal(HirLiteral),
    /// Reference to a resolved symbol.
    Symbol(SymbolId),
    BinOp(BinOp, Box<HirExpr>, Box<HirExpr>),
    UnOp(UnOp, Box<HirExpr>),
    /// f(args...). Function expression is `callee`; arg list is positional.
    Call { callee: Box<HirExpr>, args: Vec<HirExpr> },
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    /// `local x = expr` (or `local x` with `expr = Literal(Nil)`).
    LocalDecl { symbol: SymbolId, value: HirExpr },
    /// `name = expr` where `name` is a resolved symbol.
    Assign { target: SymbolId, value: HirExpr },
    /// `f(args...)` as a statement (return value discarded).
    ExprStmt(HirExpr),
    /// `if cond then then_body [else else_body] end`.
    /// elseif chains are encoded as nested If in else_body.
    If { cond: HirExpr, then_body: Vec<HirStmt>, else_body: Vec<HirStmt> },
    /// `while cond do body end`
    While { cond: HirExpr, body: Vec<HirStmt> },
    /// `repeat body until cond` — semantics: run body at least once, then exit if cond truthy.
    Repeat { cond: HirExpr, body: Vec<HirStmt> },
    /// `for var = start, stop[, step] do body end`. `step` defaults to 1.0.
    NumericFor {
        var: SymbolId,
        start: HirExpr,
        stop: HirExpr,
        step: HirExpr,
        body: Vec<HirStmt>,
    },
    /// `return [expr]` — Plan 1 supports zero or one return value.
    Return(Option<HirExpr>),
    /// Global function declaration: `function name(args) body end`.
    /// Lowered from `function f(...) end`, which is sugar for `f = function(...) end`.
    FunctionDecl { name: SymbolId, function: HirFunction },
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub params: Vec<SymbolId>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    /// All symbols declared anywhere in the program, keyed by SymbolId.
    /// `symbols[s.id.0 as usize]` is the Symbol for a given id (when present).
    pub symbols: Vec<Symbol>,
    /// Top-level statements (the main chunk).
    pub main: Vec<HirStmt>,
}
```

- [ ] **Step 3: Create `crates/luau-hir/src/lib.rs`**

```rust
//! HIR — high-level IR for Luau. AST → HIR lowering and name resolution.

mod types;
pub mod lower;
pub mod resolve;

pub use types::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HirError {
    #[error("parse error: {0}")]
    Parse(#[from] luau_parse::ParseError),
    #[error("unsupported construct in Plan 1 subset: {0}")]
    Unsupported(String),
}
```

- [ ] **Step 4: Create stub `crates/luau-hir/src/lower.rs`**

```rust
//! AST → HIR lowering. Plan 1 supports the subset documented in the plan.

use crate::{HirError, HirProgram};
use luau_parse::Ast;

/// Lower a parsed AST into HIR (without name resolution applied yet).
pub fn lower(_ast: &Ast) -> Result<HirProgram, HirError> {
    todo!("implemented in later tasks")
}
```

- [ ] **Step 5: Create stub `crates/luau-hir/src/resolve.rs`**

```rust
//! Name resolution: walks HIR and assigns SymbolIds to references.
//! In our design we resolve during lowering rather than as a separate pass,
//! so this module is currently empty. Kept as a separate file because Plan 2's
//! renaming pass will live here.

// Intentionally empty.
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p luau-hir`
Expected: compiles with one `todo!()`.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-hir
git commit -m "feat(hir): define HIR types"
```

---

## Task 4: luau-hir lowering — expressions

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

This task lowers expressions only. Statements come in Task 5. We need a `Lowerer` that carries scope state during the walk.

- [ ] **Step 1: Add scope machinery to `crates/luau-hir/src/lower.rs`**

Replace the file's contents with:

```rust
//! AST → HIR lowering. Plan 1 supports the subset documented in the plan.

use crate::{
    BinOp, HirError, HirExpr, HirFunction, HirLiteral, HirProgram, HirStmt, Symbol, SymbolId,
    SymbolKind, UnOp,
};
use luau_parse::Ast;
use std::collections::HashMap;

/// Lowerer state — manages scopes and the symbol table.
struct Lowerer {
    /// All declared symbols. Indexed by SymbolId.0 - 1 (ids start at 1).
    symbols: Vec<Symbol>,
    /// Stack of lexical scopes. Each scope maps source name → SymbolId for locals.
    /// Globals are NOT in any scope; they're resolved by absence.
    scopes: Vec<HashMap<String, SymbolId>>,
}

impl Lowerer {
    fn new() -> Self {
        Self { symbols: Vec::new(), scopes: vec![HashMap::new()] }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_local(&mut self, name: &str) -> SymbolId {
        let id = SymbolId::fresh();
        self.symbols.push(Symbol { id, kind: SymbolKind::Local, name: name.to_string() });
        self.scopes.last_mut().unwrap().insert(name.to_string(), id);
        id
    }

    fn declare_global(&mut self, name: &str) -> SymbolId {
        // Globals are not added to any scope. We still record them in the symbol
        // table so later stages can look up the name. Each reference to the same
        // global gets its own SymbolId — that's fine because the name field is
        // what carries semantic identity for globals.
        let id = SymbolId::fresh();
        self.symbols.push(Symbol { id, kind: SymbolKind::Global, name: name.to_string() });
        id
    }

    /// Resolve a source name to a SymbolId. Searches local scopes from innermost
    /// out; if not found, treats it as a global reference.
    fn resolve(&mut self, name: &str) -> SymbolId {
        for scope in self.scopes.iter().rev() {
            if let Some(&id) = scope.get(name) {
                return id;
            }
        }
        self.declare_global(name)
    }
}

pub fn lower(ast: &Ast) -> Result<HirProgram, HirError> {
    let mut lowerer = Lowerer::new();
    let main = lower_block(&mut lowerer, ast.nodes())?;
    Ok(HirProgram { symbols: lowerer.symbols, main })
}

fn lower_block(
    _lowerer: &mut Lowerer,
    _block: &full_moon::ast::Block,
) -> Result<Vec<HirStmt>, HirError> {
    todo!("implemented in task 5")
}

fn lower_expr(
    lowerer: &mut Lowerer,
    expr: &full_moon::ast::Expression,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Expression as E;
    match expr {
        E::Number(token) => {
            let s = token.token().to_string();
            let n: f64 = s.parse().map_err(|_| {
                HirError::Unsupported(format!("could not parse numeric literal {s:?}"))
            })?;
            Ok(HirExpr::Literal(HirLiteral::Number(n)))
        }
        E::String(token) => {
            let raw = token.token().to_string();
            let unquoted = strip_string_quotes(&raw)?;
            Ok(HirExpr::Literal(HirLiteral::String(unquoted)))
        }
        E::Symbol(sym) => {
            // `nil`, `true`, `false`, or `...`.
            let s = sym.token().to_string();
            match s.as_str() {
                "nil" => Ok(HirExpr::Literal(HirLiteral::Nil)),
                "true" => Ok(HirExpr::Literal(HirLiteral::Bool(true))),
                "false" => Ok(HirExpr::Literal(HirLiteral::Bool(false))),
                other => Err(HirError::Unsupported(format!(
                    "symbol expression {other:?}"
                ))),
            }
        }
        E::BinaryOperator { lhs, binop, rhs } => {
            let op = lower_binop(binop)?;
            Ok(HirExpr::BinOp(
                op,
                Box::new(lower_expr(lowerer, lhs)?),
                Box::new(lower_expr(lowerer, rhs)?),
            ))
        }
        E::UnaryOperator { unop, expression } => {
            let op = lower_unop(unop)?;
            Ok(HirExpr::UnOp(op, Box::new(lower_expr(lowerer, expression)?)))
        }
        E::Parentheses { expression, .. } => lower_expr(lowerer, expression),
        E::Var(var) => lower_var(lowerer, var),
        E::FunctionCall(call) => lower_call(lowerer, call),
        E::TypeAssertion { expression, .. } => {
            // `expr :: T` — type assertion. Strip and lower inner.
            lower_expr(lowerer, expression)
        }
        other => Err(HirError::Unsupported(format!("expression form {other:?}"))),
    }
}

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
        Var::Expression(_) => Err(HirError::Unsupported(
            "indexed var (Plan 1 has no tables)".into(),
        )),
        other => Err(HirError::Unsupported(format!("var form {other:?}"))),
    }
}

fn lower_call(
    lowerer: &mut Lowerer,
    call: &full_moon::ast::FunctionCall,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::{Call, FunctionArgs, Prefix, Suffix};
    let prefix = match call.prefix() {
        Prefix::Name(token) => HirExpr::Symbol(lowerer.resolve(&token.token().to_string())),
        Prefix::Expression(expr) => lower_expr(lowerer, expr)?,
        other => return Err(HirError::Unsupported(format!("call prefix {other:?}"))),
    };
    let mut current = prefix;
    for suffix in call.suffixes() {
        match suffix {
            Suffix::Call(Call::AnonymousCall(args)) => {
                let args = lower_call_args(lowerer, args)?;
                current = HirExpr::Call { callee: Box::new(current), args };
            }
            Suffix::Call(Call::MethodCall(_)) => {
                return Err(HirError::Unsupported("method call (a:b())".into()));
            }
            Suffix::Index(_) => {
                return Err(HirError::Unsupported("index suffix (a.b / a[b])".into()));
            }
            other => return Err(HirError::Unsupported(format!("call suffix {other:?}"))),
        }
    }
    Ok(current)
}

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
        FunctionArgs::TableConstructor(_) => {
            Err(HirError::Unsupported("table-as-call-arg (Plan 1 has no tables)".into()))
        }
        other => Err(HirError::Unsupported(format!("call args {other:?}"))),
    }
}

fn lower_binop(op: &full_moon::ast::BinOp) -> Result<BinOp, HirError> {
    use full_moon::ast::BinOp as B;
    Ok(match op {
        B::Plus(_) => BinOp::Add,
        B::Minus(_) => BinOp::Sub,
        B::Star(_) => BinOp::Mul,
        B::Slash(_) => BinOp::Div,
        B::Percent(_) => BinOp::Mod,
        B::Caret(_) => BinOp::Pow,
        B::TwoDots(_) => BinOp::Concat,
        B::TwoEqual(_) => BinOp::Eq,
        B::TildeEqual(_) => BinOp::Ne,
        B::LessThan(_) => BinOp::Lt,
        B::LessThanEqual(_) => BinOp::Le,
        B::GreaterThan(_) => BinOp::Gt,
        B::GreaterThanEqual(_) => BinOp::Ge,
        B::And(_) => BinOp::And,
        B::Or(_) => BinOp::Or,
        other => return Err(HirError::Unsupported(format!("binop {other:?}"))),
    })
}

fn lower_unop(op: &full_moon::ast::UnOp) -> Result<UnOp, HirError> {
    use full_moon::ast::UnOp as U;
    Ok(match op {
        U::Minus(_) => UnOp::Neg,
        U::Not(_) => UnOp::Not,
        U::Hash(_) => UnOp::Len,
        other => return Err(HirError::Unsupported(format!("unop {other:?}"))),
    })
}

fn strip_string_quotes(raw: &str) -> Result<String, HirError> {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return Ok(unescape(&raw[1..raw.len() - 1]));
        }
    }
    Err(HirError::Unsupported(format!(
        "unsupported string literal form: {raw:?} (Plan 1 only handles \"\" and '')"
    )))
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}
```

- [ ] **Step 2: Add unit tests at the bottom of `crates/luau-hir/src/lower.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lower_str(src: &str) -> HirProgram {
        let ast = luau_parse::parse(src).expect("parse");
        // Patch: tests skip lower_block for now (Task 5) and call lower_expr
        // directly via the helper below.
        HirProgram { symbols: Vec::new(), main: Vec::new() }
    }

    fn lower_one_expr(src: &str) -> HirExpr {
        // Parse `return <src>` to coax full_moon into giving us an expression.
        let wrapped = format!("return {src}");
        let ast = luau_parse::parse(&wrapped).expect("parse");
        let stmt = ast.nodes().last_stmt().expect("has last stmt");
        let full_moon::ast::LastStmt::Return(ret) = stmt else {
            panic!("expected return");
        };
        let expr = ret.returns().iter().next().expect("has expr");
        let mut lowerer = Lowerer::new();
        lower_expr(&mut lowerer, expr).expect("lower")
    }

    #[test]
    fn nil_literal() {
        assert!(matches!(lower_one_expr("nil"), HirExpr::Literal(HirLiteral::Nil)));
    }

    #[test]
    fn bool_literals() {
        assert!(matches!(lower_one_expr("true"), HirExpr::Literal(HirLiteral::Bool(true))));
        assert!(matches!(lower_one_expr("false"), HirExpr::Literal(HirLiteral::Bool(false))));
    }

    #[test]
    fn number_literal() {
        let e = lower_one_expr("42");
        let HirExpr::Literal(HirLiteral::Number(n)) = e else { panic!() };
        assert_eq!(n, 42.0);
    }

    #[test]
    fn string_literal_with_escape() {
        let e = lower_one_expr(r#""ab\nc""#);
        let HirExpr::Literal(HirLiteral::String(s)) = e else { panic!() };
        assert_eq!(s, "ab\nc");
    }

    #[test]
    fn binop_add() {
        let e = lower_one_expr("1 + 2");
        let HirExpr::BinOp(BinOp::Add, l, r) = e else { panic!() };
        assert!(matches!(*l, HirExpr::Literal(HirLiteral::Number(n)) if n == 1.0));
        assert!(matches!(*r, HirExpr::Literal(HirLiteral::Number(n)) if n == 2.0));
    }

    #[test]
    fn unop_neg() {
        let e = lower_one_expr("-5");
        assert!(matches!(e, HirExpr::UnOp(UnOp::Neg, _)));
    }

    #[test]
    fn global_call() {
        let e = lower_one_expr("print(1)");
        let HirExpr::Call { callee, args } = e else { panic!() };
        assert!(matches!(*callee, HirExpr::Symbol(_)));
        assert_eq!(args.len(), 1);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p luau-hir --lib lower::tests`
Expected: all 7 tests pass. The `lower_str` helper is unused yet — that's fine, Task 5 wires it up.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-hir
git commit -m "feat(hir): lower Luau expressions to HIR"
```

---

## Task 5: luau-hir lowering — statements and functions

**Files:**
- Modify: `crates/luau-hir/src/lower.rs`

- [ ] **Step 1: Implement `lower_block` and statement lowering in `crates/luau-hir/src/lower.rs`**

Replace the `lower_block` stub (and add helpers above the `#[cfg(test)]` block):

```rust
fn lower_block(
    lowerer: &mut Lowerer,
    block: &full_moon::ast::Block,
) -> Result<Vec<HirStmt>, HirError> {
    let mut out = Vec::new();
    for stmt in block.stmts() {
        out.extend(lower_stmt(lowerer, stmt)?);
    }
    if let Some(last) = block.last_stmt() {
        out.push(lower_last_stmt(lowerer, last)?);
    }
    Ok(out)
}

fn lower_stmt(
    lowerer: &mut Lowerer,
    stmt: &full_moon::ast::Stmt,
) -> Result<Vec<HirStmt>, HirError> {
    use full_moon::ast::Stmt;
    match stmt {
        Stmt::LocalAssignment(la) => {
            // `local a, b = e1, e2` — Plan 1: lower to multiple LocalDecls.
            let names: Vec<&full_moon::tokenizer::TokenReference> = la.names().iter().collect();
            let exprs: Vec<&full_moon::ast::Expression> = la.expressions().iter().collect();
            if names.len() != 1 && !exprs.is_empty() && exprs.len() != names.len() {
                return Err(HirError::Unsupported(
                    "multi-decl with mismatched rhs (Plan 1 has no multi-return)".into(),
                ));
            }
            let mut out = Vec::with_capacity(names.len());
            for (i, name_tok) in names.iter().enumerate() {
                let value = if let Some(expr) = exprs.get(i) {
                    lower_expr(lowerer, expr)?
                } else {
                    HirExpr::Literal(HirLiteral::Nil)
                };
                let symbol = lowerer.declare_local(&name_tok.token().to_string());
                out.push(HirStmt::LocalDecl { symbol, value });
            }
            Ok(out)
        }
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
                let target = match var {
                    full_moon::ast::Var::Name(tok) => lowerer.resolve(&tok.token().to_string()),
                    other => {
                        return Err(HirError::Unsupported(format!(
                            "assign target form {other:?}"
                        )))
                    }
                };
                let value = lower_expr(lowerer, expr)?;
                out.push(HirStmt::Assign { target, value });
            }
            Ok(out)
        }
        Stmt::FunctionCall(call) => {
            // We lower the call as an expression and wrap in ExprStmt.
            // lower_call needs a FunctionCall, which is exactly `call`.
            let e = lower_call(lowerer, call)?;
            Ok(vec![HirStmt::ExprStmt(e)])
        }
        Stmt::If(if_stmt) => {
            let cond = lower_expr(lowerer, if_stmt.condition())?;
            lowerer.push_scope();
            let then_body = lower_block(lowerer, if_stmt.block())?;
            lowerer.pop_scope();
            // Build else from elseif chain and else block, nesting right-to-left.
            let mut tail: Vec<HirStmt> = if let Some(else_block) = if_stmt.else_block() {
                lowerer.push_scope();
                let b = lower_block(lowerer, else_block)?;
                lowerer.pop_scope();
                b
            } else {
                Vec::new()
            };
            if let Some(elseifs) = if_stmt.else_if() {
                for ei in elseifs.iter().rev() {
                    let ei_cond = lower_expr(lowerer, ei.condition())?;
                    lowerer.push_scope();
                    let ei_body = lower_block(lowerer, ei.block())?;
                    lowerer.pop_scope();
                    tail = vec![HirStmt::If {
                        cond: ei_cond,
                        then_body: ei_body,
                        else_body: tail,
                    }];
                }
            }
            Ok(vec![HirStmt::If { cond, then_body, else_body: tail }])
        }
        Stmt::While(w) => {
            let cond = lower_expr(lowerer, w.condition())?;
            lowerer.push_scope();
            let body = lower_block(lowerer, w.block())?;
            lowerer.pop_scope();
            Ok(vec![HirStmt::While { cond, body }])
        }
        Stmt::Repeat(r) => {
            // `repeat body until cond` — in Lua, the until cond is in the scope
            // of the body's locals. We keep that semantics by lowering cond inside
            // the body's scope.
            lowerer.push_scope();
            let body = lower_block(lowerer, r.block())?;
            let cond = lower_expr(lowerer, r.until())?;
            lowerer.pop_scope();
            Ok(vec![HirStmt::Repeat { cond, body }])
        }
        Stmt::NumericFor(nf) => {
            let start = lower_expr(lowerer, nf.start())?;
            let stop = lower_expr(lowerer, nf.end())?;
            let step = match nf.step() {
                Some(e) => lower_expr(lowerer, e)?,
                None => HirExpr::Literal(HirLiteral::Number(1.0)),
            };
            lowerer.push_scope();
            let var = lowerer.declare_local(&nf.index_variable().token().to_string());
            let body = lower_block(lowerer, nf.block())?;
            lowerer.pop_scope();
            Ok(vec![HirStmt::NumericFor { var, start, stop, step, body }])
        }
        Stmt::FunctionDeclaration(fd) => {
            // Plan 1: only globally-named functions (single-name, no method).
            let name_path: Vec<&full_moon::tokenizer::TokenReference> =
                fd.name().names().iter().collect();
            if name_path.len() != 1 || fd.name().method_colon().is_some() {
                return Err(HirError::Unsupported(
                    "qualified or method-style function decl (Plan 1: top-level only)".into(),
                ));
            }
            let name = name_path[0].token().to_string();
            let symbol = lowerer.resolve(&name);
            // The function is invoked by name as a global; ensure it resolved to global.
            // (If `name` happened to shadow a local in scope, full Lua would assign through.
            // For Plan 1 we assume top-level function decl always targets a global.)
            let function = lower_function_body(lowerer, fd.body())?;
            Ok(vec![HirStmt::FunctionDecl { name: symbol, function }])
        }
        Stmt::LocalFunction(_) => Err(HirError::Unsupported(
            "`local function` (closures with upvalues are Plan 2)".into(),
        )),
        other => Err(HirError::Unsupported(format!("statement form {other:?}"))),
    }
}

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
                1 => Ok(HirStmt::Return(Some(lower_expr(lowerer, exprs[0])?))),
                _ => Err(HirError::Unsupported(
                    "multi-return (Plan 1 has 0 or 1 return value)".into(),
                )),
            }
        }
        LastStmt::Break(_) => Err(HirError::Unsupported("break (Plan 2)".into())),
        LastStmt::Continue(_) => Err(HirError::Unsupported("continue (Plan 2)".into())),
        other => Err(HirError::Unsupported(format!("last stmt form {other:?}"))),
    }
}

fn lower_function_body(
    lowerer: &mut Lowerer,
    body: &full_moon::ast::FunctionBody,
) -> Result<HirFunction, HirError> {
    lowerer.push_scope();
    let mut params = Vec::new();
    for p in body.parameters() {
        use full_moon::ast::Parameter;
        match p {
            Parameter::Name(tok) => {
                params.push(lowerer.declare_local(&tok.token().to_string()));
            }
            Parameter::Ellipsis(_) => {
                lowerer.pop_scope();
                return Err(HirError::Unsupported("varargs `...` (Plan 2)".into()));
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

- [ ] **Step 2: Update `lower_str` test helper at the bottom**

Replace the existing `lower_str` (which was a placeholder) with:

```rust
    fn lower_str(src: &str) -> HirProgram {
        let ast = luau_parse::parse(src).expect("parse");
        lower(&ast).expect("lower")
    }
```

- [ ] **Step 3: Add statement tests at the bottom of `crates/luau-hir/src/lower.rs`**

Append inside the `tests` mod:

```rust
    #[test]
    fn lowers_local_decl_with_value() {
        let p = lower_str("local x = 5");
        assert_eq!(p.main.len(), 1);
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        assert!(matches!(value, HirExpr::Literal(HirLiteral::Number(n)) if *n == 5.0));
    }

    #[test]
    fn lowers_local_decl_no_value() {
        let p = lower_str("local x");
        let HirStmt::LocalDecl { value, .. } = &p.main[0] else { panic!() };
        assert!(matches!(value, HirExpr::Literal(HirLiteral::Nil)));
    }

    #[test]
    fn lowers_global_assign() {
        let p = lower_str("x = 5");
        assert!(matches!(&p.main[0], HirStmt::Assign { .. }));
    }

    #[test]
    fn lowers_if_else() {
        let p = lower_str("if x then y = 1 else y = 2 end");
        let HirStmt::If { then_body, else_body, .. } = &p.main[0] else { panic!() };
        assert_eq!(then_body.len(), 1);
        assert_eq!(else_body.len(), 1);
    }

    #[test]
    fn lowers_if_elseif_else_as_nested() {
        let p = lower_str("if a then x=1 elseif b then x=2 else x=3 end");
        let HirStmt::If { else_body, .. } = &p.main[0] else { panic!() };
        // else_body should hold one nested If for the elseif.
        assert_eq!(else_body.len(), 1);
        assert!(matches!(&else_body[0], HirStmt::If { .. }));
    }

    #[test]
    fn lowers_while() {
        let p = lower_str("while x do y = 1 end");
        assert!(matches!(&p.main[0], HirStmt::While { .. }));
    }

    #[test]
    fn lowers_numeric_for_default_step() {
        let p = lower_str("for i = 1, 10 do x = i end");
        let HirStmt::NumericFor { step, .. } = &p.main[0] else { panic!() };
        assert!(matches!(step, HirExpr::Literal(HirLiteral::Number(n)) if *n == 1.0));
    }

    #[test]
    fn lowers_global_function_decl() {
        let p = lower_str("function f(x) return x end");
        let HirStmt::FunctionDecl { function, .. } = &p.main[0] else { panic!() };
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.body.len(), 1);
    }

    #[test]
    fn rejects_local_function() {
        let ast = luau_parse::parse("local function f() end").unwrap();
        assert!(lower(&ast).is_err());
    }
```

- [ ] **Step 4: Run all hir tests**

Run: `cargo test -p luau-hir`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-hir
git commit -m "feat(hir): lower statements, control flow, and global functions"
```

---

## Task 6: luau-mir types

**Files:**
- Create: `crates/luau-mir/Cargo.toml`
- Create: `crates/luau-mir/src/lib.rs`
- Create: `crates/luau-mir/src/types.rs`

- [ ] **Step 1: Create `crates/luau-mir/Cargo.toml`**

```toml
[package]
name = "luau-mir"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-hir = { path = "../luau-hir" }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-mir/src/types.rs`**

```rust
//! Mid-level IR. Sugar-free, CFG-shaped.
//!
//! The program is a list of MIR functions plus an implicit "main" function
//! (function index 0) representing the top-level chunk.

use luau_hir::{BinOp, SymbolId, UnOp};

/// A virtual local — an SSA-ish symbolic slot. Register allocation happens in LIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VLocal(pub u32);

/// A basic block id, scoped to its function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// A constant pool index, scoped to its function for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstId(pub u32);

/// A function id, scoped to the whole MirProgram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

/// A "value" — operand to an instruction.
#[derive(Debug, Clone, Copy)]
pub enum Value {
    VLocal(VLocal),
    Const(ConstId),
}

/// MIR instructions. All flow through a destination VLocal.
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
    /// Call a function: `dst = callee(args...)`. Plan 1 always takes 1 result.
    /// If `dst` is None the result is discarded (statement-position call).
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Create a closure from a function id, write to dst. Plan 1: no upvalues.
    MakeClosure { dst: VLocal, function: FunctionId },
}

/// Block terminator — exactly one per block.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Unconditional jump.
    Goto(BlockId),
    /// If `cond` is truthy go to `then_block`, else `else_block`.
    Branch { cond: Value, then_block: BlockId, else_block: BlockId },
    /// Return — Plan 1: 0 or 1 value.
    Return(Option<Value>),
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instrs: Vec<Instr>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub id: FunctionId,
    /// Parameter VLocals, in order.
    pub params: Vec<VLocal>,
    /// All blocks in this function; `blocks[0]` is always the entry.
    pub blocks: Vec<BasicBlock>,
    /// Constant pool for this function.
    pub consts: Vec<Constant>,
    /// Total number of VLocals (used for LIR's regalloc).
    pub n_locals: u32,
}

impl MirFunction {
    pub fn entry(&self) -> BlockId {
        self.blocks[0].id
    }
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        self.blocks.iter().find(|b| b.id == id).expect("block exists")
    }
    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        self.blocks.iter_mut().find(|b| b.id == id).expect("block exists")
    }
}

#[derive(Debug, Clone)]
pub struct MirProgram {
    /// Index 0 is the main chunk.
    pub functions: Vec<MirFunction>,
}

impl MirProgram {
    pub fn main(&self) -> &MirFunction {
        &self.functions[0]
    }
}

/// Mapping from source SymbolId → VLocal, scoped per function during lowering.
/// (Public so passes can introspect for tests; not used by codegen.)
pub type SymbolMap = std::collections::HashMap<SymbolId, VLocal>;
```

- [ ] **Step 3: Create `crates/luau-mir/src/lib.rs`**

```rust
//! MIR — mid-level IR for Luau. HIR → MIR lowering.

mod types;
pub mod lower;

pub use types::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MirError {
    #[error("hir error: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("unsupported in Plan 1 subset: {0}")]
    Unsupported(String),
}
```

- [ ] **Step 4: Create stub `crates/luau-mir/src/lower.rs`**

```rust
//! HIR → MIR lowering. Implemented in subsequent tasks.

use crate::{MirError, MirProgram};
use luau_hir::HirProgram;

pub fn lower(_hir: &HirProgram) -> Result<MirProgram, MirError> {
    todo!("implemented in subsequent tasks")
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p luau-mir`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-mir
git commit -m "feat(mir): define MIR types"
```

---

## Task 7: luau-mir lowering — function builder and expressions

**Files:**
- Modify: `crates/luau-mir/src/lower.rs`

The MIR lowerer is structured around a `FnBuilder` that owns a single function's mutable state during the walk: VLocal counter, current block, list of blocks, constant pool, and a symbol→VLocal map.

- [ ] **Step 1: Replace `crates/luau-mir/src/lower.rs`**

```rust
//! HIR → MIR lowering. Builds the CFG and desugars short-circuit operators.

use crate::{
    BasicBlock, BlockId, Constant, ConstId, FunctionId, Instr, MirError, MirFunction, MirProgram,
    SymbolMap, Terminator, VLocal, Value,
};
use luau_hir::{
    BinOp, HirExpr, HirFunction, HirLiteral, HirProgram, HirStmt, Symbol, SymbolId, SymbolKind,
    UnOp,
};

struct FnBuilder<'a> {
    id: FunctionId,
    next_local: u32,
    blocks: Vec<BasicBlock>,
    next_block: u32,
    current: Option<BlockId>,
    consts: Vec<Constant>,
    symbols: SymbolMap,
    /// Read-only view of all symbols, indexed by SymbolId.0.
    all_symbols: &'a [Symbol],
    /// Used to emit MakeClosure ops. Owned by Program builder; FnBuilder appends.
    pending_functions: &'a mut Vec<(FunctionId, &'a HirFunction)>,
    next_function: &'a mut u32,
    params: Vec<VLocal>,
}

impl<'a> FnBuilder<'a> {
    fn new(
        id: FunctionId,
        all_symbols: &'a [Symbol],
        pending_functions: &'a mut Vec<(FunctionId, &'a HirFunction)>,
        next_function: &'a mut u32,
    ) -> Self {
        Self {
            id,
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
        }
    }

    fn fresh_local(&mut self) -> VLocal {
        let v = VLocal(self.next_local);
        self.next_local += 1;
        v
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            instrs: Vec::new(),
            terminator: Terminator::Return(None), // placeholder
        });
        id
    }

    fn switch_to(&mut self, block: BlockId) {
        self.current = Some(block);
    }

    fn emit(&mut self, instr: Instr) {
        let id = self.current.expect("no current block");
        let block = self.blocks.iter_mut().find(|b| b.id == id).expect("block exists");
        block.instrs.push(instr);
    }

    fn set_terminator(&mut self, term: Terminator) {
        let id = self.current.expect("no current block");
        let block = self.blocks.iter_mut().find(|b| b.id == id).expect("block exists");
        block.terminator = term;
    }

    fn intern_const(&mut self, c: Constant) -> ConstId {
        if let Some((i, _)) = self.consts.iter().enumerate().find(|(_, x)| **x == c) {
            return ConstId(i as u32);
        }
        let id = ConstId(self.consts.len() as u32);
        self.consts.push(c);
        id
    }

    /// VLocal for a local-kind symbol; creates one on first reference.
    fn local_for(&mut self, sym: SymbolId) -> VLocal {
        if let Some(&v) = self.symbols.get(&sym) {
            return v;
        }
        let v = self.fresh_local();
        self.symbols.insert(sym, v);
        v
    }

    /// True if `sym` is a global per the original HIR symbol table.
    fn is_global(&self, sym: SymbolId) -> bool {
        // Symbols are appended with ids starting at 1.
        // We look up by linear scan; HIR symbols vector is small for now.
        self.all_symbols
            .iter()
            .find(|s| s.id == sym)
            .map(|s| s.kind == SymbolKind::Global)
            .unwrap_or(false)
    }

    /// Name of a symbol (used for globals' fenv key).
    fn name_of(&self, sym: SymbolId) -> &str {
        &self.all_symbols.iter().find(|s| s.id == sym).expect("symbol").name
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

    /// Lower an expression, returning the VLocal that holds the result.
    fn lower_expr(&mut self, e: &HirExpr) -> Result<VLocal, MirError> {
        match e {
            HirExpr::Literal(lit) => {
                let c = self.lower_literal(lit);
                let dst = self.fresh_local();
                self.emit(Instr::LoadConst { dst, src: c });
                Ok(dst)
            }
            HirExpr::Symbol(sym) => {
                if self.is_global(*sym) {
                    let name_const = self
                        .intern_const(Constant::String(self.name_of(*sym).to_string()));
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
        }
    }

    /// `a and b` / `a or b`: branchy lowering so short-circuit is preserved.
    /// Result VLocal is materialized in the join block.
    fn lower_short_circuit(
        &mut self,
        op: BinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> Result<VLocal, MirError> {
        let result = self.fresh_local();
        let lhs_v = self.lower_expr(lhs)?;
        // Move lhs into result first (it's the answer if we short-circuit).
        self.emit(Instr::Move { dst: result, src: lhs_v });
        let eval_rhs = self.new_block();
        let join = self.new_block();
        let (then_b, else_b) = match op {
            // `and`: if lhs truthy, evaluate rhs; else short-circuit with lhs.
            BinOp::And => (eval_rhs, join),
            // `or`:  if lhs truthy, short-circuit with lhs; else evaluate rhs.
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
    // queue: (function id, HIR function)
    let mut pending: Vec<(FunctionId, &HirFunction)> = Vec::new();

    // Main chunk wraps top-level stmts as a parameterless function.
    let main_hir = HirFunction { params: Vec::new(), body: hir.main.clone() };
    let main_id = FunctionId(next_function);
    next_function += 1;
    pending.push((main_id, &main_hir));

    while let Some((fid, fhir)) = pending.pop() {
        let mut builder =
            FnBuilder::new(fid, &hir.symbols, &mut pending, &mut next_function);
        // Entry block.
        let entry = builder.new_block();
        builder.switch_to(entry);
        // Params get fresh locals; record them.
        for p in &fhir.params {
            let v = builder.fresh_local();
            builder.symbols.insert(*p, v);
            builder.params.push(v);
        }
        crate::lower::lower_stmts(&mut builder, &fhir.body)?;
        // If the last terminator hasn't been set (control falls off), emit Return(None).
        ensure_terminated(&mut builder);

        program_functions.push(MirFunction {
            id: fid,
            params: builder.params,
            blocks: builder.blocks,
            consts: builder.consts,
            n_locals: builder.next_local,
        });
    }
    // Sort functions by id so functions[0] is the main chunk.
    program_functions.sort_by_key(|f| f.id.0);
    Ok(MirProgram { functions: program_functions })
}

fn ensure_terminated(b: &mut FnBuilder) {
    // If the current block's terminator is still the default Return(None), leave it.
    // (We initialized blocks with Return(None) as placeholder; that's a valid terminator.)
    let _ = b;
}

pub(crate) fn lower_stmts(b: &mut FnBuilder, stmts: &[HirStmt]) -> Result<(), MirError> {
    // Statement lowering is implemented in Task 8. We need it referenced here
    // to keep this file's structure consistent; for now it's a stub.
    crate::lower_stmts::lower(b, stmts)
}
```

- [ ] **Step 2: Create the supporting module file `crates/luau-mir/src/lower_stmts.rs`**

```rust
//! Placeholder — real implementation lands in Task 8.

use crate::{lower::FnBuilder, MirError};
use luau_hir::HirStmt;

pub(crate) fn lower(_b: &mut FnBuilder, _stmts: &[HirStmt]) -> Result<(), MirError> {
    todo!("Task 8 implements statement lowering")
}
```

Wait — to keep the build clean, we need `FnBuilder` visible to `lower_stmts.rs`. Make it `pub(crate)` in `lower.rs` (replace `struct FnBuilder<'a>` with `pub(crate) struct FnBuilder<'a>` and likewise mark its impl methods `pub(crate)` where called from outside).

Open `crates/luau-mir/src/lower.rs` and change:
- `struct FnBuilder<'a> {` → `pub(crate) struct FnBuilder<'a> {`
- `impl<'a> FnBuilder<'a> {` — keep methods private; only `fresh_local`, `new_block`, `switch_to`, `emit`, `set_terminator`, `intern_const`, `local_for`, `is_global`, `name_of`, `lower_expr`, `symbols`, `params` need `pub(crate)` access from `lower_stmts.rs`. Simplest: mark them all `pub(crate)`. Also `lower_short_circuit`.

- [ ] **Step 3: Add module declaration in `crates/luau-mir/src/lib.rs`**

Inside `lib.rs` add `mod lower_stmts;` next to `pub mod lower;`.

- [ ] **Step 4: Add a basic expression test in `crates/luau-mir/src/lower.rs`**

Append at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::lower::lower as hir_lower;

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).expect("parse");
        let hir = hir_lower(&ast).expect("hir");
        // Plan 1 — most statement tests are deferred until Task 8.
        // For now, verify that lower() runs to the FnBuilder construction.
        // Use a program with only a return-expression-free body to avoid Task 8 stub.
        super::lower(&hir).expect("mir")
    }

    // No tests yet — Task 8 supplies them once statement lowering exists.
    #[test]
    fn placeholder_compiles() {
        // Empty source — main chunk has no statements; lower should succeed.
        let _ = mir_of("");
    }
}
```

- [ ] **Step 5: Run cargo check**

Run: `cargo check -p luau-mir`
Expected: compiles (the `todo!()` in `lower_stmts.rs` will only panic at runtime if invoked).

Run: `cargo test -p luau-mir --lib lower::tests::placeholder_compiles`
Expected: passes (empty source uses no statements, so the `todo!()` is not hit). If it hits the `todo!()` it's because the empty-body lowering path still calls `lower_stmts`; in that case skip this step and move on — Task 8 wires it.

- [ ] **Step 6: Commit**

```bash
git add crates/luau-mir
git commit -m "feat(mir): expression lowering and FnBuilder scaffolding"
```

---

## Task 8: luau-mir lowering — statements and control flow

**Files:**
- Modify: `crates/luau-mir/src/lower_stmts.rs`

- [ ] **Step 1: Replace `crates/luau-mir/src/lower_stmts.rs`**

```rust
//! HIR statement → MIR lowering. Handles control flow desugaring.

use crate::{
    lower::FnBuilder, Constant, FunctionId, Instr, MirError, Terminator, VLocal, Value,
};
use luau_hir::{HirExpr, HirLiteral, HirStmt, SymbolKind};

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
                let name = b.intern_const(Constant::String(b.name_of(*target).to_string()));
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
            // Calls in statement position discard their result.
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
                    // Any other expression in statement position is unusual but legal;
                    // lower for side effects and discard.
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
            lower(b, body)?;
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(exit);
            Ok(())
        }
        HirStmt::Repeat { cond, body } => {
            let body_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(body_block));
            b.switch_to(body_block);
            lower(b, body)?;
            let cond_v = b.lower_expr(cond)?;
            // until cond — exit when truthy.
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
            // Materialize stop and step into stable locals so they don't get
            // clobbered between iterations.
            let stop_slot = b.fresh_local();
            b.emit(Instr::Move { dst: stop_slot, src: stop_v });
            let step_slot = b.fresh_local();
            b.emit(Instr::Move { dst: step_slot, src: step_v });

            let header = b.new_block();
            let body_block = b.new_block();
            let exit = b.new_block();
            b.set_terminator(Terminator::Goto(header));
            b.switch_to(header);
            // Test: if step >= 0 then i <= stop else i >= stop.
            // Plan 1 simplification: support only step > 0 (compile-time numeric default 1).
            // We emit `i <= stop` and document the limitation; Plan 2 generalizes.
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
            lower(b, body)?;
            // i = i + step
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
            // Anything after a return in the same source block is unreachable;
            // start a fresh block so subsequent emissions don't clobber the
            // terminator (the new block may be removed as dead by later passes).
            let dead = b.new_block();
            b.switch_to(dead);
            Ok(())
        }
        HirStmt::FunctionDecl { name, function } => {
            // Queue this function for lowering.
            let fid = b.queue_function(function);
            // Emit: closure := MakeClosure(fid); SetGlobal name := closure
            let closure_dst = b.fresh_local();
            b.emit(Instr::MakeClosure { dst: closure_dst, function: fid });
            if b.is_global(*name) {
                let name_const = b.intern_const(Constant::String(b.name_of(*name).to_string()));
                b.emit(Instr::SetGlobal { name: name_const, value: Value::VLocal(closure_dst) });
            } else {
                // Plan 1: function decl always targets a global, but be defensive.
                let slot = b.local_for(*name);
                b.emit(Instr::Move { dst: slot, src: closure_dst });
            }
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Add `queue_function` to `FnBuilder` in `crates/luau-mir/src/lower.rs`**

Inside the `impl<'a> FnBuilder<'a>` block add:

```rust
    pub(crate) fn queue_function(&mut self, f: &'a luau_hir::HirFunction) -> FunctionId {
        let id = FunctionId(*self.next_function);
        *self.next_function += 1;
        self.pending_functions.push((id, f));
        id
    }
```

- [ ] **Step 3: Add a structural MIR test in `crates/luau-mir/src/lower.rs`**

Replace the `tests` mod content:

```rust
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
        // The Move may be elided if dst == src (FnBuilder elides equal-slot moves).
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
        // Header branches.
        let header = &p.main().blocks[1];
        assert!(matches!(header.terminator, Terminator::Branch { .. }));
    }

    #[test]
    fn function_decl_queues_function() {
        let p = mir_of("function f(x) return x end");
        // main + f
        assert_eq!(p.functions.len(), 2);
        let f = &p.functions[1];
        assert_eq!(f.params.len(), 1);
        // f's body: entry block, with Return(Some(...))
        assert!(matches!(f.blocks[0].terminator, Terminator::Return(Some(_))));
    }

    #[test]
    fn return_after_unreachable_creates_dead_block() {
        let p = mir_of("function f() return 1 end");
        // main + f. Inside f, after `return 1`, FnBuilder allocates a dead block.
        let f = &p.functions[1];
        assert!(f.blocks.len() >= 2, "expected dead-block sentinel after return");
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p luau-mir`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-mir
git commit -m "feat(mir): statement and control-flow lowering"
```

---

## Task 9: luau-passes pass trait + identity pass

**Files:**
- Create: `crates/luau-passes/Cargo.toml`
- Create: `crates/luau-passes/src/lib.rs`
- Create: `crates/luau-passes/src/identity.rs`

- [ ] **Step 1: Create `crates/luau-passes/Cargo.toml`**

```toml
[package]
name = "luau-passes"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-mir = { path = "../luau-mir" }
rand_chacha = { workspace = true }
rand = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-passes/src/lib.rs`**

```rust
//! Obfuscation pass registry and scheduler. Plan 1 ships an identity pass only.

use luau_mir::MirProgram;
use rand_chacha::ChaCha20Rng;

pub mod identity;

/// A MIR→MIR transform.
pub trait Pass {
    /// Stable short name for the pass — used in PassPlan and logs.
    fn name(&self) -> &'static str;

    /// Run the pass. Receives a seeded RNG to source any randomness from.
    /// Mutates the program in place.
    fn run(&self, program: &mut MirProgram, rng: &mut ChaCha20Rng);
}

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

/// The default plan for Plan 1: identity only.
pub fn default_plan() -> PassPlan {
    PassPlan::new().push(Box::new(identity::Identity))
}
```

- [ ] **Step 3: Create `crates/luau-passes/src/identity.rs`**

```rust
//! No-op pass. Useful as a smoke test that the pass scheduler works.

use crate::Pass;
use luau_mir::MirProgram;
use rand_chacha::ChaCha20Rng;

pub struct Identity;

impl Pass for Identity {
    fn name(&self) -> &'static str { "identity" }
    fn run(&self, _program: &mut MirProgram, _rng: &mut ChaCha20Rng) {
        // intentionally empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn identity_runs_without_panic() {
        let ast = luau_parse::parse("return 1").unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        let mut mir = luau_mir::lower::lower(&hir).unwrap();
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let n_funcs = mir.functions.len();
        Identity.run(&mut mir, &mut rng);
        assert_eq!(mir.functions.len(), n_funcs);
    }
}
```

The `identity.rs` test depends on `luau-hir` and `luau-parse`. Add them as dev-deps:

Edit `crates/luau-passes/Cargo.toml` to add:

```toml
[dev-dependencies]
luau-hir = { path = "../luau-hir" }
luau-parse = { path = "../luau-parse" }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p luau-passes`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-passes
git commit -m "feat(passes): pass trait, plan scheduler, identity pass"
```

---

## Task 10: luau-lir types and register allocator

**Files:**
- Create: `crates/luau-lir/Cargo.toml`
- Create: `crates/luau-lir/src/lib.rs`
- Create: `crates/luau-lir/src/types.rs`
- Create: `crates/luau-lir/src/regalloc.rs`

- [ ] **Step 1: Create `crates/luau-lir/Cargo.toml`**

```toml
[package]
name = "luau-lir"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-mir = { path = "../luau-mir" }
luau-hir = { path = "../luau-hir" }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-lir/src/types.rs`**

```rust
//! Low-level IR. One per VM proto. Bytecode is linear; jumps refer to
//! byte offsets that the emit stage finalizes.

use luau_hir::{BinOp, UnOp};
use luau_mir::Constant;

/// Physical register index, 0-based per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u16);

/// Constant pool index, per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstIdx(pub u16);

/// Function id, per program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtoId(pub u16);

/// Symbolic opcode. The actual numeric byte is assigned by `luau-emit::opmap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    LoadNil,        // R(A)
    LoadTrue,       // R(A)
    LoadFalse,      // R(A)
    LoadConst,      // R(A), K(B)
    Move,           // R(A), R(B)
    Add, Sub, Mul, Div, Mod, Pow,   // R(A), R(B), R(C)
    Concat,         // R(A), R(B), R(C)
    Lt, Le, Eq,     // R(A), R(B), R(C)   (Ne/Gt/Ge are lowered: see lower.rs)
    Not, Neg, Len,  // R(A), R(B)
    GetGlobal,      // R(A), K(B)         B is string const idx
    SetGlobal,      // K(A), R(B)
    Call,           // R(A) = R(B)(R(B+1)…R(B+n)), with C = n; A = 0xFFFF means discard
    Return,         // R(A) (or 0xFFFF for "no value")
    Jmp,            // signed offset (i16)
    JmpIfTrue,      // R(A), signed offset (i16)
    JmpIfFalse,     // R(A), signed offset (i16)
    Closure,        // R(A), proto idx (u16)
}

/// One LIR instruction = one VM opcode. Operands are kept symbolic in Vec<Operand>
/// so the encoder picks integer encoding per build later.
#[derive(Debug, Clone)]
pub struct LirInstr {
    pub op: OpKind,
    pub operands: Vec<Operand>,
}

/// LIR operand. Jump targets are block IDs at this stage; the emitter resolves
/// them to byte offsets after layout.
#[derive(Debug, Clone, Copy)]
pub enum Operand {
    Reg(Reg),
    Const(ConstIdx),
    Proto(ProtoId),
    JmpTarget(BlockLabel),
    SmallInt(i16),
}

/// A label within a function used as a jump target before byte-offset resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockLabel(pub u32);

#[derive(Debug, Clone)]
pub struct LirFunction {
    pub id: ProtoId,
    pub num_params: u16,
    /// Number of registers used by this function (>= num_params).
    pub num_regs: u16,
    pub consts: Vec<Constant>,
    /// Instructions in linear order.
    pub instrs: Vec<LirInstr>,
    /// For each label, the instruction index that label points to.
    pub label_positions: Vec<(BlockLabel, u32)>,
}

#[derive(Debug, Clone)]
pub struct LirProgram {
    pub functions: Vec<LirFunction>,
}
```

- [ ] **Step 3: Create `crates/luau-lir/src/regalloc.rs`**

```rust
//! Trivially-simple register allocator: assigns each VLocal a unique register.
//! Plan 2 swaps this for live-range-based allocation that scatters registers.

use luau_mir::{MirFunction, VLocal};
use std::collections::HashMap;

pub struct RegMap {
    map: HashMap<VLocal, u16>,
    next: u16,
}

impl RegMap {
    pub fn new() -> Self {
        Self { map: HashMap::new(), next: 0 }
    }

    pub fn assign_all(f: &MirFunction) -> Self {
        let mut rm = Self::new();
        // Params get registers 0..num_params first.
        for v in &f.params {
            rm.get_or_alloc(*v);
        }
        // Then every other VLocal seen anywhere in the function.
        for block in &f.blocks {
            for instr in &block.instrs {
                visit_vlocals(instr, |v| {
                    rm.get_or_alloc(v);
                });
            }
            visit_terminator_vlocals(&block.terminator, |v| {
                rm.get_or_alloc(v);
            });
        }
        rm
    }

    pub fn get(&self, v: VLocal) -> u16 {
        *self.map.get(&v).expect("vlocal must have a register")
    }

    pub fn get_or_alloc(&mut self, v: VLocal) -> u16 {
        if let Some(r) = self.map.get(&v) {
            *r
        } else {
            let r = self.next;
            self.next += 1;
            self.map.insert(v, r);
            r
        }
    }

    pub fn num_regs(&self) -> u16 {
        self.next
    }
}

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
    }
}

fn visit_terminator_vlocals(t: &luau_mir::Terminator, mut f: impl FnMut(VLocal)) {
    use luau_mir::{Terminator, Value};
    match t {
        Terminator::Goto(_) => {}
        Terminator::Branch { cond, .. } => {
            if let Value::VLocal(l) = cond { f(*l); }
        }
        Terminator::Return(Some(Value::VLocal(l))) => f(*l),
        Terminator::Return(_) => {}
    }
}
```

- [ ] **Step 4: Create `crates/luau-lir/src/lib.rs`** (lowering stays in `lower.rs`, added in Task 11)

```rust
//! LIR — low-level IR. Linear bytecode for the custom ISA.

mod types;
mod regalloc;
pub mod lower;

pub use types::*;
pub use regalloc::RegMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LirError {
    #[error("mir error: {0}")]
    Mir(#[from] luau_mir::MirError),
    #[error("internal: {0}")]
    Internal(String),
}
```

- [ ] **Step 5: Create stub `crates/luau-lir/src/lower.rs`**

```rust
//! MIR → LIR lowering. Implemented in Task 11.

use crate::{LirError, LirProgram};
use luau_mir::MirProgram;

pub fn lower(_mir: &MirProgram) -> Result<LirProgram, LirError> {
    todo!("Task 11 implements LIR lowering")
}
```

- [ ] **Step 6: Add a regalloc unit test in `crates/luau-lir/src/regalloc.rs`**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::{lower::lower as mir_lower, MirProgram};

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        mir_lower(&hir).unwrap()
    }

    #[test]
    fn assigns_registers_to_all_vlocals() {
        let p = mir_of("local x = 1 local y = x + 2");
        let rm = RegMap::assign_all(p.main());
        // Should assign at least one register per local + temporaries.
        assert!(rm.num_regs() >= 2);
    }
}
```

Add dev-deps to `crates/luau-lir/Cargo.toml`:

```toml
[dev-dependencies]
luau-parse = { path = "../luau-parse" }
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p luau-lir`
Expected: passes.

- [ ] **Step 8: Commit**

```bash
git add crates/luau-lir
git commit -m "feat(lir): types and simple register allocator"
```

---

## Task 11: luau-lir lowering — MIR → bytecode

**Files:**
- Modify: `crates/luau-lir/src/lower.rs`

LIR lowering is mechanical: walk each MIR function, emit one or more LIR instructions per MIR instruction, and emit jumps at block terminators. We use `BlockLabel`s for jump targets so we don't need to know byte offsets yet.

- [ ] **Step 1: Replace `crates/luau-lir/src/lower.rs`**

```rust
//! MIR → LIR lowering. One MIR function becomes one LIR function (proto).

use crate::{
    BlockLabel, ConstIdx, LirError, LirFunction, LirInstr, LirProgram, OpKind, Operand, ProtoId,
    Reg, RegMap,
};
use luau_hir::{BinOp, UnOp};
use luau_mir::{
    BlockId, Constant, Instr as MInstr, MirFunction, MirProgram, Terminator, Value as MValue,
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

    // Map MIR BlockId → BlockLabel. We use the MIR block id's numeric value as
    // the label id for simplicity.
    let label_of = |b: BlockId| BlockLabel(b.0);

    // Lay out blocks in MIR-order so falling through is sometimes possible.
    let n_blocks = f.blocks.len();
    for (idx, block) in f.blocks.iter().enumerate() {
        // Record this block's label position at the current instruction count.
        label_positions.push((label_of(block.id), instrs.len() as u32));

        for instr in &block.instrs {
            lower_instr(instr, &rm, &mut instrs);
        }

        let next_block = f.blocks.get(idx + 1).map(|b| b.id);
        lower_terminator(&block.terminator, &rm, &mut instrs, next_block, label_of);
    }

    Ok(LirFunction {
        id: ProtoId(f.id.0 as u16),
        num_params: f.params.len() as u16,
        num_regs: rm.num_regs(),
        consts: f.consts.clone(),
        instrs,
        label_positions,
    })
}

fn val_to_reg(v: MValue, rm: &RegMap, instrs: &mut Vec<LirInstr>) -> Reg {
    match v {
        MValue::VLocal(l) => Reg(rm.get(l)),
        MValue::Const(_) => {
            // Plan 1 always materializes constants via LoadConst into a VLocal at MIR time,
            // so Value::Const should not be reachable here. If it ever shows up, that's a bug.
            panic!("MIR Value::Const reached LIR lowering — should have been LoadConst'd");
        }
    }
}

fn lower_instr(instr: &MInstr, rm: &RegMap, out: &mut Vec<LirInstr>) {
    match instr {
        MInstr::LoadConst { dst, src } => {
            // Plan 1: always emit LoadConst. Plan 3 specializes Nil/True/False
            // into dedicated opcodes once it shrinks constant pools as part of
            // randomization.
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
            let lhs_r = val_to_reg(*lhs, rm, out);
            let rhs_r = val_to_reg(*rhs, rm, out);
            let dst_r = Reg(rm.get(*dst));
            let (a, b) = if swap { (rhs_r, lhs_r) } else { (lhs_r, rhs_r) };
            out.push(LirInstr {
                op: kind,
                operands: vec![Operand::Reg(dst_r), Operand::Reg(a), Operand::Reg(b)],
            });
            if invert {
                // For Ne/Gt/Ge we emit base op then Not in place.
                out.push(LirInstr {
                    op: OpKind::Not,
                    operands: vec![Operand::Reg(dst_r), Operand::Reg(dst_r)],
                });
            }
        }
        MInstr::UnOp { dst, op, operand } => {
            let r = val_to_reg(*operand, rm, out);
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
            let r = val_to_reg(*value, rm, out);
            out.push(LirInstr {
                op: OpKind::SetGlobal,
                operands: vec![Operand::Const(ConstIdx(name.0 as u16)), Operand::Reg(r)],
            });
        }
        MInstr::Call { dst, callee, args } => {
            let callee_r = val_to_reg(*callee, rm, out);
            // Call convention: VM expects args in registers immediately following callee.
            // We move them into place. Plan 2 swaps this for smarter packing.
            // Allocate temp registers at the high end of the register file.
            // Plan 1 simplification: assume callee_r+1..=callee_r+n are scratch.
            let n = args.len() as u16;
            for (i, a) in args.iter().enumerate() {
                let src = val_to_reg(*a, rm, out);
                let target = Reg(callee_r.0 + 1 + i as u16);
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
                    Operand::Reg(callee_r),
                    Operand::SmallInt(n as i16),
                ],
            });
        }
        MInstr::MakeClosure { dst, function } => {
            out.push(LirInstr {
                op: OpKind::Closure,
                operands: vec![
                    Operand::Reg(Reg(rm.get(*dst))),
                    Operand::Proto(ProtoId(function.0 as u16)),
                ],
            });
        }
    }
}

fn lower_binop(op: BinOp) -> (OpKind, bool, bool) {
    // (opcode, swap operands, invert result with Not)
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
            // Should have been lowered to control flow in MIR.
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
            // else fall through naturally.
        }
        Terminator::Branch { cond, then_block, else_block } => {
            let r = match cond {
                MValue::VLocal(l) => Reg(rm.get(*l)),
                MValue::Const(_) => panic!("branch on Const — should be LoadConst'd"),
            };
            // Strategy: JmpIfFalse to else_block, then fall through / Jmp to then_block.
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
```

- [ ] **Step 2: Add tests in `crates/luau-lir/src/lower.rs`**

Append at the bottom:

```rust
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
        // Should end with a Return.
        assert!(matches!(f.instrs.last(), Some(LirInstr { op: OpKind::Return, .. })));
    }

    #[test]
    fn local_add_global_print_lowers() {
        let p = lir_of("local x = 1 + 2 print(x)");
        let f = &p.functions[0];
        // Should contain at least: LoadConst LoadConst Add GetGlobal Move Call Return.
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
        // Main proto should contain a Closure op (binding f) and SetGlobal.
        let main = &p.functions[0];
        let ops: Vec<OpKind> = main.instrs.iter().map(|i| i.op).collect();
        assert!(ops.contains(&OpKind::Closure));
        assert!(ops.contains(&OpKind::SetGlobal));
        // f's proto should end in Return.
        assert!(p.functions.len() == 2);
        assert!(matches!(p.functions[1].instrs.last(), Some(LirInstr { op: OpKind::Return, .. })));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p luau-lir`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-lir
git commit -m "feat(lir): lower MIR to linear bytecode with symbolic labels"
```

---

## Task 12: luau-emit opcode mapping and bytecode encoding

**Files:**
- Create: `crates/luau-emit/Cargo.toml`
- Create: `crates/luau-emit/src/lib.rs`
- Create: `crates/luau-emit/src/opmap.rs`
- Create: `crates/luau-emit/src/encode.rs`

- [ ] **Step 1: Create `crates/luau-emit/Cargo.toml`**

```toml
[package]
name = "luau-emit"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-lir = { path = "../luau-lir" }
luau-runtime = { path = "../luau-runtime" }
luau-mir = { path = "../luau-mir" }
minijinja = { workspace = true }
rand = { workspace = true }
rand_chacha = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-emit/src/opmap.rs`**

```rust
//! Maps symbolic OpKinds to integer opcode bytes.
//!
//! In Plan 1 the mapping is deterministic and seed-independent. Plan 3 introduces
//! per-build randomization: keep the API stable so we just swap implementations.

use luau_lir::OpKind;

pub struct OpMap {
    /// table[OpKind index] = opcode byte
    forward: [u8; 24],
}

impl OpMap {
    /// Build a fresh map. `_seed` is the entropy source for Plan 3; ignored here.
    pub fn new(_seed: &[u8; 32]) -> Self {
        // Stable order in Plan 1. Plan 3 shuffles this.
        let mut forward = [0u8; 24];
        for (i, _k) in ALL_OPS.iter().enumerate() {
            forward[i] = (i + 1) as u8;
        }
        OpMap { forward }
    }

    pub fn opcode_of(&self, k: OpKind) -> u8 {
        let idx = op_index(k);
        self.forward[idx]
    }

    /// All used opcode bytes in their assignment order (for the VM dispatch table).
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
    OpKind::Jmp, OpKind::JmpIfTrue,
    // 24 entries — JmpIfFalse and Closure beyond u8 boundary? No, we have 26 ops total.
];
```

Wait — count check: there are 26 OpKinds, not 24. Fix the array size and ALL_OPS list:

Replace the `OpMap` definition with:

```rust
pub struct OpMap {
    forward: [u8; 26],
}

impl OpMap {
    pub fn new(_seed: &[u8; 32]) -> Self {
        let mut forward = [0u8; 26];
        for i in 0..26 {
            forward[i] = (i + 1) as u8;
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
];

fn op_index(k: OpKind) -> usize {
    ALL_OPS.iter().position(|x| *x == k).expect("known op")
}
```

- [ ] **Step 3: Create `crates/luau-emit/src/encode.rs`**

Encodes a `LirFunction` into a byte string that the VM will decode. Plan 1 uses the simplest possible encoding (no XOR roll, no scrambling — that comes in Plan 3): one opcode byte, then each operand serialized inline.

Operand encoding:
- Reg: 2 bytes little-endian (u16). 0xFFFF reserved as "none".
- Const: 2 bytes little-endian.
- Proto: 2 bytes little-endian.
- SmallInt: 2 bytes little-endian (i16 cast to u16).
- JmpTarget: 2 bytes little-endian *signed* relative offset from the byte immediately after the operand. Resolved during encoding using the label position table.

```rust
use crate::opmap::OpMap;
use luau_lir::{BlockLabel, LirFunction, LirInstr, Operand};
use std::collections::HashMap;

/// Encode one function. Returns (bytecode bytes, instruction byte offsets).
/// The latter is used for resolving JmpTarget operands.
pub fn encode_function(f: &LirFunction, opmap: &OpMap) -> Vec<u8> {
    // Two-pass: first compute instruction byte offsets, then encode with jump
    // targets resolved.
    let mut instr_byte_offsets: Vec<u32> = Vec::with_capacity(f.instrs.len() + 1);
    let mut offset: u32 = 0;
    for instr in &f.instrs {
        instr_byte_offsets.push(offset);
        offset += instr_size(instr);
    }
    instr_byte_offsets.push(offset);

    // label_byte_offsets[label] = byte offset of the instruction at that label.
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
        // Position of the byte *after* this instruction.
        let after_this = instr_byte_offsets[i] + instr_size(instr);
        for operand in &instr.operands {
            match operand {
                Operand::Reg(r) => push_u16(&mut out, r.0),
                Operand::Const(c) => push_u16(&mut out, c.0),
                Operand::Proto(p) => push_u16(&mut out, p.0),
                Operand::SmallInt(n) => push_u16(&mut out, *n as u16),
                Operand::JmpTarget(label) => {
                    let target_byte = *label_byte.get(label).expect("label");
                    let delta = target_byte as i32 - after_this as i32;
                    push_u16(&mut out, delta as i16 as u16);
                }
            }
        }
    }
    out
}

fn instr_size(i: &LirInstr) -> u32 {
    // 1 byte opcode + 2 bytes per operand.
    1 + (i.operands.len() as u32) * 2
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.push((v & 0xFF) as u8);
    out.push(((v >> 8) & 0xFF) as u8);
}
```

- [ ] **Step 4: Create stub `crates/luau-emit/src/lib.rs`**

```rust
//! Emit stage. Encodes LIR to bytecode and renders the VM template.

pub mod opmap;
pub mod encode;
pub mod render;

use luau_lir::LirProgram;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("template error: {0}")]
    Template(String),
}

/// Top-level emit. Produces the final Luau chunk source.
pub fn emit(program: &LirProgram, seed: [u8; 32]) -> Result<String, EmitError> {
    let opmap = opmap::OpMap::new(&seed);
    let mut rng = ChaCha20Rng::from_seed(seed);
    render::render(program, &opmap, &mut rng)
}
```

- [ ] **Step 5: Add a basic encoder test in `crates/luau-emit/src/encode.rs`**

Append:

```rust
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
            consts: vec![],
            instrs: vec![LirInstr { op: OpKind::Return, operands: vec![Operand::Reg(Reg(0xFFFF))] }],
            label_positions: vec![],
        };
        let opmap = OpMap::new(&[0u8; 32]);
        let bytes = encode_function(&f, &opmap);
        assert_eq!(bytes.len(), 3); // opcode + 2-byte operand
        assert_eq!(bytes[0], opmap.opcode_of(OpKind::Return));
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0xFF);
    }
}
```

- [ ] **Step 6: Stub `crates/luau-emit/src/render.rs`** so the lib compiles

```rust
//! Renders the VM template to produce the final Luau chunk. Implemented in Task 14.

use crate::opmap::OpMap;
use crate::EmitError;
use luau_lir::LirProgram;
use rand_chacha::ChaCha20Rng;

pub fn render(
    _program: &LirProgram,
    _opmap: &OpMap,
    _rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    todo!("Task 14 implements template rendering")
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p luau-emit --lib encode::tests`
Expected: passes.

- [ ] **Step 8: Commit**

```bash
git add crates/luau-emit
git commit -m "feat(emit): opcode map and bytecode encoder"
```

---

## Task 13: luau-runtime VM template (Luau source)

**Files:**
- Create: `crates/luau-runtime/Cargo.toml`
- Create: `crates/luau-runtime/src/lib.rs`
- Create: `crates/luau-runtime/assets/vm.luau.j2`

The VM is templated Luau. Minijinja substitutes opcode constants, the bytecode literal, the constant pool, and the proto table. The VM is a `while true / if-elseif` interpreter over the byte stream.

- [ ] **Step 1: Create `crates/luau-runtime/Cargo.toml`**

```toml
[package]
name = "luau-runtime"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
```

- [ ] **Step 2: Create `crates/luau-runtime/src/lib.rs`**

```rust
//! Exposes the VM source template as a compile-time constant string so emit
//! doesn't have to do any filesystem I/O.

pub const VM_TEMPLATE: &str = include_str!("../assets/vm.luau.j2");
```

- [ ] **Step 3: Create `crates/luau-runtime/assets/vm.luau.j2`**

This is the Luau source of the VM with minijinja placeholders. Note: Luau uses `--` for comments, so we never need to worry about line conflicts with jinja syntax (we use `{{ }}` and `{% %}` which don't clash).

```jinja
-- luau-obf runtime
-- Generated; do not edit by hand.

local _ENV_ = _ENV or getfenv(1)
local _select = select
local _error = error
local _pcall = pcall
local _tostring = tostring
local _type = type
local _sbyte = string.byte
local _ssub = string.sub
local _schar = string.char
local _tunpack = table.unpack or unpack
local _tcreate = function(n) local t = {} for i=1,n do t[i] = false end return t end
local _floor = math.floor

-- Opcode constants
{% for opname, num in opcodes %}local OP_{{ opname }} = {{ num }}
{% endfor %}

-- Constant pools per proto.
local CONSTS = {
{% for proto_consts in consts %}    { {{ proto_consts }} },
{% endfor %}}

-- Bytecode strings per proto.
local CODE = {
{% for code_lit in codes %}    {{ code_lit }},
{% endfor %}}

-- Proto metadata: { num_params, num_regs }
local META = {
{% for m in meta %}    { {{ m.num_params }}, {{ m.num_regs }} },
{% endfor %}}

-- Read a little-endian u16 at byte offset `pos` (1-based) of `code`.
local function read_u16(code, pos)
    local lo = _sbyte(code, pos)
    local hi = _sbyte(code, pos + 1)
    return lo + hi * 256
end

-- Read signed i16 at 1-based offset.
local function read_i16(code, pos)
    local v = read_u16(code, pos)
    if v >= 32768 then v = v - 65536 end
    return v
end

local vm_call
vm_call = function(proto_id, args, nargs)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local meta = META[proto_id + 1]
    local num_params = meta[1]
    local num_regs = meta[2]
    local regs = _tcreate(num_regs + 1)
    -- Copy params into r0..r{num_params-1}.
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local pc = 1
    local code_len = #code
    while pc <= code_len do
        local op = _sbyte(code, pc)
        pc = pc + 1
        if op == OP_LoadNil then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = nil
        elseif op == OP_LoadTrue then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = true
        elseif op == OP_LoadFalse then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = false
        elseif op == OP_LoadConst then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = consts[b + 1]
        elseif op == OP_Move then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b]
        elseif op == OP_Add then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] + regs[c]
        elseif op == OP_Sub then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] - regs[c]
        elseif op == OP_Mul then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] * regs[c]
        elseif op == OP_Div then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] / regs[c]
        elseif op == OP_Mod then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] % regs[c]
        elseif op == OP_Pow then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] ^ regs[c]
        elseif op == OP_Concat then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = _tostring(regs[b]) .. _tostring(regs[c])
        elseif op == OP_Lt then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] < regs[c]
        elseif op == OP_Le then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] <= regs[c]
        elseif op == OP_Eq then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] == regs[c]
        elseif op == OP_Not then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = not regs[b]
        elseif op == OP_Neg then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = -regs[b]
        elseif op == OP_Len then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = #regs[b]
        elseif op == OP_GetGlobal then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = _ENV_[consts[b + 1]]
        elseif op == OP_SetGlobal then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            _ENV_[consts[a + 1]] = regs[b]
        elseif op == OP_Call then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local n = read_i16(code, pc); pc = pc + 2
            local fn = regs[b]
            -- Collect args
            local call_args = {}
            for i = 1, n do
                call_args[i] = regs[b + i]
            end
            local result = fn(_tunpack(call_args, 1, n))
            if a ~= 0xFFFF then
                regs[a] = result
            end
        elseif op == OP_Return then
            local a = read_u16(code, pc); pc = pc + 2
            if a == 0xFFFF then
                return nil
            else
                return regs[a]
            end
        elseif op == OP_Jmp then
            local delta = read_i16(code, pc); pc = pc + 2
            pc = pc + delta
        elseif op == OP_JmpIfTrue then
            local a = read_u16(code, pc); pc = pc + 2
            local delta = read_i16(code, pc); pc = pc + 2
            if regs[a] then
                pc = pc + delta
            end
        elseif op == OP_JmpIfFalse then
            local a = read_u16(code, pc); pc = pc + 2
            local delta = read_i16(code, pc); pc = pc + 2
            if not regs[a] then
                pc = pc + delta
            end
        elseif op == OP_Closure then
            local a = read_u16(code, pc); pc = pc + 2
            local pid = read_u16(code, pc); pc = pc + 2
            regs[a] = function(...)
                local pa = { ... }
                local n = _select("#", ...)
                return vm_call(pid, pa, n)
            end
        else
            _error("luau-obf: unknown opcode " .. _tostring(op))
        end
    end
    return nil
end

-- Bootstrap: main proto is index 0; forward varargs.
local main_args = { ... }
local main_n = _select("#", ...)
return vm_call(0, main_args, main_n)
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p luau-runtime`
Expected: compiles. (The template is included as a string — it's not validated as Luau here; that happens at differential-test time.)

- [ ] **Step 5: Commit**

```bash
git add crates/luau-runtime
git commit -m "feat(runtime): VM template for the custom ISA"
```

---

## Task 14: luau-emit template rendering

**Files:**
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Replace `crates/luau-emit/src/render.rs`**

```rust
//! Renders the VM template against the LIR + opcode map to produce a single
//! Luau chunk.

use crate::encode::encode_function;
use crate::opmap::{OpMap, ALL_OPS};
use crate::EmitError;
use luau_lir::{LirProgram, OpKind};
use luau_mir::Constant;
use minijinja::{Environment, Value};
use rand_chacha::ChaCha20Rng;
use serde::Serialize;

pub fn render(
    program: &LirProgram,
    opmap: &OpMap,
    _rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    let opcodes: Vec<(String, u8)> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();

    let consts: Vec<String> = program
        .functions
        .iter()
        .map(|f| format_const_pool(&f.consts))
        .collect();

    let codes: Vec<String> = program
        .functions
        .iter()
        .map(|f| {
            let bytes = encode_function(f, opmap);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();

    let meta: Vec<MetaEntry> = program
        .functions
        .iter()
        .map(|f| MetaEntry {
            num_params: f.num_params,
            num_regs: std::cmp::max(f.num_regs, f.num_params),
        })
        .collect();

    let mut env = Environment::new();
    env.add_template("vm", luau_runtime::VM_TEMPLATE)
        .map_err(|e| EmitError::Template(e.to_string()))?;
    let tmpl = env.get_template("vm").unwrap();
    tmpl.render(minijinja::context! {
        opcodes => opcodes,
        consts => consts,
        codes => codes,
        meta => meta,
    })
    .map_err(|e| EmitError::Template(e.to_string()))
}

#[derive(Serialize)]
struct MetaEntry {
    num_params: u16,
    num_regs: u16,
}

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
    }
}

fn format_const_pool(consts: &[Constant]) -> String {
    let parts: Vec<String> = consts.iter().map(format_const).collect();
    parts.join(", ")
}

fn format_const(c: &Constant) -> String {
    match c {
        Constant::Nil => "nil".into(),
        Constant::Bool(true) => "true".into(),
        Constant::Bool(false) => "false".into(),
        Constant::Number(n) => {
            // Use Rust's Display, which round-trips for finite f64 in most cases.
            // For NaN/Inf emit Luau-friendly forms.
            if n.is_nan() { "(0/0)".into() }
            else if n.is_infinite() && *n > 0.0 { "(1/0)".into() }
            else if n.is_infinite() { "(-1/0)".into() }
            else { format!("{}", n) }
        }
        Constant::String(s) => format!("\"{}\"", escape_luau_string(s)),
    }
}

fn escape_luau_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\{}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Encode bytes as the body of a Luau "..." literal. Every byte that isn't
/// printable ASCII is escaped with \ddd decimal.
fn encode_luau_string_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\{}", b)),
        }
    }
    out
}
```

- [ ] **Step 2: Add an end-to-end emit test in `crates/luau-emit/src/lib.rs`**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use luau_hir::lower::lower as hir_lower;
    use luau_mir::lower::lower as mir_lower;
    use luau_lir::lower::lower as lir_lower;

    fn compile_to_luau(src: &str) -> String {
        let ast = luau_parse::parse(src).unwrap();
        let hir = hir_lower(&ast).unwrap();
        let mir = mir_lower(&hir).unwrap();
        let lir = lir_lower(&mir).unwrap();
        emit(&lir, [0u8; 32]).unwrap()
    }

    #[test]
    fn empty_source_emits_a_valid_chunk() {
        let chunk = compile_to_luau("");
        // Should contain the basic VM signatures
        assert!(chunk.contains("vm_call"));
        assert!(chunk.contains("OP_Return"));
    }

    #[test]
    fn print_one_emits_call_and_get_global() {
        let chunk = compile_to_luau("print(1)");
        assert!(chunk.contains("OP_Call"));
        assert!(chunk.contains("OP_GetGlobal"));
    }
}
```

Add dev-deps to `crates/luau-emit/Cargo.toml`:

```toml
[dev-dependencies]
luau-hir = { path = "../luau-hir" }
luau-parse = { path = "../luau-parse" }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p luau-emit`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit
git commit -m "feat(emit): render VM template with bytecode + constant pool"
```

---

## Task 15: luau-obf facade

**Files:**
- Create: `crates/luau-obf/Cargo.toml`
- Create: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Create `crates/luau-obf/Cargo.toml`**

```toml
[package]
name = "luau-obf"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
luau-parse = { path = "../luau-parse" }
luau-hir = { path = "../luau-hir" }
luau-mir = { path = "../luau-mir" }
luau-passes = { path = "../luau-passes" }
luau-lir = { path = "../luau-lir" }
luau-emit = { path = "../luau-emit" }
rand = { workspace = true }
rand_chacha = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-obf/src/lib.rs`**

```rust
//! Top-level facade. One function: obfuscate.

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Options {
    /// 32-byte seed. If None, a random seed is generated and surfaced via `seed_used`.
    pub seed: Option<[u8; 32]>,
}

impl Default for Options {
    fn default() -> Self {
        Self { seed: None }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse: {0}")]
    Parse(#[from] luau_parse::ParseError),
    #[error("hir: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("mir: {0}")]
    Mir(#[from] luau_mir::MirError),
    #[error("lir: {0}")]
    Lir(#[from] luau_lir::LirError),
    #[error("emit: {0}")]
    Emit(#[from] luau_emit::EmitError),
}

pub struct ObfuscateResult {
    /// The obfuscated Luau chunk.
    pub output: String,
    /// The seed actually used (whether user-provided or generated).
    pub seed_used: [u8; 32],
}

pub fn obfuscate(source: &str, opts: Options) -> Result<ObfuscateResult, Error> {
    let seed = opts.seed.unwrap_or_else(random_seed);
    let mut rng = ChaCha20Rng::from_seed(seed);

    let ast = luau_parse::parse(source)?;
    let hir = luau_hir::lower::lower(&ast)?;
    let mut mir = luau_mir::lower::lower(&hir)?;
    let plan = luau_passes::default_plan();
    plan.run(&mut mir, &mut rng);
    let lir = luau_lir::lower::lower(&mir)?;
    let output = luau_emit::emit(&lir, seed)?;
    Ok(ObfuscateResult { output, seed_used: seed })
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    seed
}
```

- [ ] **Step 3: Add a smoke test in `crates/luau-obf/src/lib.rs`**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscate_empty() {
        let r = obfuscate("", Options::default()).unwrap();
        assert!(r.output.contains("vm_call"));
        assert_eq!(r.seed_used.len(), 32);
    }

    #[test]
    fn obfuscate_simple_print() {
        let r = obfuscate("print(1)", Options { seed: Some([1u8; 32]) }).unwrap();
        assert_eq!(r.seed_used, [1u8; 32]);
        assert!(r.output.contains("OP_GetGlobal"));
    }

    #[test]
    fn deterministic_with_fixed_seed() {
        let a = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        let b = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        assert_eq!(a.output, b.output);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p luau-obf`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-obf
git commit -m "feat(obf): facade crate with obfuscate(source, opts)"
```

---

## Task 16: luau-obf-cli

**Files:**
- Create: `crates/luau-obf-cli/Cargo.toml`
- Create: `crates/luau-obf-cli/src/main.rs`

- [ ] **Step 1: Create `crates/luau-obf-cli/Cargo.toml`**

```toml
[package]
name = "luau-obf-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "luau-obf"
path = "src/main.rs"

[dependencies]
luau-obf = { path = "../luau-obf" }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
tempfile = { workspace = true }
```

- [ ] **Step 2: Create `crates/luau-obf-cli/src/main.rs`**

```rust
use clap::Parser;
use luau_obf::{obfuscate, Options};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "luau-obf", version, about = "Luau obfuscator")]
struct Args {
    /// Input .luau file
    input: PathBuf,
    /// Output file
    #[arg(short = 'o', long = "output")]
    output: PathBuf,
    /// 64-hex-character seed (32 bytes). If omitted, a random seed is used.
    #[arg(long = "seed")]
    seed: Option<String>,
    /// Suppress progress on stderr.
    #[arg(long = "quiet")]
    quiet: bool,
}

fn parse_seed(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("seed must be 64 hex chars, got {}", s.len()));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("invalid hex at position {}", i * 2))?;
    }
    Ok(out)
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn main() -> ExitCode {
    let args = Args::parse();
    let source = match fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{{\"error\":\"read input\",\"message\":\"{}\"}}", e);
            return ExitCode::from(1);
        }
    };
    let seed = match args.seed.as_deref().map(parse_seed) {
        None => None,
        Some(Ok(s)) => Some(s),
        Some(Err(msg)) => {
            eprintln!("{{\"error\":\"seed\",\"message\":\"{}\"}}", msg);
            return ExitCode::from(1);
        }
    };
    let opts = Options { seed };
    match obfuscate(&source, opts) {
        Ok(result) => {
            if !args.quiet {
                eprintln!("seed={}", hex_of(&result.seed_used));
            }
            if let Err(e) = fs::write(&args.output, &result.output) {
                eprintln!("{{\"error\":\"write output\",\"message\":\"{}\"}}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Parse/HIR errors are user errors → exit 1; the rest are internal → exit 2.
            let exit_code = match &e {
                luau_obf::Error::Parse(_) | luau_obf::Error::Hir(_) => 1,
                _ => 2,
            };
            eprintln!("{{\"error\":\"obfuscate\",\"message\":\"{}\"}}", e);
            ExitCode::from(exit_code)
        }
    }
}
```

- [ ] **Step 3: Add a CLI integration test in `crates/luau-obf-cli/tests/cli.rs`**

Create the file:

```rust
use assert_cmd::Command;
use std::fs;

#[test]
fn cli_obfuscates_simple_program() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let output = dir.path().join("out.luau");
    fs::write(&input, "print(1)").unwrap();

    Command::cargo_bin("luau-obf")
        .unwrap()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--quiet")
        .assert()
        .success();

    let out = fs::read_to_string(&output).unwrap();
    assert!(out.contains("vm_call"));
}

#[test]
fn cli_exits_1_on_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let output = dir.path().join("out.luau");
    fs::write(&input, "this is not luau {{{").unwrap();

    Command::cargo_bin("luau-obf")
        .unwrap()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--quiet")
        .assert()
        .code(1);
}

#[test]
fn cli_accepts_explicit_seed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let out_a = dir.path().join("a.luau");
    let out_b = dir.path().join("b.luau");
    fs::write(&input, "print(1)").unwrap();

    let seed = "0011223344556677889900112233445566778899001122334455667788990011";

    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_a).arg("--seed").arg(seed).arg("--quiet")
        .assert().success();
    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_b).arg("--seed").arg(seed).arg("--quiet")
        .assert().success();

    let a = fs::read_to_string(&out_a).unwrap();
    let b = fs::read_to_string(&out_b).unwrap();
    assert_eq!(a, b, "same seed must produce identical output");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p luau-obf-cli`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/luau-obf-cli
git commit -m "feat(cli): luau-obf CLI binary with seed and JSON errors"
```

---

## Task 17: runtime-tests harness

**Files:**
- Create: `runtime-tests/Cargo.toml`
- Create: `runtime-tests/src/main.rs`

This is the differential test harness. For each `.luau` file under `runtime-tests/corpus/`, it:
1. Runs the program through the system Luau interpreter directly (`luau <file>`).
2. Obfuscates the program with a fixed seed.
3. Runs the obfuscated program through `luau`.
4. Asserts stdout is byte-identical and both exit successfully.

It also runs each corpus program twice with the same seed to assert determinism of the obfuscator output.

- [ ] **Step 1: Create `runtime-tests/Cargo.toml`**

```toml
[package]
name = "runtime-tests"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[[bin]]
name = "luau-obf-difftest"
path = "src/main.rs"

[dependencies]
luau-obf = { path = "../crates/luau-obf" }
```

- [ ] **Step 2: Create `runtime-tests/src/main.rs`**

```rust
//! Differential test harness.
//!
//! Walks `runtime-tests/corpus/*.luau`, obfuscates each, and compares execution
//! output to the plain Luau interpreter's output. Requires `luau` on PATH.

use luau_obf::{obfuscate, Options};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let corpus_dir = corpus_dir();
    let entries: Vec<PathBuf> = match fs::read_dir(&corpus_dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension() == Some(OsStr::new("luau")))
            .collect(),
        Err(e) => {
            eprintln!("could not read corpus dir {corpus_dir:?}: {e}");
            std::process::exit(2);
        }
    };
    if entries.is_empty() {
        eprintln!("no .luau files found in {corpus_dir:?}");
        std::process::exit(2);
    }
    let mut failed: Vec<String> = Vec::new();
    let mut passed = 0usize;
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match run_one(&path) {
            Ok(()) => {
                println!("PASS  {name}");
                passed += 1;
            }
            Err(msg) => {
                println!("FAIL  {name}\n      {msg}");
                failed.push(name);
            }
        }
    }
    println!();
    println!("{} passed, {} failed", passed, failed.len());
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

fn run_one(source_path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(source_path).map_err(|e| format!("read source: {e}"))?;

    // Plain Luau output (reference).
    let plain = run_luau_with_source(&src)?;

    // Obfuscated output (twice with same seed → determinism).
    let seed = [0xABu8; 32];
    let r1 = obfuscate(&src, Options { seed: Some(seed) })
        .map_err(|e| format!("obfuscate (run 1): {e}"))?;
    let r2 = obfuscate(&src, Options { seed: Some(seed) })
        .map_err(|e| format!("obfuscate (run 2): {e}"))?;
    if r1.output != r2.output {
        return Err("non-deterministic obfuscator output for fixed seed".into());
    }

    let obfuscated_output = run_luau_with_source(&r1.output)?;
    if plain != obfuscated_output {
        return Err(format!(
            "output mismatch:\n  plain: {plain:?}\n  obfus: {obfuscated_output:?}"
        ));
    }
    Ok(())
}

fn run_luau_with_source(source: &str) -> Result<String, String> {
    let dir = tempdir()?;
    let path = dir.join("run.luau");
    fs::write(&path, source).map_err(|e| format!("write tmp: {e}"))?;
    let out = Command::new("luau")
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("spawn luau: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("luau exited non-zero: {} stderr={stderr}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn corpus_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("corpus")
}

// Inlined tempdir to avoid pulling `tempfile` into runtime-tests (only used here).
fn tempdir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = base.join(format!("luau-obf-difftest-{pid}-{nanos}"));
    fs::create_dir_all(&path).map_err(|e| format!("mkdir tmp: {e}"))?;
    Ok(path)
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p runtime-tests`
Expected: compiles. (It will not run successfully yet — corpus is empty and we add it in Task 18.)

- [ ] **Step 4: Commit**

```bash
git add runtime-tests
git commit -m "feat(runtime-tests): differential execution harness"
```

---

## Task 18: Add test corpus

**Files:**
- Create: `runtime-tests/corpus/arithmetic.luau`
- Create: `runtime-tests/corpus/if_else.luau`
- Create: `runtime-tests/corpus/while_loop.luau`
- Create: `runtime-tests/corpus/numeric_for.luau`
- Create: `runtime-tests/corpus/recursion.luau`
- Create: `runtime-tests/corpus/string_ops.luau`

Each corpus file is a self-checking program: it computes and prints something concrete. The harness compares output byte-for-byte against plain Luau.

- [ ] **Step 1: Create `runtime-tests/corpus/arithmetic.luau`**

```lua
local a = 7 + 3
local b = 7 - 3
local c = 7 * 3
local d = 7 / 2
local e = 7 % 3
local f = 2 ^ 5
print(a, b, c, d, e, f)
print(-a)
print(not false)
print(1 == 1, 1 == 2, 1 < 2, 1 <= 1, 2 > 1, 2 >= 2, 1 ~= 2)
```

- [ ] **Step 2: Create `runtime-tests/corpus/if_else.luau`**

```lua
local function classify(n)
    if n < 0 then
        return "neg"
    elseif n == 0 then
        return "zero"
    else
        return "pos"
    end
end
print(classify(-1))
print(classify(0))
print(classify(1))
```

- [ ] **Step 3: Create `runtime-tests/corpus/while_loop.luau`**

```lua
local i = 0
local total = 0
while i < 5 do
    total = total + i
    i = i + 1
end
print(total)

-- repeat .. until
local j = 0
local product = 1
repeat
    j = j + 1
    product = product * j
until j >= 4
print(product)
```

- [ ] **Step 4: Create `runtime-tests/corpus/numeric_for.luau`**

```lua
local total = 0
for i = 1, 10 do
    total = total + i
end
print(total)

local stepped = 0
for i = 0, 20, 5 do
    stepped = stepped + i
end
print(stepped)
```

- [ ] **Step 5: Create `runtime-tests/corpus/recursion.luau`**

```lua
function factorial(n)
    if n <= 1 then return 1 end
    return n * factorial(n - 1)
end
print(factorial(6))

function fib(n)
    if n < 2 then return n end
    return fib(n - 1) + fib(n - 2)
end
print(fib(10))
```

- [ ] **Step 6: Create `runtime-tests/corpus/string_ops.luau`**

```lua
local greeting = "hello" .. " " .. "world"
print(greeting)
print(#greeting)
print("a" == "a", "a" == "b")
print(("x" .. "y") .. ("a" .. "b"))
```

- [ ] **Step 7: Check `luau` is installed**

Run: `luau --version`
Expected: prints a version. If not installed, install via `brew install luau` (macOS) or build from https://github.com/luau-lang/luau. The harness requires the `luau` CLI on PATH.

- [ ] **Step 8: Run the differential harness**

Run: `cargo run -p runtime-tests --release`
Expected: all 6 corpus programs report PASS.

If any FAIL: that's a real bug in the pipeline. Use `--quiet` off mode of the CLI plus seed printing to reproduce manually:

```bash
cargo run -p luau-obf-cli -- runtime-tests/corpus/<failing>.luau -o /tmp/out.luau
luau /tmp/out.luau
diff <(luau runtime-tests/corpus/<failing>.luau) <(luau /tmp/out.luau)
```

Fix the bug in whichever pipeline stage owns the failure, then re-run.

- [ ] **Step 9: Commit**

```bash
git add runtime-tests/corpus
git commit -m "test(runtime-tests): corpus exercising Plan 1 language subset"
```

---

## Task 19: Wire `cargo test --workspace` and CI smoke

**Files:**
- Create: `runtime-tests/tests/difftest.rs`

Wire the runtime-tests binary into `cargo test --workspace` so the differential harness is part of the standard test command, not a separate `cargo run`.

- [ ] **Step 1: Create `runtime-tests/tests/difftest.rs`**

```rust
//! Integration test that invokes the differential harness binary.
//! Requires `luau` on PATH; if not present, the test is skipped.

use std::process::Command;

#[test]
fn run_differential_harness() {
    if which("luau").is_err() {
        eprintln!("skipping: `luau` not on PATH");
        return;
    }
    let status = Command::new(env!("CARGO_BIN_EXE_luau-obf-difftest"))
        .status()
        .expect("spawn harness");
    assert!(status.success(), "differential harness failed");
}

fn which(prog: &str) -> Result<(), ()> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let p = std::path::Path::new(dir).join(prog);
            if p.exists() {
                return Ok(());
            }
        }
    }
    Err(())
}
```

- [ ] **Step 2: Verify it runs via `cargo test`**

Run: `cargo test -p runtime-tests --release run_differential_harness`
Expected: passes if `luau` is on PATH, skipped otherwise.

- [ ] **Step 3: Run the whole workspace**

Run: `cargo test --workspace`
Expected: all unit/integration tests pass.

- [ ] **Step 4: Commit**

```bash
git add runtime-tests/tests
git commit -m "test: hook differential harness into cargo test"
```

---

## Task 20: End-to-end verification + README pointer

**Files:**
- Modify (or create): `README.md`

- [ ] **Step 1: Smoke-test the CLI on a fresh program**

```bash
cat > /tmp/demo.luau <<'EOF'
function greet(name)
    return "hi, " .. name
end
print(greet("plan-1"))
EOF
cargo run -p luau-obf-cli -- /tmp/demo.luau -o /tmp/demo.out.luau
luau /tmp/demo.out.luau
```

Expected output: `hi, plan-1`

- [ ] **Step 2: Verify same input + same seed produces byte-identical output**

```bash
SEED="0011223344556677889900112233445566778899001122334455667788990011"
cargo run -p luau-obf-cli -- /tmp/demo.luau -o /tmp/demo.a.luau --seed $SEED --quiet
cargo run -p luau-obf-cli -- /tmp/demo.luau -o /tmp/demo.b.luau --seed $SEED --quiet
diff /tmp/demo.a.luau /tmp/demo.b.luau
```

Expected: no diff (exit 0).

- [ ] **Step 3: Verify `luau-obf <bad-source>` exits 1**

```bash
echo "this is not luau {{{" > /tmp/bad.luau
cargo run -p luau-obf-cli -- /tmp/bad.luau -o /tmp/out.luau --quiet
echo "exit=$?"
```

Expected: `exit=1`.

- [ ] **Step 4: Create `README.md`**

```markdown
# luau-obf

Rust-implemented Luau obfuscator. VM-based, loadstring-style protection
intended for Roblox scripts.

**Status:** Plan 1 — skeleton & identity pipeline. The obfuscator round-trips
a meaningful Luau subset through a custom VM with no obfuscation passes yet.

## Build

```
cargo build --workspace --release
```

## Use

```
cargo run -p luau-obf-cli --release -- input.luau -o output.luau
```

## Test

Differential execution tests require the `luau` CLI on PATH.

```
cargo test --workspace --release
```

## Architecture

See `docs/superpowers/specs/2026-06-01-luau-obfuscator-design.md`.
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: add README describing Plan 1 status"
```

- [ ] **Step 6: Final clippy sweep**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. If any, fix them (most likely candidates: unused imports, `&Vec<T>` instead of `&[T]`, format-string lints).

- [ ] **Step 7: Final commit**

If clippy fixed anything:

```bash
git add -u
git commit -m "chore: clippy cleanups"
```

---

## Self-review

After all tasks complete the workspace should:

- Build cleanly with `cargo build --workspace --release`.
- Pass `cargo test --workspace --release`.
- Pass `cargo clippy --workspace --all-targets -- -D warnings`.
- Roundtrip the 6 corpus programs through the obfuscator with byte-identical
  stdout vs. plain Luau.
- Produce byte-identical output for the same `(input, seed)` pair.

**Plan 2 entry points** (no work required in Plan 1, just confirming the seams are in place):

- New passes go in `crates/luau-passes/src/` and register in `default_plan()`.
- New language features extend `luau-hir` (parser already handles them via full_moon) → `luau-mir` (desugaring) → possibly new `OpKind`s.
- Per-build opcode randomization swaps `OpMap::new`'s implementation.
- Constant encryption swaps `encode_function` to take an encryption strategy.

These seams already exist in Plan 1 and don't need changes — Plan 2's tasks add new behavior at those points.
