//! Deterministic secondary index engine.
//! Indexes are derived state — rebuilt from canonical merged rows.

use core::types::{ColumnId, IndexDef, Row, RowId, TableId, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Key for a secondary index entry: (column_values, row_id)
pub type IndexKey = (Vec<Value>, RowId);

/// Secondary index: ordered map from index key to set of row IDs.
#[derive(Debug, Clone)]
pub struct SecondaryIndex {
    pub def: IndexDef,
    /// (column_values) -> BTreeSet<RowId>
    entries: BTreeMap<Vec<Value>, BTreeSet<RowId>>,
}

impl SecondaryIndex {
    pub fn new(def: IndexDef) -> Self {
        Self { def, entries: BTreeMap::new() }
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

    /// Range scan: returns sorted row IDs for keys in [start, end).
    pub fn range(&self, start: Option<&[Value]>, end: Option<&[Value]>) -> Vec<&RowId> {
        let iter: Box<dyn Iterator<Item = (&Vec<Value>, &BTreeSet<RowId>)>> = match (start, end) {
            (Some(s), Some(e)) => Box::new(self.entries.range(s.to_vec()..e.to_vec())),
            (Some(s), None) => Box::new(self.entries.range(s.to_vec()..)),
            (None, Some(e)) => Box::new(self.entries.range(..e.to_vec())),
            (None, None) => Box::new(self.entries.iter()),
        };
        iter.flat_map(|(_, set)| set.iter()).collect()
    }

    pub fn lookup(&self, key: &[Value]) -> Option<&BTreeSet<RowId>> {
        self.entries.get(key)
    }
}

/// Manages all secondary indexes for a table.
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

    /// Rebuild all indexes for a table from scratch (after merge).
    pub fn rebuild_table(&mut self, table_id: &str, rows: impl Iterator<Item = Row>) {
        if let Some(table_indexes) = self.indexes.get_mut(table_id) {
            for idx in table_indexes.values_mut() {
                idx.entries.clear();
            }
            for row in rows {
                if !row.is_visible() {
                    continue;
                }
                for idx in table_indexes.values_mut() {
                    let key: Vec<Value> = idx.def.columns.iter()
                        .map(|col| row.cells.get(col).map(|c| c.value.clone()).unwrap_or(Value::Null))
                        .collect();
                    idx.insert(key, row.id.clone());
                }
            }
        }
    }

    pub fn get_index(&self, table_id: &str, index_name: &str) -> Option<&SecondaryIndex> {
        self.indexes.get(table_id)?.get(index_name)
    }

    pub fn get_indexes_for_table(&self, table_id: &str) -> Vec<&SecondaryIndex> {
        self.indexes.get(table_id).map(|m| m.values().collect()).unwrap_or_default()
    }
}
