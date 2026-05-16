//! Engine host: manages multiple peer replicas.
//! Implements the benchmark adapter API via in-memory CRDT engine.

use core::types::{Row, Value};
use hashing::SnapshotHasher;
use index::IndexManager;
use replication::ReplicaState;
use sql::{
    enforce_fk_cascades, enforce_uniqueness_tombstones, is_effective_unique_winner, SqlExecutor,
};
use std::collections::BTreeMap;
use sync::{apply_delta, extract_delta};

pub struct EngineHost {
    peers: BTreeMap<String, (ReplicaState, IndexManager)>,
    executor: SqlExecutor,
}

impl EngineHost {
    pub fn new() -> Self {
        Self {
            peers: BTreeMap::new(),
            executor: SqlExecutor::new(),
        }
    }

    pub fn open_peer(&mut self, peer_id: &str) {
        if !self.peers.contains_key(peer_id) {
            self.peers.insert(
                peer_id.to_string(),
                (ReplicaState::new(peer_id), IndexManager::new()),
            );
        }
    }

    pub fn apply_schema(&mut self, peer_id: &str, stmts: &[String]) -> Result<(), String> {
        let (replica, indexes) = self
            .peers
            .get_mut(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        for stmt in stmts {
            self.executor
                .execute(replica, indexes, stmt, &[])
                .map_err(|e| format!("schema error in '{stmt}': {e}"))?;
        }
        Ok(())
    }

    pub fn execute(
        &mut self,
        peer_id: &str,
        sql_stmt: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value, String> {
        let (replica, indexes) = self
            .peers
            .get_mut(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        let rust_params: Vec<Value> = params.iter().map(json_to_value).collect();

        let result = self
            .executor
            .execute(replica, indexes, sql_stmt, &rust_params)
            .map_err(|e| format!("execute error: {e}"))?;

        // Return rows as JSON array of objects
        if result.rows.is_empty() && result.columns.is_empty() {
            return Ok(serde_json::Value::Null);
        }

        let rows: Vec<serde_json::Value> = result
            .rows
            .iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (col, val) in result.columns.iter().zip(row.iter()) {
                    obj.insert(col.clone(), value_to_json(val));
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(serde_json::Value::Array(rows))
    }

    pub fn sync(&mut self, peer_a_id: &str, peer_b_id: &str) -> Result<(), String> {
        // Extract deltas without borrowing both peers mutably
        let delta_for_b = {
            let (a, _) = self
                .peers
                .get(peer_a_id)
                .ok_or_else(|| format!("peer {peer_a_id} not found"))?;
            let (b, _) = self
                .peers
                .get(peer_b_id)
                .ok_or_else(|| format!("peer {peer_b_id} not found"))?;
            extract_delta(a, &b.frontier)
        };
        let delta_for_a = {
            let (b, _) = self.peers.get(peer_b_id).unwrap();
            let (a, _) = self.peers.get(peer_a_id).unwrap();
            extract_delta(b, &a.frontier)
        };

        {
            let (b, b_idx) = self.peers.get_mut(peer_b_id).unwrap();
            apply_delta(b, &delta_for_b).map_err(|e| e.to_string())?;
            // 1. FK cascades first: propagate deletions to children before resolving uniqueness.
            //    This ensures that if a uniqueness winner is cascade-deleted, the loser promotion
            //    (step 2) sees the winner as dead and preserves the effective winner in the same
            //    sync cycle.
            enforce_fk_cascades(b);
            // 2. Tombstone uniqueness losers (auditable conflict lifecycle).
            //    Preserves effective winners when the canonical owner is already dead.
            enforce_uniqueness_tombstones(b);
            // 3. Re-run FK cascades: newly tombstoned uniqueness losers may themselves be FK
            //    parents whose children need cascading.
            enforce_fk_cascades(b);
            // Rebuild indexes after merge and integrity enforcement
            for table in b.storage.table_names() {
                let rows: Vec<Row> = b.storage.all_rows(&table).cloned().collect();
                b_idx.rebuild_table(&table, rows.iter());
            }
        }
        {
            let (a, a_idx) = self.peers.get_mut(peer_a_id).unwrap();
            apply_delta(a, &delta_for_a).map_err(|e| e.to_string())?;
            enforce_fk_cascades(a);
            enforce_uniqueness_tombstones(a);
            enforce_fk_cascades(a);
            for table in a.storage.table_names() {
                let rows: Vec<Row> = a.storage.all_rows(&table).cloned().collect();
                a_idx.rebuild_table(&table, rows.iter());
            }
        }

        Ok(())
    }

    pub fn snapshot_hash(&self, peer_id: &str) -> Result<String, String> {
        let (replica, _) = self
            .peers
            .get(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        let tables: BTreeMap<String, BTreeMap<String, core::types::Row>> = replica
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

        SnapshotHasher::full_hash(&tables, &tombstones, &claims).map_err(|e| e.to_string())
    }

    pub fn snapshot_state(&self, peer_id: &str) -> Result<serde_json::Value, String> {
        let (replica, _) = self
            .peers
            .get(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        let mut result = serde_json::Map::new();

        // Tables sorted by name (BTreeMap order guarantees this)
        for table_name in replica.storage.table_names() {
            let schema = replica.schemas.get(&table_name).cloned();

            let unique_cols: Vec<String> = schema
                .as_ref()
                .map(|s| s.unique_columns().iter().map(|c| c.name.clone()).collect())
                .unwrap_or_default();

            let composite_constraints: Vec<core::types::UniqueConstraintDef> = schema
                .as_ref()
                .map(|s| s.composite_unique_constraints().to_vec())
                .unwrap_or_default();

            // Rows sorted by PK (BTreeMap row store order guarantees this)
            let rows: Vec<serde_json::Value> = replica
                .storage
                .visible_rows(&table_name)
                .filter(|row| {
                    // Single-column uniqueness filter (all value types, including Integer)
                    for col in &unique_cols {
                        if let Some(cell) = row.cells.get(col) {
                            if cell.value != core::types::Value::Null {
                                let val_str = cell.value.to_string();
                                if !is_effective_unique_winner(
                                    &replica.uniqueness,
                                    &replica.storage,
                                    &table_name,
                                    col,
                                    &val_str,
                                    &row.id,
                                ) {
                                    return false;
                                }
                            }
                        }
                    }
                    // Composite unique constraint filter
                    for constraint in &composite_constraints {
                        if let Some(val_key) = constraint.value_key_from_cells(&row.cells) {
                            if !is_effective_unique_winner(
                                &replica.uniqueness,
                                &replica.storage,
                                &table_name,
                                &constraint.constraint_key(),
                                &val_key,
                                &row.id,
                            ) {
                                return false;
                            }
                        }
                    }
                    true
                })
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    // Columns sorted lexicographically (BTreeMap cell order)
                    for (col, cell) in &row.cells {
                        obj.insert(col.clone(), value_to_json(&cell.value));
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            result.insert(table_name, serde_json::Value::Array(rows));
        }

        Ok(serde_json::Value::Object(result))
    }

    pub fn close(&mut self) {
        self.peers.clear();
    }
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Integer(n) => serde_json::Value::Number((*n).into()),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Blob(b) => serde_json::Value::String(hex::encode(b)),
    }
}

fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Bool(b) => Value::Integer(if *b { 1 } else { 0 }),
        other => Value::Text(other.to_string()),
    }
}
