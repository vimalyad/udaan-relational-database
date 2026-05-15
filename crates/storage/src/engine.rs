use core::error::{CrdtError, CrdtResult};
use core::types::{Frontier, Row, RowId, TableId, Tombstone, UniquenessClaim};
use std::collections::BTreeMap;

/// In-memory canonical row store.
/// All state is held in deterministic BTreeMaps.
/// Persistence is a future concern; this is the authoritative runtime store.
#[derive(Debug, Clone, Default)]
pub struct StorageEngine {
    /// table_id -> row_id -> Row
    tables: BTreeMap<TableId, BTreeMap<RowId, Row>>,
}

impl StorageEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new (empty) table namespace.
    pub fn create_table(&mut self, table_id: &str) {
        self.tables.entry(table_id.to_string()).or_default();
    }

    /// Upsert a row into a table.
    pub fn upsert_row(&mut self, table_id: &str, row: Row) -> CrdtResult<()> {
        let table = self.tables.get_mut(table_id)
            .ok_or_else(|| CrdtError::TableNotFound(table_id.to_string()))?;
        table.insert(row.id.clone(), row);
        Ok(())
    }

    /// Get a row (including tombstoned rows for internal use).
    pub fn get_row(&self, table_id: &str, row_id: &str) -> Option<&Row> {
        self.tables.get(table_id)?.get(row_id)
    }

    /// Get a visible (non-tombstoned) row.
    pub fn get_visible_row(&self, table_id: &str, row_id: &str) -> Option<&Row> {
        let row = self.get_row(table_id, row_id)?;
        if row.is_visible() { Some(row) } else { None }
    }

    /// Iterate all rows in a table (including tombstoned).
    pub fn all_rows(&self, table_id: &str) -> impl Iterator<Item = &Row> {
        self.tables.get(table_id).into_iter().flat_map(|t| t.values())
    }

    /// Iterate visible rows only (sorted by primary key — BTreeMap guarantees this).
    pub fn visible_rows(&self, table_id: &str) -> impl Iterator<Item = &Row> {
        self.all_rows(table_id).filter(|r| r.is_visible())
    }

    /// Get all rows as a snapshot (for sync delta generation).
    pub fn snapshot_table(&self, table_id: &str) -> Option<&BTreeMap<RowId, Row>> {
        self.tables.get(table_id)
    }

    pub fn table_names(&self) -> Vec<TableId> {
        self.tables.keys().cloned().collect()
    }

    pub fn table_exists(&self, table_id: &str) -> bool {
        self.tables.contains_key(table_id)
    }
}
