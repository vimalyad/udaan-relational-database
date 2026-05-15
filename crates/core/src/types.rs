use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

pub type PeerId = String;
pub type RowId = String;
pub type ColumnId = String;
pub type TableId = String;

/// Lamport logical clock + deterministic peer_id tie-break.
/// Ordering: higher counter wins; on tie, higher peer_id (lexicographic) wins.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct Version {
    pub counter: u64,
    pub peer_id: PeerId,
}

impl Version {
    pub fn new(counter: u64, peer_id: impl Into<PeerId>) -> Self {
        Self { counter, peer_id: peer_id.into() }
    }

    pub fn zero(peer_id: impl Into<PeerId>) -> Self {
        Self { counter: 0, peer_id: peer_id.into() }
    }
}

/// SQL value type. Ordered: Null < Integer < Text < Blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Blob(a), Value::Blob(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        use Value::*;
        match (self, other) {
            (Null, Null) => Ordering::Equal,
            (Null, _) => Ordering::Less,
            (_, Null) => Ordering::Greater,
            (Integer(a), Integer(b)) => a.cmp(b),
            (Integer(_), _) => Ordering::Less,
            (_, Integer(_)) => Ordering::Greater,
            (Text(a), Text(b)) => a.cmp(b),
            (Text(_), _) => Ordering::Less,
            (_, Text(_)) => Ordering::Greater,
            (Blob(a), Blob(b)) => a.cmp(b),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "NULL"),
            Value::Integer(n) => write!(f, "{n}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::Blob(b) => write!(f, "<blob {} bytes>", b.len()),
        }
    }
}

/// A single cell: value + Lamport version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub value: Value,
    pub version: Version,
}

impl Cell {
    pub fn new(value: Value, version: Version) -> Self {
        Self { value, version }
    }
}

/// A full row with cell-level CRDT state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub id: RowId,
    pub cells: BTreeMap<ColumnId, Cell>,
    pub deleted: bool,
    /// Version of the delete operation (if deleted=true).
    pub delete_version: Option<Version>,
}

impl Row {
    pub fn new(id: impl Into<RowId>) -> Self {
        Self {
            id: id.into(),
            cells: BTreeMap::new(),
            deleted: false,
            delete_version: None,
        }
    }

    pub fn get(&self, col: &str) -> Option<&Value> {
        self.cells.get(col).map(|c| &c.value)
    }

    pub fn is_visible(&self) -> bool {
        !self.deleted
    }
}

/// Per-peer Lamport frontier: tracks the highest counter seen from each peer.
pub type Frontier = BTreeMap<PeerId, u64>;

/// Tombstone record for a deleted row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub row_id: RowId,
    pub table_id: TableId,
    pub version: Version,
}

/// Uniqueness claim: one row's ownership of a unique value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniquenessClaim {
    pub table_id: TableId,
    pub column_id: ColumnId,
    pub value: String,
    pub owner_row: RowId,
    pub version: Version,
    /// Rows that lost the uniqueness race — preserved for recoverability.
    pub losers: Vec<LooserEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LooserEntry {
    pub row_id: RowId,
    pub version: Version,
}

/// Schema for a column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: ColumnId,
    pub data_type: DataType,
    pub nullable: bool,
    pub unique: bool,
    pub primary_key: bool,
    pub default_value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Text,
    Integer,
    Blob,
}

/// Foreign key reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyDef {
    pub column: ColumnId,
    pub ref_table: TableId,
    pub ref_column: ColumnId,
    pub on_delete: FkPolicy,
}

/// FK conflict policy — declared once, applied uniformly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FkPolicy {
    /// Child survives; parent preserved as tombstone.
    Tombstone,
    /// Child cascades with parent deletion.
    Cascade,
    /// Child survives with FK column set to NULL.
    Orphan,
}

/// Table schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: TableId,
    pub columns: Vec<ColumnSchema>,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub indexes: Vec<IndexDef>,
}

impl TableSchema {
    pub fn primary_key_column(&self) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.primary_key)
    }

    pub fn unique_columns(&self) -> Vec<&ColumnSchema> {
        self.columns.iter().filter(|c| c.unique && !c.primary_key).collect()
    }

    pub fn column(&self, name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// Secondary index definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDef {
    pub name: String,
    pub table: TableId,
    pub columns: Vec<ColumnId>,
    pub unique: bool,
}

/// A delta payload used during sync — the unit of exchange between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDelta {
    pub source_peer: PeerId,
    pub rows: Vec<RowDelta>,
    pub tombstones: Vec<Tombstone>,
    pub uniqueness_claims: Vec<UniquenessClaim>,
    pub frontier: Frontier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowDelta {
    pub table_id: TableId,
    pub row: Row,
}
