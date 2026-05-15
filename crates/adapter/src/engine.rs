//! Engine host: manages multiple peer replicas and exposes the adapter API.

use core::error::CrdtResult;
use hashing::SnapshotHasher;
use replication::ReplicaState;
use sync::{apply_delta, extract_delta};
use std::collections::BTreeMap;

pub struct EngineHost {
    peers: BTreeMap<String, ReplicaState>,
}

impl EngineHost {
    pub fn new() -> Self {
        Self { peers: BTreeMap::new() }
    }

    pub fn open_peer(&mut self, peer_id: &str) {
        if !self.peers.contains_key(peer_id) {
            self.peers.insert(peer_id.to_string(), ReplicaState::new(peer_id));
        }
    }

    pub fn apply_schema(&mut self, peer_id: &str, stmts: &[String]) -> Result<(), String> {
        let peer = self.peers.get_mut(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        for stmt in stmts {
            sql::parser::parse_sql(stmt).map_err(|e| e.to_string())?;
            // Schema application will be fully implemented in Phase 6
            // For now, parse and register table schemas
            apply_schema_stmt(peer, stmt).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn execute(
        &mut self,
        peer_id: &str,
        sql_stmt: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value, String> {
        let peer = self.peers.get_mut(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;
        execute_sql(peer, sql_stmt, params).map_err(|e| e.to_string())
    }

    pub fn sync(&mut self, peer_a_id: &str, peer_b_id: &str) -> Result<(), String> {
        // We need two mutable references — extract and apply in two steps
        let delta_for_b = {
            let a = self.peers.get(peer_a_id)
                .ok_or_else(|| format!("peer {peer_a_id} not found"))?;
            let b = self.peers.get(peer_b_id)
                .ok_or_else(|| format!("peer {peer_b_id} not found"))?;
            extract_delta(a, &b.frontier)
        };
        let delta_for_a = {
            let b = self.peers.get(peer_b_id).unwrap();
            let a = self.peers.get(peer_a_id).unwrap();
            extract_delta(b, &a.frontier)
        };

        {
            let b = self.peers.get_mut(peer_b_id).unwrap();
            apply_delta(b, &delta_for_b).map_err(|e| e.to_string())?;
        }
        {
            let a = self.peers.get_mut(peer_a_id).unwrap();
            apply_delta(a, &delta_for_a).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub fn snapshot_hash(&self, peer_id: &str) -> Result<String, String> {
        let peer = self.peers.get(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        let tables: std::collections::BTreeMap<String, std::collections::BTreeMap<String, core::types::Row>> = peer
            .storage
            .table_names()
            .into_iter()
            .filter_map(|name| {
                peer.storage.snapshot_table(&name).map(|t| (name, t.clone()))
            })
            .collect();

        let tombstones: Vec<core::types::Tombstone> = peer.tombstones.all().cloned().collect();
        let claims: Vec<core::types::UniquenessClaim> = peer.uniqueness.all_claims().cloned().collect();

        SnapshotHasher::full_hash(&tables, &tombstones, &claims).map_err(|e| e.to_string())
    }

    pub fn snapshot_state(&self, peer_id: &str) -> Result<serde_json::Value, String> {
        let peer = self.peers.get(peer_id)
            .ok_or_else(|| format!("peer {peer_id} not found"))?;

        let mut result = serde_json::Map::new();

        for table_name in peer.storage.table_names() {
            let mut rows_out = Vec::new();
            for row in peer.storage.visible_rows(&table_name) {
                let mut row_map = serde_json::Map::new();
                for (col, cell) in &row.cells {
                    let val = value_to_json(&cell.value);
                    row_map.insert(col.clone(), val);
                }
                rows_out.push(serde_json::Value::Object(row_map));
            }
            result.insert(table_name, serde_json::Value::Array(rows_out));
        }

        Ok(serde_json::Value::Object(result))
    }

    pub fn close(&mut self) {
        self.peers.clear();
    }
}

fn value_to_json(v: &core::types::Value) -> serde_json::Value {
    match v {
        core::types::Value::Null => serde_json::Value::Null,
        core::types::Value::Integer(n) => serde_json::Value::Number((*n).into()),
        core::types::Value::Text(s) => serde_json::Value::String(s.clone()),
        core::types::Value::Blob(b) => serde_json::Value::String(hex::encode(b)),
    }
}

fn apply_schema_stmt(peer: &mut ReplicaState, stmt: &str) -> CrdtResult<()> {
    // Delegate to sql executor — full impl in Phase 6
    let _ = stmt;
    let _ = peer;
    Ok(())
}

fn execute_sql(
    peer: &mut ReplicaState,
    sql: &str,
    params: &[serde_json::Value],
) -> CrdtResult<serde_json::Value> {
    // Delegate to sql executor — full impl in Phase 6
    let _ = (peer, sql, params);
    Ok(serde_json::Value::Null)
}
