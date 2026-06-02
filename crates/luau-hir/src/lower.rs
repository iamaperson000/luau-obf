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
            let e = lower_call(lowerer, call)?;
            Ok(vec![HirStmt::ExprStmt(e)])
        }
        Stmt::If(if_stmt) => {
            let cond = lower_expr(lowerer, if_stmt.condition())?;
            lowerer.push_scope();
            let then_body = lower_block(lowerer, if_stmt.block())?;
            lowerer.pop_scope();
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

fn parse_luau_number(raw: &str) -> Option<f64> {
    // Strip underscore separators (valid in Luau numeric literals).
    let s_owned: String = raw.chars().filter(|c| *c != '_').collect();
    let s = s_owned.as_str();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok().map(|v| v as f64);
    }
    if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        return u64::from_str_radix(bin, 2).ok().map(|v| v as f64);
    }
    s.parse::<f64>().ok()
}

fn lower_expr(
    lowerer: &mut Lowerer,
    expr: &full_moon::ast::Expression,
) -> Result<HirExpr, HirError> {
    use full_moon::ast::Expression as E;
    match expr {
        E::Number(token) => {
            let raw = token.token().to_string();
            let n = parse_luau_number(&raw)
                .ok_or_else(|| HirError::Unsupported(format!("could not parse numeric literal {raw:?}")))?;
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
    use full_moon::ast::{Call, Prefix, Suffix};
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

    fn lower_str(src: &str) -> HirProgram {
        let ast = luau_parse::parse(src).expect("parse");
        lower(&ast).expect("lower")
    }

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

    #[test]
    fn hex_literal() {
        let e = lower_one_expr("0xff");
        let HirExpr::Literal(HirLiteral::Number(n)) = e else { panic!() };
        assert_eq!(n, 255.0);
    }

    #[test]
    fn underscore_separator() {
        let e = lower_one_expr("1_000_000");
        let HirExpr::Literal(HirLiteral::Number(n)) = e else { panic!() };
        assert_eq!(n, 1000000.0);
    }

    #[test]
    fn hex_with_underscore() {
        let e = lower_one_expr("0xDE_AD");
        let HirExpr::Literal(HirLiteral::Number(n)) = e else { panic!() };
        assert_eq!(n, 0xDEAD as f64);
    }
}
