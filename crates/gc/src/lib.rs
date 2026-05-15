//! Causal-stability-based tombstone and metadata garbage collection.
//!
//! GC safety invariant: a tombstone can only be reclaimed when ALL known peers
//! have seen the delete event — i.e., their frontier >= tombstone.version.
//!
//! This prevents zombie rows: if peer C hasn't seen the delete yet, syncing
//! with C after GC would re-insert the tombstoned row.

use core::types::{Frontier, Tombstone};
use core::utils::frontier_dominates;

/// Check if a tombstone is causally stable (safe to GC).
pub fn is_tombstone_stable(tombstone: &Tombstone, global_min_frontier: &Frontier) -> bool {
    let mut ts_requires = Frontier::new();
    ts_requires.insert(tombstone.version.peer_id.clone(), tombstone.version.counter);
    frontier_dominates(global_min_frontier, &ts_requires)
}

/// Partition tombstones into (retained, garbage-collected).
pub fn collect_stable_tombstones(
    tombstones: Vec<Tombstone>,
    global_min_frontier: &Frontier,
) -> (Vec<Tombstone>, Vec<Tombstone>) {
    tombstones
        .into_iter()
        .partition(|ts| !is_tombstone_stable(ts, global_min_frontier))
}

/// Run GC on a TombstoneStore given the global minimum frontier.
/// Returns the number of tombstones collected.
pub fn run_gc(
    tombstones: &mut crdt::TombstoneStore,
    storage: &mut storage::StorageEngine,
    global_min_frontier: &Frontier,
) -> usize {
    let all: Vec<Tombstone> = tombstones.all().cloned().collect();
    let mut collected = 0;

    for ts in &all {
        if is_tombstone_stable(ts, global_min_frontier) {
            // Remove tombstone metadata
            storage.gc_row(&ts.table_id, &ts.row_id);
            collected += 1;
        }
    }

    // Remove stable tombstones from the store
    tombstones.collect_stable(global_min_frontier);

    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::Version;
    use crdt::TombstoneStore;
    use storage::StorageEngine;

    fn tombstone(row_id: &str, table: &str, counter: u64, peer: &str) -> Tombstone {
        Tombstone {
            row_id: row_id.to_string(),
            table_id: table.to_string(),
            version: Version::new(counter, peer.to_string()),
        }
    }

    #[test]
    fn stable_when_all_peers_seen() {
        let ts = tombstone("r1", "t", 5, "A");
        let mut frontier = Frontier::new();
        frontier.insert("A".to_string(), 10); // A has seen counter 10 >= 5
        assert!(is_tombstone_stable(&ts, &frontier));
    }

    #[test]
    fn not_stable_when_peer_behind() {
        let ts = tombstone("r1", "t", 10, "A");
        let mut frontier = Frontier::new();
        frontier.insert("A".to_string(), 5); // A only at 5 < 10
        assert!(!is_tombstone_stable(&ts, &frontier));
    }

    #[test]
    fn gc_removes_stable_tombstones() {
        let mut store = TombstoneStore::new();
        let mut storage = StorageEngine::new();
        storage.create_table("users");

        let ts = tombstone("u1", "users", 5, "A");
        store.insert(ts);

        let mut frontier = Frontier::new();
        frontier.insert("A".to_string(), 10);

        let count = run_gc(&mut store, &mut storage, &frontier);
        assert_eq!(count, 1);
        assert_eq!(store.all().count(), 0);
    }

    #[test]
    fn gc_retains_unstable_tombstones() {
        let mut store = TombstoneStore::new();
        let mut storage = StorageEngine::new();

        store.insert(tombstone("r1", "t", 15, "A"));

        let mut frontier = Frontier::new();
        frontier.insert("A".to_string(), 10); // Behind

        run_gc(&mut store, &mut storage, &frontier);
        assert_eq!(store.all().count(), 1); // Still there
    }
}
