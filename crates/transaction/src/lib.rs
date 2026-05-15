//! Local transaction context with row-level atomicity.
//! Transactions are local-only; no distributed coordination.

use core::error::{CrdtError, CrdtResult};
use core::types::{Row, RowId, TableId};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum WriteOp {
    Upsert { table_id: TableId, row: Row },
    Delete { table_id: TableId, row_id: RowId, row: Row },
}

/// A lightweight local transaction buffer.
/// On commit, all ops are applied atomically to storage.
/// On rollback, ops are discarded.
#[derive(Debug, Default)]
pub struct Transaction {
    pub ops: Vec<WriteOp>,
    pub committed: bool,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, table_id: impl Into<TableId>, row: Row) {
        self.ops.push(WriteOp::Upsert { table_id: table_id.into(), row });
    }

    pub fn delete(&mut self, table_id: impl Into<TableId>, row_id: impl Into<RowId>, row: Row) {
        self.ops.push(WriteOp::Delete {
            table_id: table_id.into(),
            row_id: row_id.into(),
            row,
        });
    }

    pub fn rollback(mut self) {
        self.ops.clear();
    }
}
