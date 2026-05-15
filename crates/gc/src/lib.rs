//! Causal-stability-based tombstone and metadata garbage collection.

use core::types::{Frontier, Tombstone};
use core::utils::frontier_dominates;

/// Check if a tombstone is causally stable (safe to GC).
/// A tombstone is stable when all known peers have seen the delete version.
pub fn is_tombstone_stable(tombstone: &Tombstone, global_frontier: &Frontier) -> bool {
    let ts_min_frontier: Frontier = {
        let mut f = Frontier::new();
        f.insert(tombstone.version.peer_id.clone(), tombstone.version.counter);
        f
    };
    frontier_dominates(global_frontier, &ts_min_frontier)
}

/// Filter out GC-able tombstones from a list.
pub fn collect_stable_tombstones(
    tombstones: Vec<Tombstone>,
    global_frontier: &Frontier,
) -> (Vec<Tombstone>, Vec<Tombstone>) {
    tombstones.into_iter().partition(|ts| !is_tombstone_stable(ts, global_frontier))
}
