use blake3::Hasher;
use core::error::{CrdtError, CrdtResult};
use core::types::{Row, TableId, UniquenessClaim, Tombstone};
use std::collections::BTreeMap;

/// Compute a deterministic BLAKE3 snapshot hash over the canonical database state.
///
/// Canonicalization rules (must match exactly across all peers):
/// 1. Tables sorted by name (BTreeMap order)
/// 2. Rows sorted by primary key (BTreeMap order)
/// 3. Columns sorted lexicographically (BTreeMap order)
/// 4. Tombstones sorted by (table_id, row_id)
/// 5. Uniqueness claims sorted by (table_id, column_id, value)
pub struct SnapshotHasher;

impl SnapshotHasher {
    /// Hash the visible table state (excludes tombstoned rows).
    pub fn hash_tables(tables: &BTreeMap<TableId, BTreeMap<String, Row>>) -> CrdtResult<String> {
        let mut hasher = Hasher::new();

        for (table_name, rows) in tables.iter() {
            hasher.update(table_name.as_bytes());
            hasher.update(b"|table|");

            for (row_id, row) in rows.iter() {
                if !row.is_visible() {
                    continue;
                }
                hasher.update(row_id.as_bytes());
                hasher.update(b"|row|");

                for (col_id, cell) in row.cells.iter() {
                    hasher.update(col_id.as_bytes());
                    hasher.update(b"=");
                    let val_bytes = canonical_value_bytes(&cell.value);
                    hasher.update(&val_bytes);
                    hasher.update(b"|");
                }
                hasher.update(b"|endrow|");
            }
            hasher.update(b"|endtable|");
        }

        Ok(hex::encode(*hasher.finalize().as_bytes()))
    }

    /// Hash tombstones (sorted by table+row id).
    pub fn hash_tombstones(tombstones: &[Tombstone]) -> String {
        let mut hasher = Hasher::new();
        let mut sorted: Vec<_> = tombstones.iter().collect();
        sorted.sort_by(|a, b| a.table_id.cmp(&b.table_id).then(a.row_id.cmp(&b.row_id)));
        for ts in sorted {
            hasher.update(ts.table_id.as_bytes());
            hasher.update(b"|");
            hasher.update(ts.row_id.as_bytes());
            hasher.update(b"|");
            hasher.update(&ts.version.counter.to_be_bytes());
            hasher.update(ts.version.peer_id.as_bytes());
            hasher.update(b"|");
        }
        hex::encode(*hasher.finalize().as_bytes())
    }

    /// Full snapshot hash combining tables + tombstones + uniqueness claims.
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

        // Hash uniqueness claims (sorted canonically)
        let mut sorted_claims: Vec<_> = claims.iter().collect();
        sorted_claims.sort_by(|a, b| {
            a.table_id.cmp(&b.table_id)
                .then(a.column_id.cmp(&b.column_id))
                .then(a.value.cmp(&b.value))
        });
        for claim in sorted_claims {
            hasher.update(claim.table_id.as_bytes());
            hasher.update(b"|");
            hasher.update(claim.column_id.as_bytes());
            hasher.update(b"|");
            hasher.update(claim.value.as_bytes());
            hasher.update(b"|");
            hasher.update(claim.owner_row.as_bytes());
            hasher.update(b"|");
        }

        Ok(hex::encode(*hasher.finalize().as_bytes()))
    }
}

fn canonical_value_bytes(value: &core::types::Value) -> Vec<u8> {
    use core::types::Value;
    match value {
        Value::Null => b"null".to_vec(),
        Value::Integer(n) => format!("i:{n}").into_bytes(),
        Value::Text(s) => format!("t:{s}").into_bytes(),
        Value::Blob(b) => {
            let mut v = b"b:".to_vec();
            v.extend_from_slice(b);
            v
        }
    }
}
