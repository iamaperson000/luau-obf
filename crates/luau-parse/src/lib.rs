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
