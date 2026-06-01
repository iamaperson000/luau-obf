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
        E::TypeAssertion { expression, .. } => lower_expr(lowerer, expression),
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

#[cfg(test)]
mod tests {
    use super::*;

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
