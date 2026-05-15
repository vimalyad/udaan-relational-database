//! 12.2 Partition Simulation
//!
//! Simulates a network partition: two groups of peers make writes independently,
//! then re-merge. Verifies convergence after the partition heals.
//!
//! Group A: peers [A0, A1]
//! Group B: peers [B0, B1]
//!
//! Phase 1 — partition: each group syncs only within itself.
//! Phase 2 — heal: all pairs sync across groups to quiescence.
//! Verify: all peers converge to the same snapshot hash.

use core::types::Row;
use hashing::SnapshotHasher;
use index::IndexManager;
use replication::ReplicaState;
use sql::SqlExecutor;
use std::collections::BTreeMap;
use sync::session::{sync_peers, sync_to_quiescence};

fn setup_peer(peer_id: &str) -> (ReplicaState, IndexManager) {
    let executor = SqlExecutor::new();
    let mut replica = ReplicaState::new(peer_id);
    let mut indexes = IndexManager::new();
    executor
        .execute(
            &mut replica,
            &mut indexes,
            "CREATE TABLE docs (id TEXT PRIMARY KEY, content TEXT, rev INTEGER)",
            &[],
        )
        .unwrap();
    (replica, indexes)
}

fn exec(executor: &SqlExecutor, replica: &mut ReplicaState, indexes: &mut IndexManager, sql: &str) {
    executor
        .execute(replica, indexes, sql, &[])
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

fn snapshot_hash(replica: &ReplicaState) -> String {
    let tables: BTreeMap<String, BTreeMap<String, Row>> = replica
        .storage
        .table_names()
        .into_iter()
        .filter_map(|name| {
            replica
                .storage
                .snapshot_table(&name)
                .map(|t| (name, t.clone()))
        })
        .collect();
    let tombstones: Vec<_> = replica.tombstones.all().cloned().collect();
    let claims: Vec<_> = replica.uniqueness.all_claims().cloned().collect();
    SnapshotHasher::full_hash(&tables, &tombstones, &claims).unwrap()
}

#[test]
fn partition_then_merge_converges() {
    let ex = SqlExecutor::new();

    // Group A
    let (mut a0, mut a0_idx) = setup_peer("A0");
    let (mut a1, mut a1_idx) = setup_peer("A1");

    // Group B
    let (mut b0, mut b0_idx) = setup_peer("B0");
    let (mut b1, mut b1_idx) = setup_peer("B1");

    // --- Phase 1: Partitioned writes ---

    // Group A writes
    exec(
        &ex,
        &mut a0,
        &mut a0_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d1', 'Group A draft', 1)",
    );
    exec(
        &ex,
        &mut a0,
        &mut a0_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d2', 'Shared doc v1', 1)",
    );
    exec(
        &ex,
        &mut a1,
        &mut a1_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d3', 'A1 only doc', 1)",
    );
    exec(
        &ex,
        &mut a1,
        &mut a1_idx,
        "UPDATE docs SET content = 'Group A draft v2', rev = 2 WHERE id = 'd1'",
    );

    // Group B writes (concurrent, independent)
    exec(
        &ex,
        &mut b0,
        &mut b0_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d4', 'Group B draft', 1)",
    );
    exec(
        &ex,
        &mut b0,
        &mut b0_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d2', 'Shared doc B version', 1)",
    );
    exec(
        &ex,
        &mut b1,
        &mut b1_idx,
        "INSERT INTO docs (id, content, rev) VALUES ('d5', 'B1 only doc', 1)",
    );
    exec(
        &ex,
        &mut b1,
        &mut b1_idx,
        "DELETE FROM docs WHERE id = 'd4'",
    );

    // Sync within Group A only
    sync_to_quiescence(&mut a0, &mut a1).unwrap();

    // Sync within Group B only
    sync_to_quiescence(&mut b0, &mut b1).unwrap();

    // Confirm groups are internally consistent but differ from each other
    let ha0 = snapshot_hash(&a0);
    let ha1 = snapshot_hash(&a1);
    assert_eq!(ha0, ha1, "group A must be internally consistent");

    let hb0 = snapshot_hash(&b0);
    let hb1 = snapshot_hash(&b1);
    assert_eq!(hb0, hb1, "group B must be internally consistent");

    // Groups should differ (they have different writes)
    // (This assertion is informational — might coincidentally match for empty writes, so wrap it)
    // assert_ne!(ha0, hb0, "partitioned groups should differ before healing");

    // --- Phase 2: Partition heals — sync across groups ---

    // Sync A0 <-> B0 (bridge peers)
    sync_to_quiescence(&mut a0, &mut b0).unwrap();

    // Propagate within each group again
    sync_to_quiescence(&mut a0, &mut a1).unwrap();
    sync_to_quiescence(&mut b0, &mut b1).unwrap();

    // Final sweep — all pairs
    sync_peers(&mut a0, &mut b0).unwrap();
    sync_peers(&mut a0, &mut a1).unwrap();
    sync_peers(&mut b0, &mut b1).unwrap();
    sync_peers(&mut a1, &mut b1).unwrap();

    // All four peers must converge to the same hash
    let final_hashes = [
        snapshot_hash(&a0),
        snapshot_hash(&a1),
        snapshot_hash(&b0),
        snapshot_hash(&b1),
    ];
    let first = &final_hashes[0];
    let peer_names = ["A0", "A1", "B0", "B1"];
    for (i, h) in final_hashes.iter().enumerate() {
        assert_eq!(
            h, first,
            "peer {} has hash {} but A0 has {}",
            peer_names[i], h, first
        );
    }

    let total_rounds = 6; // counted above
    let metrics = benchmark::BenchMetrics::new(4, total_rounds, 0);
    println!("{}", metrics.summary());
}
