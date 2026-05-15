//! Anti-entropy sync engine.
//! Provides pairwise, bidirectional, replay-safe, idempotent synchronization.
//!
//! Sync invariants:
//! - Replay-safe: sync(A,B) twice produces same result as once
//! - Order-independent: sync(A,B) then sync(B,C) == sync(B,C) then sync(A,B)
//! - Convergent: repeated pairwise syncs reach quiescence

pub mod delta;
pub mod session;

pub use delta::{apply_delta, extract_delta};
pub use session::sync_peers;

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{Cell, Row, Tombstone, Value, Version};
    use replication::ReplicaState;

    fn text_row(id: &str, col: &str, val: &str, counter: u64, peer: &str) -> Row {
        let mut row = Row::new(id);
        row.cells.insert(
            col.to_string(),
            Cell::new(
                Value::Text(val.to_string()),
                Version::new(counter, peer.to_string()),
            ),
        );
        row
    }

    fn setup_peer(peer_id: &str) -> ReplicaState {
        let mut p = ReplicaState::new(peer_id);
        p.storage.create_table("users");
        p.storage.create_table("orders");
        p
    }

    #[test]
    fn basic_sync_propagates_rows() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        let row = text_row("u1", "name", "Alice", 1, "A");
        a.clock.tick();
        a.storage.upsert_row("users", row).unwrap();
        core::utils::frontier_update(&mut a.frontier, "A", a.clock.counter);

        sync_peers(&mut a, &mut b).unwrap();

        let got = b.storage.get_visible_row("users", "u1").unwrap();
        assert_eq!(got.cells["name"].value, Value::Text("Alice".to_string()));
    }

    #[test]
    fn sync_is_bidirectional() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        let row_a = text_row("u1", "name", "Alice", 1, "A");
        a.clock.tick();
        a.storage.upsert_row("users", row_a).unwrap();
        core::utils::frontier_update(&mut a.frontier, "A", a.clock.counter);

        let row_b = text_row("u2", "name", "Bob", 1, "B");
        b.clock.tick();
        b.storage.upsert_row("users", row_b).unwrap();
        core::utils::frontier_update(&mut b.frontier, "B", b.clock.counter);

        sync_peers(&mut a, &mut b).unwrap();

        // Both should have both rows
        assert!(a.storage.get_visible_row("users", "u2").is_some());
        assert!(b.storage.get_visible_row("users", "u1").is_some());
    }

    #[test]
    fn sync_is_idempotent() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        let row = text_row("u1", "name", "Alice", 1, "A");
        a.clock.tick();
        a.storage.upsert_row("users", row).unwrap();
        core::utils::frontier_update(&mut a.frontier, "A", a.clock.counter);

        sync_peers(&mut a, &mut b).unwrap();
        sync_peers(&mut a, &mut b).unwrap(); // second sync — idempotent

        assert_eq!(b.storage.visible_count("users"), 1);
    }

    #[test]
    fn sync_propagates_tombstones() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        // Insert u1 on B first
        let row = text_row("u1", "name", "Alice", 1, "B");
        b.clock.tick();
        b.storage.upsert_row("users", row).unwrap();
        core::utils::frontier_update(&mut b.frontier, "B", b.clock.counter);

        // A deletes u1 (with tombstone)
        let dv = Version::new(2, "A".to_string());
        let mut deleted_row = text_row("u1", "name", "Alice", 1, "B");
        deleted_row.deleted = true;
        deleted_row.delete_version = Some(dv.clone());
        a.clock.tick();
        a.clock.tick();
        a.storage.upsert_row("users", deleted_row).unwrap();
        a.tombstones.insert(Tombstone {
            row_id: "u1".to_string(),
            table_id: "users".to_string(),
            version: dv,
        });
        core::utils::frontier_update(&mut a.frontier, "A", a.clock.counter);

        sync_peers(&mut a, &mut b).unwrap();

        // After sync, u1 should be tombstoned on both
        let row_b = b.storage.get_row("users", "u1").unwrap();
        assert!(row_b.deleted, "tombstone must propagate");
        assert!(b.tombstones.contains("users", "u1"));
    }

    #[test]
    fn sync_cell_level_merge() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        // A and B both insert u1, A sets name, B sets email
        let mut row_a = Row::new("u1");
        row_a.cells.insert(
            "name".to_string(),
            Cell::new(
                Value::Text("Alice Cooper".to_string()),
                Version::new(10, "A".to_string()),
            ),
        );
        a.clock.counter = 10;
        a.storage.upsert_row("users", row_a).unwrap();
        core::utils::frontier_update(&mut a.frontier, "A", 10);

        let mut row_b = Row::new("u1");
        row_b.cells.insert(
            "email".to_string(),
            Cell::new(
                Value::Text("alice@ex.org".to_string()),
                Version::new(8, "B".to_string()),
            ),
        );
        b.clock.counter = 8;
        b.storage.upsert_row("users", row_b).unwrap();
        core::utils::frontier_update(&mut b.frontier, "B", 8);

        sync_peers(&mut a, &mut b).unwrap();

        // Both should have both columns
        let row = a.storage.get_visible_row("users", "u1").unwrap();
        assert!(row.cells.contains_key("name"), "name column must survive");
        assert!(row.cells.contains_key("email"), "email column must survive");
    }

    #[test]
    fn sync_order_independence() {
        // A->B->C converges same as B->C->A
        let mut a1 = setup_peer("A");
        let mut b1 = setup_peer("B");
        let mut c1 = setup_peer("C");

        let mut a2 = setup_peer("A");
        let mut b2 = setup_peer("B");
        let mut c2 = setup_peer("C");

        let setup = |a: &mut ReplicaState, b: &mut ReplicaState, c: &mut ReplicaState| {
            let row_a = text_row("u1", "name", "Alice", 3, "A");
            a.clock.counter = 3;
            a.storage.upsert_row("users", row_a).unwrap();
            core::utils::frontier_update(&mut a.frontier, "A", 3);

            let row_b = text_row("u2", "name", "Bob", 2, "B");
            b.clock.counter = 2;
            b.storage.upsert_row("users", row_b).unwrap();
            core::utils::frontier_update(&mut b.frontier, "B", 2);

            let row_c = text_row("u3", "name", "Charlie", 1, "C");
            c.clock.counter = 1;
            c.storage.upsert_row("users", row_c).unwrap();
            core::utils::frontier_update(&mut c.frontier, "C", 1);
        };

        setup(&mut a1, &mut b1, &mut c1);
        setup(&mut a2, &mut b2, &mut c2);

        // Order 1: A↔B, B↔C, A↔C
        sync_peers(&mut a1, &mut b1).unwrap();
        sync_peers(&mut b1, &mut c1).unwrap();
        sync_peers(&mut a1, &mut c1).unwrap();

        // Order 2: B↔C, A↔C, A↔B
        sync_peers(&mut b2, &mut c2).unwrap();
        sync_peers(&mut a2, &mut c2).unwrap();
        sync_peers(&mut a2, &mut b2).unwrap();

        // All should have same 3 rows
        for peer in [&a1, &b1, &c1, &a2, &b2, &c2] {
            assert_eq!(peer.storage.visible_count("users"), 3);
        }
    }

    #[test]
    fn sync_quiescence() {
        let mut a = setup_peer("A");
        let mut b = setup_peer("B");

        let row = text_row("u1", "x", "val", 1, "A");
        a.clock.tick();
        a.storage.upsert_row("users", row).unwrap();
        core::utils::frontier_update(&mut a.frontier, "A", a.clock.counter);

        sync_peers(&mut a, &mut b).unwrap();

        // After quiescence, extracting delta again should yield empty rows
        let delta = extract_delta(&a, &b.frontier);
        assert!(delta.rows.is_empty(), "no new rows after quiescence");
        assert!(delta.tombstones.is_empty());
    }
}
