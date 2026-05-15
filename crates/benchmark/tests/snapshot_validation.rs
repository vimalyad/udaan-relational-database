//! 12.3 Snapshot Validation — Order-Invariance Property
//!
//! Replays the same set of operations in different orders across peers and
//! verifies deterministic convergence: the same snapshot hash regardless of
//! the sync order used to propagate state.
//!
//! This directly tests the CRDT order-independence invariant:
//!   sync(A↔B, B↔C, A↔C) == sync(C↔B, A↔B, C↔A) == sync(A↔C, C↔B, A↔B)

use core::types::Row;
use hashing::SnapshotHasher;
use index::IndexManager;
use replication::ReplicaState;
use sql::SqlExecutor;
use std::collections::BTreeMap;
use sync::session::sync_peers;

fn setup_peer(peer_id: &str) -> (ReplicaState, IndexManager) {
    let executor = SqlExecutor::new();
    let mut replica = ReplicaState::new(peer_id);
    let mut indexes = IndexManager::new();
    executor
        .execute(
            &mut replica,
            &mut indexes,
            "CREATE TABLE events (id TEXT PRIMARY KEY, kind TEXT, val INTEGER)",
            &[],
        )
        .unwrap();
    (replica, indexes)
}

fn exec_ops(executor: &SqlExecutor, peer_id: &str, replica: &mut ReplicaState, indexes: &mut IndexManager) {
    // Each peer gets its own writes, uniquely identified by peer prefix
    match peer_id {
        "A" => {
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e1', 'click', 10)", &[]).unwrap();
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e2', 'view', 20)", &[]).unwrap();
            executor.execute(replica, indexes,
                "UPDATE events SET val = 15 WHERE id = 'e1'", &[]).unwrap();
        }
        "B" => {
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e3', 'scroll', 5)", &[]).unwrap();
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e4', 'click', 30)", &[]).unwrap();
            executor.execute(replica, indexes,
                "DELETE FROM events WHERE id = 'e3'", &[]).unwrap();
        }
        "C" => {
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e5', 'hover', 1)", &[]).unwrap();
            executor.execute(replica, indexes,
                "INSERT INTO events (id, kind, val) VALUES ('e6', 'click', 50)", &[]).unwrap();
            executor.execute(replica, indexes,
                "UPDATE events SET kind = 'tap', val = 55 WHERE id = 'e6'", &[]).unwrap();
        }
        _ => panic!("unexpected peer_id: {}", peer_id),
    }
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

/// Build a fresh 3-peer cluster and run all writes
fn build_cluster() -> Vec<(ReplicaState, IndexManager)> {
    let peer_ids = ["A", "B", "C"];
    let ex = SqlExecutor::new();
    peer_ids
        .iter()
        .map(|id| {
            let (mut r, mut idx) = setup_peer(id);
            exec_ops(&ex, id, &mut r, &mut idx);
            (r, idx)
        })
        .collect()
}

#[test]
fn same_ops_different_sync_orders_converge() {
    // Order 1: A↔B, B↔C, A↔C (then repeat to ensure convergence)
    let mut c1 = build_cluster();
    // Use split_at_mut to get pairs
    {
        let (left, right) = c1.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔B
    }
    {
        let (_, right) = c1.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap(); // B↔C
    }
    {
        let (left, right) = c1.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔C
    }
    // Second pass to quiesce fully
    {
        let (left, right) = c1.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }
    {
        let (_, right) = c1.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap();
    }
    {
        let (left, right) = c1.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }

    let hashes1: Vec<String> = c1.iter().map(|(r, _)| snapshot_hash(r)).collect();

    // Order 2: C↔B, A↔C, A↔B (then repeat)
    let mut c2 = build_cluster();
    {
        let (_, right) = c2.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap(); // B↔C
    }
    {
        let (left, right) = c2.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔C
    }
    {
        let (left, right) = c2.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔B
    }
    // Second pass
    {
        let (_, right) = c2.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap();
    }
    {
        let (left, right) = c2.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }
    {
        let (left, right) = c2.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }

    let hashes2: Vec<String> = c2.iter().map(|(r, _)| snapshot_hash(r)).collect();

    // Order 3: A↔C, B↔C, A↔B (then repeat)
    let mut c3 = build_cluster();
    {
        let (left, right) = c3.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔C
    }
    {
        let (_, right) = c3.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap(); // B↔C
    }
    {
        let (left, right) = c3.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap(); // A↔B
    }
    // Second pass
    {
        let (left, right) = c3.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }
    {
        let (_, right) = c3.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap();
    }
    {
        let (left, right) = c3.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }

    let hashes3: Vec<String> = c3.iter().map(|(r, _)| snapshot_hash(r)).collect();

    // All clusters' peer-0 (A) hashes must agree across orderings
    assert_eq!(
        hashes1[0], hashes2[0],
        "order-1 A hash {} != order-2 A hash {}",
        hashes1[0], hashes2[0]
    );
    assert_eq!(
        hashes1[0], hashes3[0],
        "order-1 A hash {} != order-3 A hash {}",
        hashes1[0], hashes3[0]
    );

    // Within each cluster, all peers agree
    for (cluster_name, hashes) in [("order1", &hashes1), ("order2", &hashes2), ("order3", &hashes3)] {
        let first = &hashes[0];
        for (i, h) in hashes.iter().enumerate() {
            assert_eq!(
                h, first,
                "{} cluster: peer {} hash {} != peer 0 hash {}",
                cluster_name, i, h, first
            );
        }
    }

    let metrics = benchmark::BenchMetrics::new(3, 6, 0);
    println!("{}", metrics.summary());
}

#[test]
fn idempotent_resync_same_hash() {
    // A single cluster synced multiple times must still have the same hash
    let mut cluster = build_cluster();

    // Sync once
    for _ in 0..3 {
        let (left, right) = cluster.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
        let (_, right) = cluster.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap();
        let (left, right) = cluster.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }

    let h1 = snapshot_hash(&cluster[0].0);

    // Sync again — must not change hash
    for _ in 0..2 {
        let (left, right) = cluster.split_at_mut(1);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
        let (_, right) = cluster.split_at_mut(1);
        let (mid, far) = right.split_at_mut(1);
        sync_peers(&mut mid[0].0, &mut far[0].0).unwrap();
        let (left, right) = cluster.split_at_mut(2);
        sync_peers(&mut left[0].0, &mut right[0].0).unwrap();
    }

    let h2 = snapshot_hash(&cluster[0].0);
    assert_eq!(h1, h2, "repeated syncs must not change the snapshot hash");
}
