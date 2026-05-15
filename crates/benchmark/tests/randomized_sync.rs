//! 12.1 Randomized Sync Simulation
//!
//! Creates N peers (3–5), applies random INSERTs/UPDATEs/DELETEs in random order
//! across peers, then syncs all pairs to quiescence. Verifies all peers converge
//! to the same snapshot_hash.
//!
//! Uses deterministic index cycling instead of the `rand` crate.

use core::types::Row;
use hashing::SnapshotHasher;
use index::IndexManager;
use replication::ReplicaState;
use sql::SqlExecutor;
use std::collections::BTreeMap;
use sync::session::{sync_peers, sync_to_quiescence};

/// Minimal deterministic sequence generator — no external crates.
struct DeterministicSeq {
    state: usize,
}

impl DeterministicSeq {
    fn new(seed: usize) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> usize {
        // Xorshift-style mix
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

fn setup_peer(peer_id: &str) -> (ReplicaState, IndexManager) {
    let executor = SqlExecutor::new();
    let mut replica = ReplicaState::new(peer_id);
    let mut indexes = IndexManager::new();
    executor
        .execute(
            &mut replica,
            &mut indexes,
            "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT, qty INTEGER)",
            &[],
        )
        .unwrap();
    (replica, indexes)
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
fn randomized_sync_converges() {
    const N_PEERS: usize = 4;
    const N_OPS: usize = 24;

    let peer_ids = ["P0", "P1", "P2", "P3"];
    let mut peers: Vec<(ReplicaState, IndexManager)> = peer_ids[..N_PEERS]
        .iter()
        .map(|id| setup_peer(id))
        .collect();

    let executor = SqlExecutor::new();
    let mut seq = DeterministicSeq::new(0xDEAD_BEEF);

    let names = ["alpha", "beta", "gamma", "delta"];

    // Apply random ops across peers
    for _ in 0..N_OPS {
        let peer_idx = seq.next() % N_PEERS;
        let op_type = seq.next() % 3;
        let row_id = format!("r{}", seq.next() % 8);
        let name = names[seq.next() % names.len()];
        let qty = (seq.next() % 100) as i64;

        let (replica, indexes) = &mut peers[peer_idx];

        match op_type {
            0 => {
                // INSERT — ignore duplicate PK errors (CRDT: last-write wins anyway)
                let _ = executor.execute(
                    replica,
                    indexes,
                    &format!(
                        "INSERT INTO items (id, name, qty) VALUES ('{}', '{}', {})",
                        row_id, name, qty
                    ),
                    &[],
                );
            }
            1 => {
                // UPDATE
                let _ = executor.execute(
                    replica,
                    indexes,
                    &format!(
                        "UPDATE items SET name = '{}', qty = {} WHERE id = '{}'",
                        name, qty, row_id
                    ),
                    &[],
                );
            }
            2 => {
                // DELETE
                let _ = executor.execute(
                    replica,
                    indexes,
                    &format!("DELETE FROM items WHERE id = '{}'", row_id),
                    &[],
                );
            }
            _ => unreachable!(),
        }
    }

    // Sync all pairs to quiescence
    let mut total_rounds = 0usize;
    for i in 0..N_PEERS {
        for j in (i + 1)..N_PEERS {
            let (left, right) = peers.split_at_mut(j);
            let (a, _) = &mut left[i];
            let (b, _) = &mut right[0];
            let rounds = sync_to_quiescence(a, b).unwrap();
            total_rounds += rounds;
        }
    }

    // One extra round ensures full N-way convergence
    for i in 0..N_PEERS {
        for j in (i + 1)..N_PEERS {
            let (left, right) = peers.split_at_mut(j);
            let (a, _) = &mut left[i];
            let (b, _) = &mut right[0];
            sync_peers(a, b).unwrap();
        }
    }

    // All peers must have the same snapshot hash
    let hashes: Vec<String> = peers.iter().map(|(r, _)| snapshot_hash(r)).collect();
    let first = &hashes[0];
    for (i, h) in hashes.iter().enumerate() {
        assert_eq!(h, first, "peer {} hash {} != peer 0 hash {}", i, h, first);
    }

    let metrics = benchmark::BenchMetrics::new(N_PEERS, total_rounds, 0);
    println!("{}", metrics.summary());
}
