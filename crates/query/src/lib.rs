//! Query execution engine. Phase 5/6 placeholder.
//! Executes SELECT/WHERE/ORDER BY/LIMIT against the local materialized replica.

use core::types::Value;

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl QueryResult {
    pub fn empty() -> Self {
        Self {
            columns: vec![],
            rows: vec![],
        }
    }

    pub fn new(columns: Vec<String>, rows: Vec<Vec<Value>>) -> Self {
        Self { columns, rows }
    }
}
