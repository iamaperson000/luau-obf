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
