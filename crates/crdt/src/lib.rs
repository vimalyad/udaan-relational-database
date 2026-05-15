pub mod clock;
pub mod merge;
pub mod tombstone;
pub mod uniqueness;

pub use clock::LamportClock;
pub use merge::{merge_cell, merge_row, merge_table};
pub use tombstone::TombstoneStore;
pub use uniqueness::{ClaimResult, UniquenessRegistry};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use core::types::{Cell, Row, Tombstone, Value, Version};

    fn v(counter: u64, peer: &str) -> Version {
        Version::new(counter, peer.to_string())
    }

    fn cell_int(val: i64, counter: u64, peer: &str) -> Cell {
        Cell::new(Value::Integer(val), v(counter, peer))
    }

    fn cell_text(val: &str, counter: u64, peer: &str) -> Cell {
        Cell::new(Value::Text(val.to_string()), v(counter, peer))
    }

    // ---- Property tests for merge invariants ----

    #[test]
    fn merge_associative_three_peers() {
        let mut a = Row::new("r1");
        a.cells.insert("x".to_string(), cell_int(1, 1, "A"));

        let mut b = Row::new("r1");
        b.cells.insert("x".to_string(), cell_int(2, 5, "B"));

        let mut c = Row::new("r1");
        c.cells.insert("x".to_string(), cell_int(3, 3, "C"));

        let ab_c = merge_row(&merge_row(&a, &b), &c);
        let a_bc = merge_row(&a, &merge_row(&b, &c));
        assert_eq!(ab_c, a_bc);
    }

    #[test]
    fn merge_commutative_three_peers() {
        let mut a = Row::new("r1");
        a.cells.insert("name".to_string(), cell_text("Alice", 5, "A"));
        a.cells.insert("email".to_string(), cell_text("a@x.com", 3, "A"));

        let mut b = Row::new("r1");
        b.cells.insert("name".to_string(), cell_text("Bob", 2, "B"));
        b.cells.insert("email".to_string(), cell_text("b@x.com", 7, "B"));

        assert_eq!(merge_row(&a, &b), merge_row(&b, &a));
    }

    #[test]
    fn merge_idempotent() {
        let mut r = Row::new("r1");
        r.cells.insert("x".to_string(), cell_int(42, 10, "A"));
        assert_eq!(merge_row(&r, &r), r);
    }

    #[test]
    fn delete_wins_regardless_of_merge_order() {
        let mut alive = Row::new("r1");
        alive.cells.insert("x".to_string(), cell_int(99, 20, "A"));

        let mut deleted = Row::new("r1");
        deleted.deleted = true;
        deleted.delete_version = Some(v(5, "B")); // earlier clock, but delete still wins

        let m1 = merge_row(&alive, &deleted);
        let m2 = merge_row(&deleted, &alive);
        assert!(m1.deleted);
        assert!(m2.deleted);
        assert_eq!(m1, m2);
    }

    #[test]
    fn cell_level_merge_preserves_both_columns() {
        let mut a = Row::new("r1");
        a.cells.insert("name".to_string(), cell_text("Alice Cooper", 10, "A"));

        let mut b = Row::new("r1");
        b.cells.insert("email".to_string(), cell_text("alice@ex.org", 8, "B"));

        let merged = merge_row(&a, &b);
        assert_eq!(merged.cells.get("name").unwrap().value, Value::Text("Alice Cooper".to_string()));
        assert_eq!(merged.cells.get("email").unwrap().value, Value::Text("alice@ex.org".to_string()));
    }

    #[test]
    fn uniqueness_convergence_two_concurrent_claims() {
        // Simulate B inserts u3 with same email as A's u1
        let mut reg_a = UniquenessRegistry::new();
        let mut reg_b = UniquenessRegistry::new();

        reg_a.claim("users", "email", "alice@x.com", "u1", v(3, "A"));
        reg_b.claim("users", "email", "alice@x.com", "u3", v(2, "B"));

        // Merge A into B and B into A
        reg_b.merge(&reg_a);
        reg_a.merge(&reg_b);

        // Both must agree on the same canonical owner
        let owner_a = reg_a.owner("users", "email", "alice@x.com");
        let owner_b = reg_b.owner("users", "email", "alice@x.com");
        assert_eq!(owner_a, owner_b, "uniqueness must converge to same owner");

        // Loser must be preserved
        let claims_a: Vec<_> = reg_a.all_claims().collect();
        assert_eq!(claims_a.len(), 1);
        assert!(!claims_a[0].losers.is_empty(), "loser must be preserved");
    }

    #[test]
    fn lamport_clock_monotone_across_sync() {
        let mut clock_a = LamportClock::new("A");
        let mut clock_b = LamportClock::new("B");

        let v1 = clock_a.tick();
        assert_eq!(v1.counter, 1);

        // B ticks higher
        clock_b.tick();
        clock_b.tick();
        let v_b = clock_b.tick(); // counter = 3

        // A receives B's version
        clock_a.update(&v_b);
        let v2 = clock_a.tick();

        // A's next tick must be > B's last tick
        assert!(v2.counter > v_b.counter);
    }

    #[test]
    fn tombstone_merge_idempotent() {
        let ts = Tombstone {
            row_id: "r1".to_string(),
            table_id: "users".to_string(),
            version: v(5, "A"),
        };
        let mut store = TombstoneStore::new();
        store.insert(ts.clone());
        let mut other = TombstoneStore::new();
        other.insert(ts.clone());
        store.merge(&other);
        store.merge(&other); // idempotent second merge
        let count = store.all().count();
        assert_eq!(count, 1);
    }
}
