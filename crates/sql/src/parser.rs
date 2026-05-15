//! sqlparser-rs integration. Phase 6 placeholder.

use core::error::{CrdtError, CrdtResult};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::Statement;

pub fn parse_sql(sql: &str) -> CrdtResult<Vec<Statement>> {
    let dialect = GenericDialect {};
    Parser::parse_sql(&dialect, sql)
        .map_err(|e| CrdtError::ParseError(e.to_string()))
}
