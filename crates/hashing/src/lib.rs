use blake3::Hasher;
use core::error::CrdtResult;
use core::types::{Row, TableId, Tombstone, UniquenessClaim};
use std::collections::BTreeMap;

/// Compute a deterministic BLAKE3 snapshot hash over the canonical database state.
///
/// Canonicalization rules (must match exactly across all peers):
/// 1. Tables sorted by name (BTreeMap order)
/// 2. Rows sorted by primary key (BTreeMap order)
/// 3. Columns sorted lexicographically (BTreeMap order)
/// 4. Tombstones sorted by (table_id, row_id) — version NOT included (it's internal metadata)
/// 5. Uniqueness claims sorted by (table_id, column_id, value) — only winner row_id included
///
/// KEY INVARIANT: version/clock metadata is NEVER included in the hash.
/// Only semantic content (row data, which rows are deleted, which row owns a unique value)
/// is hashed. This ensures order-invariance: the same logical operations produce the same
/// hash regardless of sync order.
pub struct SnapshotHasher;

impl SnapshotHasher {
    /// Hash visible table state: table names, row IDs, column values (no versions).
    pub fn hash_tables(tables: &BTreeMap<TableId, BTreeMap<String, Row>>) -> CrdtResult<String> {
        let mut hasher = Hasher::new();

        for (table_name, rows) in tables.iter() {
            hasher.update(table_name.as_bytes());
            hasher.update(b"\x00");

            for (row_id, row) in rows.iter() {
                if !row.is_visible() {
                    continue;
                }
                hasher.update(row_id.as_bytes());
                hasher.update(b"\x01");

                // Hash column values only (not versions — they vary with sync order)
                for (col_id, cell) in row.cells.iter() {
                    hasher.update(col_id.as_bytes());
                    hasher.update(b"\x02");
                    hasher.update(&canonical_value_bytes(&cell.value));
                    hasher.update(b"\x03");
                }
                hasher.update(b"\x04");
            }
            hasher.update(b"\x05");
        }

        Ok(hex::encode(*hasher.finalize().as_bytes()))
    }

    /// Hash tombstones: only (table_id, row_id) — NOT version.
    /// Version varies with sync order; the EXISTENCE of a tombstone is what matters.
    pub fn hash_tombstones(tombstones: &[Tombstone]) -> String {
        let mut hasher = Hasher::new();
        let mut sorted: Vec<_> = tombstones.iter().collect();
        sorted.sort_by(|a, b| a.table_id.cmp(&b.table_id).then(a.row_id.cmp(&b.row_id)));
        for ts in sorted {
            hasher.update(ts.table_id.as_bytes());
            hasher.update(b"\x06");
            hasher.update(ts.row_id.as_bytes());
            hasher.update(b"\x07");
            // No version — it's internal CRDT metadata, not semantic content
        }
        hex::encode(*hasher.finalize().as_bytes())
    }

    /// Hash uniqueness claims: (table, column, value) -> canonical owner row ID.
    /// No version — only the semantic assignment matters.
    pub fn hash_uniqueness(claims: &[UniquenessClaim]) -> String {
        let mut hasher = Hasher::new();
        let mut sorted: Vec<_> = claims.iter().collect();
        sorted.sort_by(|a, b| {
            a.table_id
                .cmp(&b.table_id)
                .then(a.column_id.cmp(&b.column_id))
                .then(a.value.cmp(&b.value))
        });
        for claim in sorted {
            hasher.update(claim.table_id.as_bytes());
            hasher.update(b"\x08");
            hasher.update(claim.column_id.as_bytes());
            hasher.update(b"\x09");
            hasher.update(claim.value.as_bytes());
            hasher.update(b"\x0A");
            hasher.update(claim.owner_row.as_bytes());
            hasher.update(b"\x0B");
        }
        hex::encode(*hasher.finalize().as_bytes())
    }

    /// Full snapshot hash: combines table data + tombstone existence + uniqueness ownership.
    /// Order-invariant: same logical state → same hash regardless of sync order.
    pub fn full_hash(
        tables: &BTreeMap<TableId, BTreeMap<String, Row>>,
        tombstones: &[Tombstone],
        claims: &[UniquenessClaim],
    ) -> CrdtResult<String> {
        let mut hasher = Hasher::new();

        let table_hash = Self::hash_tables(tables)?;
        hasher.update(table_hash.as_bytes());

        let ts_hash = Self::hash_tombstones(tombstones);
        hasher.update(ts_hash.as_bytes());

        let uniq_hash = Self::hash_uniqueness(claims);
        hasher.update(uniq_hash.as_bytes());

        Ok(hex::encode(*hasher.finalize().as_bytes()))
    }
}

fn canonical_value_bytes(value: &core::types::Value) -> Vec<u8> {
    use core::types::Value;
    match value {
        Value::Null => b"N".to_vec(),
        Value::Integer(n) => format!("I{n}").into_bytes(),
        Value::Text(s) => {
            let mut v = b"T".to_vec();
            v.extend_from_slice(s.as_bytes());
            v
        }
        Value::Blob(b) => {
            let mut v = b"B".to_vec();
            v.extend_from_slice(b);
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{Cell, Value, Version};

    #[allow(dead_code)]
    fn row_with_val(id: &str, col: &str, val: &str, counter: u64, peer: &str) -> Row {
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

    #[test]
    fn same_data_different_versions_same_hash() {
        // Same values but different version metadata → same hash (order-invariant)
        let mut tables1: BTreeMap<String, BTreeMap<String, Row>> = BTreeMap::new();
        let mut r1 = Row::new("u1");
        r1.cells.insert(
            "name".to_string(),
            Cell::new(
                Value::Text("Alice".to_string()),
                Version::new(5, "A".to_string()),
            ),
        );
        tables1
            .entry("users".to_string())
            .or_default()
            .insert("u1".to_string(), r1);

        let mut tables2: BTreeMap<String, BTreeMap<String, Row>> = BTreeMap::new();
        let mut r2 = Row::new("u1");
        r2.cells.insert(
            "name".to_string(),
            Cell::new(
                Value::Text("Alice".to_string()),
                Version::new(10, "B".to_string()),
            ),
        );
        tables2
            .entry("users".to_string())
            .or_default()
            .insert("u1".to_string(), r2);

        let h1 = SnapshotHasher::hash_tables(&tables1).unwrap();
        let h2 = SnapshotHasher::hash_tables(&tables2).unwrap();
        assert_eq!(
            h1, h2,
            "same values must hash identically regardless of version"
        );
    }

    #[test]
    fn tombstone_hash_version_independent() {
        let ts1 = vec![Tombstone {
            row_id: "u1".to_string(),
            table_id: "users".to_string(),
            version: Version::new(5, "A".to_string()),
        }];
        let ts2 = vec![Tombstone {
            row_id: "u1".to_string(),
            table_id: "users".to_string(),
            version: Version::new(99, "C".to_string()),
        }];
        assert_eq!(
            SnapshotHasher::hash_tombstones(&ts1),
            SnapshotHasher::hash_tombstones(&ts2),
            "tombstone hash must not depend on version"
        );
    }

    #[test]
    fn different_values_different_hash() {
        let mut tables1: BTreeMap<String, BTreeMap<String, Row>> = BTreeMap::new();
        let mut r1 = Row::new("u1");
        r1.cells.insert(
            "name".to_string(),
            Cell::new(
                Value::Text("Alice".to_string()),
                Version::new(1, "A".to_string()),
            ),
        );
        tables1
            .entry("users".to_string())
            .or_default()
            .insert("u1".to_string(), r1);

        let mut tables2: BTreeMap<String, BTreeMap<String, Row>> = BTreeMap::new();
        let mut r2 = Row::new("u1");
        r2.cells.insert(
            "name".to_string(),
            Cell::new(
                Value::Text("Bob".to_string()),
                Version::new(1, "A".to_string()),
            ),
        );
        tables2
            .entry("users".to_string())
            .or_default()
            .insert("u1".to_string(), r2);

        let h1 = SnapshotHasher::hash_tables(&tables1).unwrap();
        let h2 = SnapshotHasher::hash_tables(&tables2).unwrap();
        assert_ne!(h1, h2, "different values must hash differently");
    }
}
