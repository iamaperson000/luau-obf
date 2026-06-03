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

#[derive(Debug, Clone)]
pub enum TableEntry {
    /// `value` — positional, integer-keyed at the next slot (1-based).
    Array(HirExpr),
    /// `name = value` — string-keyed shorthand.
    Field(String, HirExpr),
    /// `[expr] = value`.
    Keyed(HirExpr, HirExpr),
}

/// How a function's upvalue is sourced from its immediately-enclosing frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpvalueSource {
    /// The parent function's local (by SymbolId).
    ParentLocal(SymbolId),
    /// The parent function's upvalue (by index into its upvalues vec).
    ParentUpval(u32),
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
    /// The associated `HirFunction.upvalues` list records any locals captured
    /// from enclosing function frames.
    Function(HirFunction),
    /// Read this function's upvalue at the given index.
    Upvalue(u32),
    /// `...` — the current function's varargs as a multi-value expression.
    /// In most positions only the first value is used; in tail position of a
    /// call/return/decl/assign expression list, all values spread.
    Vararg,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    /// `local x = expr` (or `local x` with `expr = Literal(Nil)`).
    LocalDecl { symbol: SymbolId, value: HirExpr },
    /// `name = expr` where `name` is a resolved symbol.
    Assign { target: SymbolId, value: HirExpr },
    /// Write this function's upvalue at the given index.
    UpvalueAssign { upvalue: u32, value: HirExpr },
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
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub params: Vec<SymbolId>,
    pub body: Vec<HirStmt>,
    /// Upvalue sources, in declaration order. The index in this vec is the
    /// upvalue identifier used by `HirExpr::Upvalue` and `HirStmt::UpvalueAssign`
    /// inside this function's body.
    pub upvalues: Vec<UpvalueSource>,
    /// True if the function declared `...` in its parameter list.
    /// Inside the body, `HirExpr::Vararg` is only valid when this is true.
    pub is_vararg: bool,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    /// All symbols declared anywhere in the program, keyed by SymbolId.
    /// `symbols[s.id.0 as usize]` is the Symbol for a given id (when present).
    pub symbols: Vec<Symbol>,
    /// Top-level statements (the main chunk).
    pub main: Vec<HirStmt>,
}
