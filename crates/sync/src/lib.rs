//! Anti-entropy sync engine. Phase 4 implementation.
//! Provides pairwise, bidirectional, replay-safe synchronization.

use core::error::{CrdtError, CrdtResult};
use core::types::{Frontier, PeerId, RowDelta, SyncDelta, TableId, Tombstone, UniquenessClaim};
use core::utils::{frontier_update, merge_frontiers};
use crdt::merge::{merge_row, merge_table};
use crdt::{TombstoneStore, UniquenessRegistry};
use replication::ReplicaState;
use std::collections::BTreeMap;

/// Extract a SyncDelta from a replica: rows/tombstones/claims that the remote
/// peer hasn't seen yet (based on frontier comparison).
pub fn extract_delta(replica: &ReplicaState, remote_frontier: &Frontier) -> SyncDelta {
    let mut rows: Vec<RowDelta> = Vec::new();

    for table_name in replica.storage.table_names() {
        if let Some(table) = replica.storage.snapshot_table(&table_name) {
            for row in table.values() {
                // Include row if any of its cell versions are newer than the remote frontier knows
                let row_is_new = row.cells.values().any(|cell| {
                    let peer_frontier_counter = remote_frontier.get(&cell.version.peer_id).copied().unwrap_or(0);
                    cell.version.counter > peer_frontier_counter
                }) || row.delete_version.as_ref().map_or(false, |dv| {
                    let peer_frontier_counter = remote_frontier.get(&dv.peer_id).copied().unwrap_or(0);
                    dv.counter > peer_frontier_counter
                });

                if row_is_new {
                    rows.push(RowDelta { table_id: table_name.clone(), row: row.clone() });
                }
            }
        }
    }

    // Include tombstones not yet seen by remote
    let tombstones: Vec<Tombstone> = replica.tombstones.all()
        .filter(|ts| {
            let peer_counter = remote_frontier.get(&ts.version.peer_id).copied().unwrap_or(0);
            ts.version.counter > peer_counter
        })
        .cloned()
        .collect();

    // Include uniqueness claims
    let uniqueness_claims: Vec<UniquenessClaim> = replica.uniqueness.all_claims().cloned().collect();

    SyncDelta {
        source_peer: replica.peer_id.clone(),
        rows,
        tombstones,
        uniqueness_claims,
        frontier: replica.frontier.clone(),
    }
}

/// Apply a received SyncDelta to a replica.
/// This is replay-safe, idempotent, and order-independent.
pub fn apply_delta(replica: &mut ReplicaState, delta: &SyncDelta) -> CrdtResult<()> {
    // Merge rows
    for row_delta in &delta.rows {
        let table_id = &row_delta.table_id;

        // Ensure table exists in storage
        if !replica.storage.table_exists(table_id) {
            replica.storage.create_table(table_id);
        }

        let existing = replica.storage.get_row(table_id, &row_delta.row.id).cloned();
        let merged = match existing {
            Some(existing_row) => merge_row(&existing_row, &row_delta.row),
            None => row_delta.row.clone(),
        };

        replica.storage.upsert_row(table_id, merged)?;
    }

    // Merge tombstones
    let mut incoming_ts_store = TombstoneStore::new();
    for ts in &delta.tombstones {
        incoming_ts_store.insert(ts.clone());
    }
    replica.tombstones.merge(&incoming_ts_store);

    // Merge uniqueness claims
    let mut incoming_uniqueness = UniquenessRegistry::new();
    for claim in &delta.uniqueness_claims {
        incoming_uniqueness.claim(
            &claim.table_id,
            &claim.column_id,
            &claim.value,
            &claim.owner_row,
            claim.version.clone(),
        );
    }
    replica.uniqueness.merge(&incoming_uniqueness);

    // Advance clock from remote frontier
    replica.clock.update_from_frontier(&delta.frontier);

    // Merge frontiers
    replica.frontier = merge_frontiers(&replica.frontier, &delta.frontier);

    // Update our own frontier entry
    frontier_update(&mut replica.frontier, &replica.peer_id, replica.clock.counter);

    Ok(())
}

/// Perform a full pairwise bidirectional sync between two replicas.
/// After this call, both replicas have seen all each other's state.
/// This function is called repeatedly until quiescence (no new deltas exchanged).
pub fn sync_peers(a: &mut ReplicaState, b: &mut ReplicaState) -> CrdtResult<()> {
    // Extract deltas based on current frontiers
    let delta_for_b = extract_delta(a, &b.frontier);
    let delta_for_a = extract_delta(b, &a.frontier);

    // Apply deltas
    apply_delta(b, &delta_for_b)?;
    apply_delta(a, &delta_for_a)?;

    Ok(())
}
