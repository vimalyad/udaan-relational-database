use core::types::{Cell, ColumnId, Row, RowId, TableId, Value, Version};
use std::collections::BTreeMap;

/// Merge two cells: the cell with the higher Version wins (LWW per cell).
/// Satisfies associativity, commutativity, idempotency.
pub fn merge_cell(a: &Cell, b: &Cell) -> Cell {
    if b.version > a.version {
        b.clone()
    } else {
        a.clone()
    }
}

/// Merge two rows: cell-level merge + delete-wins tombstone semantics.
/// If either row is deleted, the merged row is deleted (delete wins).
/// The delete_version is the max of both delete versions.
pub fn merge_row(a: &Row, b: &Row) -> Row {
    debug_assert_eq!(a.id, b.id, "cannot merge rows with different IDs");

    let mut merged_cells: BTreeMap<ColumnId, Cell> = BTreeMap::new();

    // Merge cells present in a
    for (col, cell_a) in &a.cells {
        let merged = if let Some(cell_b) = b.cells.get(col) {
            merge_cell(cell_a, cell_b)
        } else {
            cell_a.clone()
        };
        merged_cells.insert(col.clone(), merged);
    }

    // Add cells only in b
    for (col, cell_b) in &b.cells {
        if !merged_cells.contains_key(col) {
            merged_cells.insert(col.clone(), cell_b.clone());
        }
    }

    // Delete-wins: if either is deleted, result is deleted.
    // The delete_version tracks which delete event applies.
    let (deleted, delete_version) = match (&a.deleted, &b.deleted, &a.delete_version, &b.delete_version) {
        (true, true, Some(va), Some(vb)) => (true, Some(if vb > va { vb.clone() } else { va.clone() })),
        (true, _, v, _) => (true, v.clone()),
        (_, true, _, v) => (true, v.clone()),
        _ => (false, None),
    };

    Row {
        id: a.id.clone(),
        cells: merged_cells,
        deleted,
        delete_version,
    }
}

/// Merge two table states (BTreeMap<RowId, Row>).
pub fn merge_table(
    a: &BTreeMap<RowId, Row>,
    b: &BTreeMap<RowId, Row>,
) -> BTreeMap<RowId, Row> {
    let mut result = a.clone();
    for (row_id, row_b) in b {
        let merged = if let Some(row_a) = result.get(row_id) {
            merge_row(row_a, row_b)
        } else {
            row_b.clone()
        };
        result.insert(row_id.clone(), merged);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::Version;

    fn cell(val: i64, counter: u64, peer: &str) -> Cell {
        Cell::new(Value::Integer(val), Version::new(counter, peer.to_string()))
    }

    #[test]
    fn cell_merge_higher_counter_wins() {
        let a = cell(1, 5, "A");
        let b = cell(2, 10, "A");
        let m = merge_cell(&a, &b);
        assert_eq!(m.value, Value::Integer(2));
    }

    #[test]
    fn cell_merge_tie_break_peer_id() {
        let a = cell(1, 5, "peerA");
        let b = cell(2, 5, "peerB");
        // peerB > peerA lexicographically
        let m = merge_cell(&a, &b);
        assert_eq!(m.value, Value::Integer(2));
    }

    #[test]
    fn cell_merge_idempotent() {
        let a = cell(1, 5, "A");
        assert_eq!(merge_cell(&a, &a), a);
    }

    #[test]
    fn cell_merge_commutative() {
        let a = cell(1, 5, "A");
        let b = cell(2, 10, "A");
        assert_eq!(merge_cell(&a, &b), merge_cell(&b, &a));
    }

    #[test]
    fn cell_merge_associative() {
        let a = cell(1, 1, "A");
        let b = cell(2, 2, "A");
        let c = cell(3, 3, "A");
        assert_eq!(merge_cell(&a, &merge_cell(&b, &c)), merge_cell(&merge_cell(&a, &b), &c));
    }

    #[test]
    fn row_merge_cell_level() {
        let mut row_a = Row::new("r1");
        row_a.cells.insert("name".to_string(), cell(10, 5, "A"));

        let mut row_b = Row::new("r1");
        row_b.cells.insert("email".to_string(), cell(20, 3, "B"));

        let merged = merge_row(&row_a, &row_b);
        assert!(merged.cells.contains_key("name"));
        assert!(merged.cells.contains_key("email"));
    }

    #[test]
    fn row_merge_delete_wins() {
        let mut row_a = Row::new("r1");
        row_a.cells.insert("name".to_string(), cell(10, 5, "A"));

        let mut row_b = Row::new("r1");
        row_b.deleted = true;
        row_b.delete_version = Some(Version::new(7, "B".to_string()));

        let merged = merge_row(&row_a, &row_b);
        assert!(merged.deleted);
        assert!(merged.cells.contains_key("name")); // updates preserved internally
    }
}
