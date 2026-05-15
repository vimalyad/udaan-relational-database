use core::types::{Frontier, RowId, TableId, Tombstone};
use core::utils::frontier_dominates;
use std::collections::BTreeMap;

/// Tracks tombstones and supports causal-stability-based GC.
#[derive(Debug, Clone, Default)]
pub struct TombstoneStore {
    /// table_id -> row_id -> tombstone
    tombstones: BTreeMap<TableId, BTreeMap<RowId, Tombstone>>,
}

impl TombstoneStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, tombstone: Tombstone) {
        self.tombstones
            .entry(tombstone.table_id.clone())
            .or_default()
            .insert(tombstone.row_id.clone(), tombstone);
    }

    pub fn get(&self, table_id: &str, row_id: &str) -> Option<&Tombstone> {
        self.tombstones.get(table_id)?.get(row_id)
    }

    pub fn contains(&self, table_id: &str, row_id: &str) -> bool {
        self.tombstones
            .get(table_id)
            .is_some_and(|t| t.contains_key(row_id))
    }

    pub fn all_for_table(&self, table_id: &str) -> impl Iterator<Item = &Tombstone> {
        self.tombstones
            .get(table_id)
            .into_iter()
            .flat_map(|t| t.values())
    }

    pub fn all(&self) -> impl Iterator<Item = &Tombstone> {
        self.tombstones.values().flat_map(|t| t.values())
    }

    /// Merge in tombstones from another store. Uses max-version semantics.
    pub fn merge(&mut self, other: &TombstoneStore) {
        for ts in other.all() {
            let existing = self.get(&ts.table_id, &ts.row_id);
            let should_insert = match existing {
                None => true,
                Some(e) => ts.version > e.version,
            };
            if should_insert {
                self.insert(ts.clone());
            }
        }
    }

    /// GC: remove tombstones that are causally stable (all known peers have seen them).
    pub fn collect_stable(&mut self, global_frontier: &Frontier) {
        for table_map in self.tombstones.values_mut() {
            table_map.retain(|_, ts| {
                // Tombstone is stable when all peers in the frontier have seen its version.
                let ts_frontier: Frontier = {
                    let mut f = Frontier::new();
                    f.insert(ts.version.peer_id.clone(), ts.version.counter);
                    f
                };
                !frontier_dominates(global_frontier, &ts_frontier)
            });
        }
    }

    pub fn into_vec(self) -> Vec<Tombstone> {
        self.tombstones
            .into_values()
            .flat_map(|m| m.into_values())
            .collect()
    }
}
