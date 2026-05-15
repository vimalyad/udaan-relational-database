//! SQL executor stub. Phase 6 will fully implement this.

use core::error::CrdtResult;
use query::QueryResult;

pub struct SqlExecutor;

impl SqlExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SqlExecutor {
    fn default() -> Self {
        Self::new()
    }
}
