use core::error::CrdtResult;
use core::types::{Row, RowId, TableId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// In-memory canonical row store.
/// All state is held in deterministic BTreeMaps — no HashMap.
/// Physical layout matches design: tables/ rows/ cells/ tombstones/
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageEngine {
    /// table_id -> row_id -> Row (BTreeMap ensures deterministic iteration)
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

    pub fn table_exists(&self, table_id: &str) -> bool {
        self.tables.contains_key(table_id)
    }

    /// Upsert a row into a table (create table namespace if missing).
    pub fn upsert_row(&mut self, table_id: &str, row: Row) -> CrdtResult<()> {
        self.tables
            .entry(table_id.to_string())
            .or_default()
            .insert(row.id.clone(), row);
        Ok(())
    }

    /// Get any row including tombstoned (for internal/sync use).
    pub fn get_row(&self, table_id: &str, row_id: &str) -> Option<&Row> {
        self.tables.get(table_id)?.get(row_id)
    }

    /// Get only a visible (non-tombstoned) row.
    pub fn get_visible_row(&self, table_id: &str, row_id: &str) -> Option<&Row> {
        let row = self.get_row(table_id, row_id)?;
        if row.is_visible() {
            Some(row)
        } else {
            None
        }
    }

    /// Iterate all rows in a table including tombstoned (sorted by row_id — BTreeMap order).
    pub fn all_rows(&self, table_id: &str) -> impl Iterator<Item = &Row> {
        self.tables
            .get(table_id)
            .into_iter()
            .flat_map(|t| t.values())
    }

    /// Iterate only visible rows (sorted by primary key — BTreeMap order guarantees this).
    pub fn visible_rows(&self, table_id: &str) -> impl Iterator<Item = &Row> {
        self.all_rows(table_id).filter(|r| r.is_visible())
    }

    /// Snapshot the full table state including tombstoned rows (for sync).
    pub fn snapshot_table(&self, table_id: &str) -> Option<&BTreeMap<RowId, Row>> {
        self.tables.get(table_id)
    }

    /// Return all table names in deterministic (sorted) order.
    pub fn table_names(&self) -> Vec<TableId> {
        self.tables.keys().cloned().collect()
    }

    /// Count of visible rows in a table.
    pub fn visible_count(&self, table_id: &str) -> usize {
        self.visible_rows(table_id).count()
    }

    /// Count of all rows including tombstoned.
    pub fn total_count(&self, table_id: &str) -> usize {
        self.all_rows(table_id).count()
    }

    /// Remove a tombstoned row that is causally stable (GC step).
    /// Only call after confirming causal stability.
    pub fn gc_row(&mut self, table_id: &str, row_id: &str) {
        if let Some(table) = self.tables.get_mut(table_id) {
            table.remove(row_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{Cell, Value, Version};

    fn make_row(id: &str, col: &str, val: i64, counter: u64, peer: &str) -> Row {
        let mut row = Row::new(id);
        row.cells.insert(
            col.to_string(),
            Cell::new(Value::Integer(val), Version::new(counter, peer.to_string())),
        );
        row
    }

    #[test]
    fn upsert_and_get_row() {
        let mut engine = StorageEngine::new();
        engine.create_table("users");
        let row = make_row("u1", "name", 42, 1, "A");
        engine.upsert_row("users", row.clone()).unwrap();
        let got = engine.get_visible_row("users", "u1").unwrap();
        assert_eq!(got.id, "u1");
    }

    #[test]
    fn tombstoned_row_not_visible() {
        let mut engine = StorageEngine::new();
        engine.create_table("users");
        let mut row = make_row("u1", "name", 42, 1, "A");
        row.deleted = true;
        row.delete_version = Some(Version::new(2, "B".to_string()));
        engine.upsert_row("users", row).unwrap();
        assert!(engine.get_visible_row("users", "u1").is_none());
        assert!(engine.get_row("users", "u1").is_some()); // still accessible internally
    }

    #[test]
    fn visible_rows_sorted_by_id() {
        let mut engine = StorageEngine::new();
        engine.create_table("users");
        engine
            .upsert_row("users", make_row("u3", "x", 3, 1, "A"))
            .unwrap();
        engine
            .upsert_row("users", make_row("u1", "x", 1, 1, "A"))
            .unwrap();
        engine
            .upsert_row("users", make_row("u2", "x", 2, 1, "A"))
            .unwrap();
        let ids: Vec<_> = engine
            .visible_rows("users")
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids, vec!["u1", "u2", "u3"]); // BTreeMap order
    }

    #[test]
    fn table_names_deterministic() {
        let mut engine = StorageEngine::new();
        engine.create_table("orders");
        engine.create_table("users");
        engine.create_table("items");
        let names = engine.table_names();
        assert_eq!(names, vec!["items", "orders", "users"]); // sorted
    }

    #[test]
    fn roundtrip_persistence_serialization() {
        use crate::serialization::{from_cbor, to_cbor};
        let mut engine = StorageEngine::new();
        engine.create_table("users");
        engine
            .upsert_row("users", make_row("u1", "name", 10, 5, "A"))
            .unwrap();
        let bytes = to_cbor(&engine).unwrap();
        let recovered: StorageEngine = from_cbor(&bytes).unwrap();
        let row = recovered.get_visible_row("users", "u1").unwrap();
        assert_eq!(row.id, "u1");
    }
}
