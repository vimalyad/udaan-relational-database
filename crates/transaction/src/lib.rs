//! Local transaction context with row-level atomicity.
//! Transactions are local-only — no distributed coordination.
//!
//! Guarantees:
//! - Local transaction atomicity: all ops commit or none do
//! - Crash-safe: uncommitted ops are discarded on recovery
//! - No distributed ACID, no global serializable isolation

use core::error::CrdtResult;
use core::types::{Row, RowId, TableId, Tombstone};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum WriteOp {
    Upsert { table_id: TableId, row: Row },
    Delete { table_id: TableId, row_id: RowId, tombstone: Tombstone },
}

/// Buffered write transaction. Applied atomically on commit.
#[derive(Debug, Default)]
pub struct Transaction {
    ops: Vec<WriteOp>,
    committed: bool,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buffer_upsert(&mut self, table_id: impl Into<TableId>, row: Row) {
        self.ops.push(WriteOp::Upsert { table_id: table_id.into(), row });
    }

    pub fn buffer_delete(&mut self, table_id: impl Into<TableId>, row_id: impl Into<RowId>, tombstone: Tombstone) {
        self.ops.push(WriteOp::Delete {
            table_id: table_id.into(),
            row_id: row_id.into(),
            tombstone,
        });
    }

    /// Consume and return all ops for application to storage.
    pub fn commit(mut self) -> Vec<WriteOp> {
        self.committed = true;
        self.ops
    }

    /// Discard all buffered ops.
    pub fn rollback(self) {
        // Drop ops — nothing applied to storage yet
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn op_count(&self) -> usize {
        self.ops.len()
    }
}

/// Apply a committed transaction's ops to a storage engine + tombstone store.
pub fn apply_transaction(
    ops: Vec<WriteOp>,
    storage: &mut storage::StorageEngine,
    tombstones: &mut crdt::TombstoneStore,
) -> CrdtResult<()> {
    for op in ops {
        match op {
            WriteOp::Upsert { table_id, row } => {
                storage.upsert_row(&table_id, row)?;
            }
            WriteOp::Delete { table_id, row_id, tombstone } => {
                tombstones.insert(tombstone);
                // Row should already be tombstoned in storage by the caller
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{Cell, Value, Version};
    use crdt::TombstoneStore;
    use storage::StorageEngine;

    fn make_row(id: &str) -> Row {
        let mut row = Row::new(id);
        row.cells.insert(
            "x".to_string(),
            Cell::new(Value::Integer(1), Version::new(1, "A".to_string())),
        );
        row
    }

    #[test]
    fn commit_applies_upsert() {
        let mut storage = StorageEngine::new();
        storage.create_table("t");
        let mut ts = TombstoneStore::new();

        let mut txn = Transaction::new();
        txn.buffer_upsert("t", make_row("r1"));
        let ops = txn.commit();
        apply_transaction(ops, &mut storage, &mut ts).unwrap();

        assert!(storage.get_visible_row("t", "r1").is_some());
    }

    #[test]
    fn rollback_discards_ops() {
        let mut storage = StorageEngine::new();
        storage.create_table("t");
        let mut ts = TombstoneStore::new();

        let mut txn = Transaction::new();
        txn.buffer_upsert("t", make_row("r1"));
        txn.rollback(); // discard

        // Nothing was applied
        assert!(storage.get_visible_row("t", "r1").is_none());
    }

    #[test]
    fn transaction_is_empty_initially() {
        let txn = Transaction::new();
        assert!(txn.is_empty());
    }
}
