//! Deterministic secondary index engine.
//! Indexes are purely derived state — rebuilt from canonical merged rows.
//! This guarantees convergence: identical row state → identical indexes.

use core::types::{IndexDef, Row, RowId, TableId, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Secondary index: ordered map from (index key columns) -> set of row IDs.
/// BTreeMap ensures deterministic range scans across all replicas.
#[derive(Debug, Clone)]
pub struct SecondaryIndex {
    pub def: IndexDef,
    /// Sorted column values → sorted set of matching row IDs
    entries: BTreeMap<Vec<Value>, BTreeSet<RowId>>,
}

impl SecondaryIndex {
    pub fn new(def: IndexDef) -> Self {
        Self {
            def,
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: Vec<Value>, row_id: RowId) {
        self.entries.entry(key).or_default().insert(row_id);
    }

    pub fn remove(&mut self, key: &[Value], row_id: &str) {
        if let Some(set) = self.entries.get_mut(key) {
            set.remove(row_id);
            if set.is_empty() {
                self.entries.remove(key);
            }
        }
    }

    /// Exact lookup for a key value.
    pub fn lookup(&self, key: &[Value]) -> impl Iterator<Item = &RowId> {
        self.entries.get(key).into_iter().flat_map(|s| s.iter())
    }

    /// Range scan: returns row IDs for keys in [start, end).
    /// Results are in deterministic (BTreeMap) order.
    pub fn range_scan(&self, start: Option<Vec<Value>>, end: Option<Vec<Value>>) -> Vec<RowId> {
        let iter: Box<dyn Iterator<Item = (&Vec<Value>, &BTreeSet<RowId>)>> = match (start, end) {
            (Some(s), Some(e)) => Box::new(self.entries.range(s..e)),
            (Some(s), None) => Box::new(self.entries.range(s..)),
            (None, Some(e)) => Box::new(self.entries.range(..e)),
            (None, None) => Box::new(self.entries.iter()),
        };
        iter.flat_map(|(_, set)| set.iter().cloned()).collect()
    }

    /// All row IDs in index order.
    pub fn all_row_ids(&self) -> Vec<RowId> {
        self.entries
            .values()
            .flat_map(|s| s.iter().cloned())
            .collect()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.values().map(|s| s.len()).sum()
    }
}

/// Manages all secondary indexes for all tables.
#[derive(Debug, Clone, Default)]
pub struct IndexManager {
    /// table_id -> index_name -> SecondaryIndex
    indexes: BTreeMap<TableId, BTreeMap<String, SecondaryIndex>>,
}

impl IndexManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_index(&mut self, def: IndexDef) {
        self.indexes
            .entry(def.table.clone())
            .or_default()
            .insert(def.name.clone(), SecondaryIndex::new(def));
    }

    /// Fully rebuild all indexes for a table from the current row set.
    /// Called after any sync merge to ensure convergence.
    pub fn rebuild_table<'a>(&mut self, table_id: &str, rows: impl Iterator<Item = &'a Row>) {
        if let Some(table_indexes) = self.indexes.get_mut(table_id) {
            for idx in table_indexes.values_mut() {
                idx.entries.clear();
            }
            for row in rows {
                if !row.is_visible() {
                    continue;
                }
                for idx in table_indexes.values_mut() {
                    let key = extract_index_key(&idx.def.columns, row);
                    idx.insert(key, row.id.clone());
                }
            }
        }
    }

    /// Incremental update: remove old key, insert new key for a row.
    pub fn update_row(&mut self, table_id: &str, old_row: Option<&Row>, new_row: &Row) {
        if let Some(table_indexes) = self.indexes.get_mut(table_id) {
            for idx in table_indexes.values_mut() {
                if let Some(old) = old_row {
                    if old.is_visible() {
                        let old_key = extract_index_key(&idx.def.columns, old);
                        idx.remove(&old_key, &old.id);
                    }
                }
                if new_row.is_visible() {
                    let new_key = extract_index_key(&idx.def.columns, new_row);
                    idx.insert(new_key, new_row.id.clone());
                }
            }
        }
    }

    pub fn get_index(&self, table_id: &str, index_name: &str) -> Option<&SecondaryIndex> {
        self.indexes.get(table_id)?.get(index_name)
    }

    pub fn indexes_for_table(&self, table_id: &str) -> Vec<&SecondaryIndex> {
        self.indexes
            .get(table_id)
            .map(|m| m.values().collect())
            .unwrap_or_default()
    }
}

fn extract_index_key(columns: &[String], row: &Row) -> Vec<Value> {
    columns
        .iter()
        .map(|col| {
            row.cells
                .get(col)
                .map(|c| c.value.clone())
                .unwrap_or(Value::Null)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{Cell, Value, Version};

    fn row_with_cols(id: &str, cols: &[(&str, &str)]) -> Row {
        let mut row = Row::new(id);
        for (col, val) in cols {
            row.cells.insert(
                col.to_string(),
                Cell::new(
                    Value::Text(val.to_string()),
                    Version::new(1, "A".to_string()),
                ),
            );
        }
        row
    }

    fn make_index_def(name: &str, table: &str, columns: &[&str]) -> IndexDef {
        IndexDef {
            name: name.to_string(),
            table: table.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            unique: false,
        }
    }

    #[test]
    fn insert_and_lookup() {
        let def = make_index_def("idx_name", "users", &["name"]);
        let mut idx = SecondaryIndex::new(def);
        idx.insert(vec![Value::Text("Alice".to_string())], "u1".to_string());
        let found: Vec<_> = idx.lookup(&[Value::Text("Alice".to_string())]).collect();
        assert_eq!(found, vec![&"u1".to_string()]);
    }

    #[test]
    fn remove_entry() {
        let def = make_index_def("idx", "t", &["x"]);
        let mut idx = SecondaryIndex::new(def);
        idx.insert(vec![Value::Integer(1)], "r1".to_string());
        idx.insert(vec![Value::Integer(1)], "r2".to_string());
        idx.remove(&[Value::Integer(1)], "r1");
        let found: Vec<_> = idx.lookup(&[Value::Integer(1)]).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], "r2");
    }

    #[test]
    fn range_scan_ordered() {
        let def = make_index_def("idx", "t", &["score"]);
        let mut idx = SecondaryIndex::new(def);
        idx.insert(vec![Value::Integer(30)], "r3".to_string());
        idx.insert(vec![Value::Integer(10)], "r1".to_string());
        idx.insert(vec![Value::Integer(20)], "r2".to_string());

        // Scan all
        let all = idx.range_scan(None, None);
        assert_eq!(all, vec!["r1", "r2", "r3"]);

        // Range [10, 25)
        let ranged = idx.range_scan(
            Some(vec![Value::Integer(10)]),
            Some(vec![Value::Integer(25)]),
        );
        assert_eq!(ranged, vec!["r1", "r2"]);
    }

    #[test]
    fn rebuild_excludes_tombstoned() {
        let def = make_index_def("idx_name", "users", &["name"]);
        let mut mgr = IndexManager::new();
        mgr.create_index(def);

        let r1 = row_with_cols("u1", &[("name", "Alice")]);
        let mut r2 = row_with_cols("u2", &[("name", "Bob")]);
        r2.deleted = true;
        r2.delete_version = Some(Version::new(2, "B".to_string()));

        let rows = [r1, r2];
        mgr.rebuild_table("users", rows.iter());

        let idx = mgr.get_index("users", "idx_name").unwrap();
        assert_eq!(idx.entry_count(), 1); // Bob's row excluded
        assert_eq!(idx.lookup(&[Value::Text("Alice".to_string())]).count(), 1);
        assert_eq!(idx.lookup(&[Value::Text("Bob".to_string())]).count(), 0);
    }

    #[test]
    fn index_deterministic_across_insert_orders() {
        let def1 = make_index_def("idx", "t", &["val"]);
        let def2 = make_index_def("idx", "t", &["val"]);
        let mut mgr1 = IndexManager::new();
        let mut mgr2 = IndexManager::new();
        mgr1.create_index(def1);
        mgr2.create_index(def2);

        let r1 = row_with_cols("r1", &[("val", "a")]);
        let r2 = row_with_cols("r2", &[("val", "b")]);
        let r3 = row_with_cols("r3", &[("val", "c")]);

        // Insert in different orders
        mgr1.rebuild_table("t", [&r1, &r2, &r3].into_iter());
        mgr2.rebuild_table("t", [&r3, &r1, &r2].into_iter());

        let ids1 = mgr1.get_index("t", "idx").unwrap().all_row_ids();
        let ids2 = mgr2.get_index("t", "idx").unwrap().all_row_ids();
        assert_eq!(
            ids1, ids2,
            "index must be deterministic regardless of insert order"
        );
    }
}
