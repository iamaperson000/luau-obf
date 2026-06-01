//! AST → HIR lowering. Plan 1 supports the subset documented in the plan.

use crate::{
    BinOp, HirError, HirExpr, HirFunction, HirLiteral, HirProgram, HirStmt, Symbol, SymbolId,
    SymbolKind, TableEntry, UnOp,
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
    /// Parallel to `scopes`: scope_function_depth[i] is the function-nesting depth
    /// of scopes[i]. The top-level chunk is depth 0; each nested function pushes
    /// a fresh depth one higher.
    scope_function_depth: Vec<u32>,
    /// The function-nesting depth currently being lowered.
    current_function_depth: u32,
}

impl Lowerer {
    fn new() -> Self {
        Self {
            symbols: Vec::new(),
            scopes: vec![HashMap::new()],
            scope_function_depth: vec![0],
            current_function_depth: 0,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_function_depth.push(self.current_function_depth);
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.scope_function_depth.pop();
    }

    fn enter_function(&mut self) {
        self.current_function_depth += 1;
        self.push_scope();
    }

    fn exit_function(&mut self) {
        self.pop_scope();
        self.current_function_depth -= 1;
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
    /// out; returns the matching local if it's in the current function frame.
    /// If a local with a shallower function-frame is found, returns an error
    /// (upvalue capture is deferred to Plan 3). If no local matches, treats
    /// the name as a global reference.
    fn resolve(&mut self, name: &str) -> Result<SymbolId, HirError> {
        for (i, scope) in self.scopes.iter().enumerate().rev() {
            if let Some(&id) = scope.get(name) {
                if self.scope_function_depth[i] == self.current_function_depth {
                    return Ok(id);
                }
                return Err(HirError::Unsupported(format!(
                    "function body captures parent local `{}` (upvalues deferred to Plan 3)",
                    name
                )));
            }
        }
        Ok(self.declare_global(name))
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
                let value = lower_expr(lowerer, expr)?;
                out.push(lower_assign_target(lowerer, var, value)?);
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
        Stmt::FunctionDeclaration(fd) => lower_function_decl(lowerer, fd),
        Stmt::LocalFunction(_) => Err(HirError::Unsupported(
            "`local function` (closures with upvalues are Plan 3)".into(),
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
        LastStmt::Break(_) => Ok(HirStmt::Break),
        LastStmt::Continue(_) => Err(HirError::Unsupported("continue (Plan 4)".into())),
        other => Err(HirError::Unsupported(format!("last stmt form {other:?}"))),
    }
}

fn lower_function_body(
    lowerer: &mut Lowerer,
    body: &full_moon::ast::FunctionBody,
) -> Result<HirFunction, HirError> {
    lower_function_body_with_self(lowerer, body, false)
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
        E::TableConstructor(tc) => lower_table_ctor(lowerer, tc),
        E::Function(anon) => {
            let function = lower_function_body(lowerer, &anon.1)?;
            Ok(HirExpr::Function(function))
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
            Ok(HirExpr::Symbol(lowerer.resolve(&name)?))
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
        Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())?),
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
        Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())?),
        Prefix::Expression(e) => lower_expr(lowerer, e)?,
        other => return Err(HirError::Unsupported(format!("call prefix {other:?}"))),
    };
    for suffix in call.suffixes() {
        current = lower_suffix(lowerer, current, suffix)?;
    }
    Ok(current)
}

/// Apply one suffix (call or index) to the expression accumulated so far.
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
        Suffix::Call(Call::MethodCall(mc)) => {
            let method = mc.name().token().to_string();
            let args = lower_call_args(lowerer, mc.args())?;
            Ok(HirExpr::MethodCall {
                obj: Box::new(current),
                method,
                args,
            })
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

fn lower_assign_target(
    lowerer: &mut Lowerer,
    var: &full_moon::ast::Var,
    value: HirExpr,
) -> Result<HirStmt, HirError> {
    use full_moon::ast::{Index, Prefix, Suffix, Var};
    match var {
        Var::Name(t) => {
            let target = lowerer.resolve(&t.token().to_string())?;
            Ok(HirStmt::Assign { target, value })
        }
        Var::Expression(ve) => {
            let suffixes: Vec<&Suffix> = ve.suffixes().collect();
            if suffixes.is_empty() {
                return Err(HirError::Unsupported("empty VarExpression in assign LHS".into()));
            }
            let (last, rest) = suffixes.split_last().unwrap();
            let mut obj = match ve.prefix() {
                Prefix::Name(t) => HirExpr::Symbol(lowerer.resolve(&t.token().to_string())?),
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
                entries.push(TableEntry::Keyed(k, v));
            }
            Field::NameKey { key, value, .. } => {
                let name = key.token().to_string();
                let v = lower_expr(lowerer, value)?;
                entries.push(TableEntry::Field(name, v));
            }
            Field::NoKey(value) => {
                let v = lower_expr(lowerer, value)?;
                entries.push(TableEntry::Array(v));
            }
            other => return Err(HirError::Unsupported(format!("table field {other:?}"))),
        }
    }
    Ok(HirExpr::Table(entries))
}

fn lower_function_decl(
    lowerer: &mut Lowerer,
    fd: &full_moon::ast::FunctionDeclaration,
) -> Result<Vec<HirStmt>, HirError> {
    let names: Vec<String> = fd.name().names().iter().map(|t| t.token().to_string()).collect();
    let method_name: Option<String> = fd.name().method_name().map(|t| t.token().to_string());

    let function = lower_function_body_with_self(lowerer, fd.body(), method_name.is_some())?;

    // Simple top-level decl: single name, no method colon.
    if names.len() == 1 && method_name.is_none() {
        let symbol = lowerer.resolve(&names[0])?;
        return Ok(vec![HirStmt::FunctionDecl { name: symbol, function }]);
    }

    // Dotted / method form. Walk the head as a Symbol, intermediate names as
    // Index reads, then emit IndexAssign(obj, key, function-as-value).
    let head_sym = lowerer.resolve(&names[0])?;
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
                return Err(HirError::Unsupported("varargs `...` (Plan 3)".into()));
            }
            other => {
                lowerer.exit_function();
                return Err(HirError::Unsupported(format!("parameter form {other:?}")));
            }
        }
    }
    let body = lower_block(lowerer, body.block())?;
    lowerer.exit_function();
    Ok(HirFunction { params, body, upvalues: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TableEntry;

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
        assert!(matches!(obj, HirExpr::Index { .. }));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "c"));
    }

    #[test]
    fn lowers_break_in_while() {
        let p = lower_str("while x do break end");
        let HirStmt::While { body, .. } = &p.main[0] else { panic!() };
        assert!(matches!(&body[0], HirStmt::Break));
    }

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
        assert_eq!(f.params.len(), 2);
    }

    #[test]
    fn lowers_deep_dotted_function_decl() {
        let p = lower_str("function a.b.c() end");
        let HirStmt::IndexAssign { obj, key, .. } = &p.main[0] else { panic!() };
        let HirExpr::Index { key: inner_key, .. } = obj else { panic!() };
        assert!(matches!(inner_key.as_ref(), HirExpr::Literal(HirLiteral::String(s)) if s == "b"));
        assert!(matches!(key, HirExpr::Literal(HirLiteral::String(s)) if s == "c"));
    }

    #[test]
    fn rejects_anonymous_closure_capturing_local() {
        let ast = luau_parse::parse("local x = 1 local f = function() return x end").unwrap();
        let err = lower(&ast).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("captures parent local"), "got: {msg}");
    }

    #[test]
    fn rejects_method_decl_capturing_outer_local() {
        let ast = luau_parse::parse("local x = 1 local t = {} function t:m() return x end").unwrap();
        let err = lower(&ast).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("captures parent local"), "got: {msg}");
    }

    #[test]
    fn top_level_function_can_reference_global() {
        // Top-level `function f() print("hi") end` references `print` as a global —
        // that still works because globals are NOT looked up via the local-scope path.
        let p = lower_str("function f() print(\"hi\") end");
        assert_eq!(p.main.len(), 1);
    }

    #[test]
    fn nested_function_can_access_own_params() {
        // The new closure-capture check must NOT block use of the function's own params.
        let p = lower_str("local f = function(x) return x + 1 end");
        assert_eq!(p.main.len(), 1);
    }

    #[test]
    fn nested_function_can_access_globals() {
        let p = lower_str("local f = function() return print(\"hi\") end");
        assert_eq!(p.main.len(), 1);
    }
}
