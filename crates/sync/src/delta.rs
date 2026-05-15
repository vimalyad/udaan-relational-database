use core::error::CrdtResult;
use core::types::{Frontier, RowDelta, SyncDelta, Tombstone, UniquenessClaim};
use core::utils::{frontier_update, merge_frontiers};
use crdt::merge::merge_row;
use crdt::{TombstoneStore, UniquenessRegistry};
use replication::ReplicaState;

/// Extract a SyncDelta from `source` containing only rows/tombstones
/// that `remote_frontier` hasn't seen yet.
pub fn extract_delta(source: &ReplicaState, remote_frontier: &Frontier) -> SyncDelta {
    let mut rows: Vec<RowDelta> = Vec::new();

    for table_name in source.storage.table_names() {
        if let Some(table) = source.storage.snapshot_table(&table_name) {
            for row in table.values() {
                let row_needs_sending = row.cells.values().any(|cell| {
                    let known = remote_frontier
                        .get(&cell.version.peer_id)
                        .copied()
                        .unwrap_or(0);
                    cell.version.counter > known
                }) || row.delete_version.as_ref().is_some_and(|dv| {
                    let known = remote_frontier.get(&dv.peer_id).copied().unwrap_or(0);
                    dv.counter > known
                });

                if row_needs_sending {
                    rows.push(RowDelta {
                        table_id: table_name.clone(),
                        row: row.clone(),
                    });
                }
            }
        }
    }

    let tombstones: Vec<Tombstone> = source
        .tombstones
        .all()
        .filter(|ts| {
            let known = remote_frontier
                .get(&ts.version.peer_id)
                .copied()
                .unwrap_or(0);
            ts.version.counter > known
        })
        .cloned()
        .collect();

    // Always send all uniqueness claims (they are small and idempotent to merge)
    let uniqueness_claims: Vec<UniquenessClaim> = source.uniqueness.all_claims().cloned().collect();

    SyncDelta {
        source_peer: source.peer_id.clone(),
        rows,
        tombstones,
        uniqueness_claims,
        frontier: source.frontier.clone(),
    }
}

/// Apply a received SyncDelta to `target`.
/// Replay-safe, idempotent, order-independent.
pub fn apply_delta(target: &mut ReplicaState, delta: &SyncDelta) -> CrdtResult<()> {
    // Merge rows: cell-level CRDT merge
    for row_delta in &delta.rows {
        let table_id = &row_delta.table_id;
        if !target.storage.table_exists(table_id) {
            target.storage.create_table(table_id);
        }

        let merged = match target.storage.get_row(table_id, &row_delta.row.id).cloned() {
            Some(existing) => merge_row(&existing, &row_delta.row),
            None => row_delta.row.clone(),
        };

        target.storage.upsert_row(table_id, merged)?;
    }

    // Merge tombstones
    let mut incoming_ts = TombstoneStore::new();
    for ts in &delta.tombstones {
        incoming_ts.insert(ts.clone());
        // Also ensure the row in storage reflects the tombstone
        if let Some(existing_row) = target.storage.get_row(&ts.table_id, &ts.row_id).cloned() {
            if !existing_row.deleted
                || existing_row
                    .delete_version
                    .as_ref()
                    .is_none_or(|ev| ts.version > *ev)
            {
                let mut updated = existing_row;
                updated.deleted = true;
                updated.delete_version = Some(ts.version.clone());
                target.storage.upsert_row(&ts.table_id, updated)?;
            }
        }
    }
    target.tombstones.merge(&incoming_ts);

    // Merge uniqueness claims
    let mut incoming_unique = UniquenessRegistry::new();
    for claim in &delta.uniqueness_claims {
        incoming_unique.claim(
            &claim.table_id,
            &claim.column_id,
            &claim.value,
            &claim.owner_row,
            claim.version.clone(),
        );
        // Propagate losers too
        for loser in &claim.losers {
            incoming_unique.claim(
                &claim.table_id,
                &claim.column_id,
                &claim.value,
                &loser.row_id,
                loser.version.clone(),
            );
        }
    }
    target.uniqueness.merge(&incoming_unique);

    // Advance Lamport clock from remote frontier
    target.clock.update_from_frontier(&delta.frontier);

    // Merge frontiers
    target.frontier = merge_frontiers(&target.frontier, &delta.frontier);
    frontier_update(&mut target.frontier, &target.peer_id, target.clock.counter);

    Ok(())
}
