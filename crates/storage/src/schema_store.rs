use core::error::{CrdtError, CrdtResult};
use core::types::TableSchema;
use std::collections::BTreeMap;

/// In-memory schema registry.
#[derive(Debug, Clone, Default)]
pub struct SchemaStore {
    tables: BTreeMap<String, TableSchema>,
}

impl SchemaStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_table(&mut self, schema: TableSchema) -> CrdtResult<()> {
        if self.tables.contains_key(&schema.name) {
            return Err(CrdtError::SchemaError(format!(
                "table {} already exists",
                schema.name
            )));
        }
        self.tables.insert(schema.name.clone(), schema);
        Ok(())
    }

    pub fn get(&self, table_name: &str) -> Option<&TableSchema> {
        self.tables.get(table_name)
    }

    pub fn all(&self) -> impl Iterator<Item = &TableSchema> {
        self.tables.values()
    }

    pub fn table_names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }

    pub fn contains(&self, table_name: &str) -> bool {
        self.tables.contains_key(table_name)
    }
}
