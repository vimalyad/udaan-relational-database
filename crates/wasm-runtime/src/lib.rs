//! WASM runtime bindings.
//! Exposes the Anvil CRDT engine to browser JavaScript via wasm-bindgen.
//!
//! Build: wasm-pack build crates/wasm-runtime --target web

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct AnvilDb {
    inner: engine::AnvilEngine,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl AnvilDb {
    #[wasm_bindgen(constructor)]
    pub fn new(peer_id: &str) -> AnvilDb {
        AnvilDb {
            inner: engine::AnvilEngine::new(peer_id),
        }
    }

    pub fn execute(&mut self, sql: &str) -> Result<JsValue, JsValue> {
        self.inner
            .execute(sql, &[])
            .map(|r| serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn snapshot_hash(&self) -> Result<String, JsValue> {
        self.inner
            .snapshot_hash()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn snapshot_state(&self) -> Result<JsValue, JsValue> {
        self.inner
            .snapshot_state()
            .map(|s| serde_wasm_bindgen::to_value(&s).unwrap_or(JsValue::NULL))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Sync with another AnvilDb instance (in-process, same JS context).
    pub fn sync_with(&mut self, other: &mut AnvilDb) -> Result<(), JsValue> {
        // Extract deltas and apply
        let delta_for_other =
            sync::extract_delta(&self.inner.replica, &other.inner.replica.frontier);
        let delta_for_self =
            sync::extract_delta(&other.inner.replica, &self.inner.replica.frontier);
        sync::apply_delta(&mut other.inner.replica, &delta_for_other)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        sync::apply_delta(&mut self.inner.replica, &delta_for_self)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(())
    }
}

// Native (non-WASM) module — exposes engine for testing
pub mod engine {
    use core::error::CrdtResult;
    use core::types::Value;
    use index::IndexManager;
    use query::QueryResult;
    use replication::ReplicaState;
    use sql::SqlExecutor;

    pub struct AnvilEngine {
        pub replica: ReplicaState,
        pub indexes: IndexManager,
        executor: SqlExecutor,
    }

    impl AnvilEngine {
        pub fn new(peer_id: &str) -> Self {
            Self {
                replica: ReplicaState::new(peer_id),
                indexes: IndexManager::new(),
                executor: SqlExecutor::new(),
            }
        }

        pub fn execute(&mut self, sql: &str, params: &[Value]) -> CrdtResult<QueryResult> {
            self.executor
                .execute(&mut self.replica, &mut self.indexes, sql, params)
        }

        pub fn snapshot_hash(&self) -> CrdtResult<String> {
            use hashing::SnapshotHasher;
            use std::collections::BTreeMap;
            let tables: BTreeMap<String, BTreeMap<String, core::types::Row>> = self
                .replica
                .storage
                .table_names()
                .into_iter()
                .filter_map(|name| {
                    self.replica
                        .storage
                        .snapshot_table(&name)
                        .map(|t| (name, t.clone()))
                })
                .collect();
            let tombstones: Vec<_> = self.replica.tombstones.all().cloned().collect();
            let claims: Vec<_> = self.replica.uniqueness.all_claims().cloned().collect();
            SnapshotHasher::full_hash(&tables, &tombstones, &claims)
        }

        pub fn snapshot_state(&self) -> CrdtResult<serde_json::Value> {
            let mut result = serde_json::Map::new();
            for table_name in self.replica.storage.table_names() {
                let rows: Vec<serde_json::Value> = self
                    .replica
                    .storage
                    .visible_rows(&table_name)
                    .map(|row| {
                        let mut obj = serde_json::Map::new();
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
    }

    fn value_to_json(v: &Value) -> serde_json::Value {
        match v {
            Value::Null => serde_json::Value::Null,
            Value::Integer(n) => serde_json::Value::Number((*n).into()),
            Value::Text(s) => serde_json::Value::String(s.clone()),
            Value::Blob(b) => serde_json::Value::String(hex::encode(b)),
        }
    }
}
