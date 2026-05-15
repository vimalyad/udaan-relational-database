use core::error::{CrdtError, CrdtResult};
use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

pub fn parse_sql(sql: &str) -> CrdtResult<Vec<Statement>> {
    let dialect = GenericDialect {};
    Parser::parse_sql(&dialect, sql).map_err(|e| CrdtError::ParseError(e.to_string()))
}

pub fn parse_single(sql: &str) -> CrdtResult<Statement> {
    let mut stmts = parse_sql(sql)?;
    if stmts.len() != 1 {
        return Err(CrdtError::ParseError(format!(
            "expected 1 statement, got {}",
            stmts.len()
        )));
    }
    Ok(stmts.remove(0))
}
