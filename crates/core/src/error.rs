use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrdtError {
    #[error("row not found: {0}")]
    RowNotFound(String),

    #[error("table not found: {0}")]
    TableNotFound(String),

    #[error("column not found: {0}")]
    ColumnNotFound(String),

    #[error("uniqueness violation: column {column} value {value} already claimed by row {owner}")]
    UniquenessViolation {
        column: String,
        value: String,
        owner: String,
    },

    #[error("foreign key violation: referenced row {row} in table {table} does not exist")]
    ForeignKeyViolation { table: String, row: String },

    #[error("primary key violation: row '{0}' already exists in table '{1}'")]
    PrimaryKeyViolation(String, String),

    #[error("not null violation: column '{0}' cannot be NULL")]
    NotNullViolation(String),

    #[error("schema error: {0}")]
    SchemaError(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("storage error: {0}")]
    StorageError(String),

    #[error("sync error: {0}")]
    SyncError(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type CrdtResult<T> = Result<T, CrdtError>;
