//! SQL DDL schema parsing: CREATE TABLE, CREATE INDEX.

use core::error::{CrdtError, CrdtResult};
use core::types::{
    ColumnSchema, DataType, FkPolicy, ForeignKeyDef, IndexDef, TableSchema, UniqueConstraintDef,
    Value,
};
use sqlparser::ast::{
    ColumnOption, DataType as SqlType, ObjectName, ReferentialAction, Statement, TableConstraint,
};

/// Parse a CREATE TABLE statement into a TableSchema.
pub fn parse_create_table(stmt: &Statement) -> CrdtResult<TableSchema> {
    match stmt {
        Statement::CreateTable(create) => {
            let table_name = object_name_to_string(&create.name);
            let mut columns: Vec<ColumnSchema> = Vec::new();
            let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
            let mut unique_constraints: Vec<UniqueConstraintDef> = Vec::new();
            let mut table_pk: Option<Vec<String>> = None;
            // Single-column UNIQUE table constraints (applied after column parsing)
            let mut table_unique_single: Vec<String> = Vec::new();

            // Collect table-level constraints first
            for constraint in &create.constraints {
                match constraint {
                    TableConstraint::PrimaryKey {
                        columns: pk_cols, ..
                    } => {
                        table_pk = Some(pk_cols.iter().map(|c| c.value.clone()).collect());
                    }
                    TableConstraint::ForeignKey {
                        columns,
                        foreign_table,
                        referred_columns,
                        on_delete,
                        ..
                    } => {
                        let col = columns.first().ok_or_else(|| {
                            CrdtError::SchemaError(
                                "FK must reference at least one column".to_string(),
                            )
                        })?;
                        let ref_col = referred_columns.first().ok_or_else(|| {
                            CrdtError::SchemaError(
                                "FK must reference at least one column".to_string(),
                            )
                        })?;
                        foreign_keys.push(ForeignKeyDef {
                            column: col.value.clone(),
                            ref_table: object_name_to_string(foreign_table),
                            ref_column: ref_col.value.clone(),
                            on_delete: parse_referential_action(on_delete),
                        });
                    }
                    TableConstraint::Unique {
                        columns: uniq_cols, ..
                    } => {
                        let col_names: Vec<String> =
                            uniq_cols.iter().map(|c| c.value.clone()).collect();
                        if col_names.len() == 1 {
                            // Single-column: mark the column's `unique` flag
                            table_unique_single.push(col_names.into_iter().next().unwrap());
                        } else if col_names.len() > 1 {
                            // Multi-column composite constraint
                            unique_constraints.push(UniqueConstraintDef { columns: col_names });
                        }
                    }
                    _ => {}
                }
            }

            // Parse columns
            for col_def in &create.columns {
                let col_name = col_def.name.value.clone();
                let data_type = parse_data_type(&col_def.data_type)?;
                let mut nullable = true;
                let mut unique = false;
                let mut primary_key = false;
                let mut default_value: Option<Value> = None;

                for opt in &col_def.options {
                    match &opt.option {
                        ColumnOption::NotNull => nullable = false,
                        ColumnOption::Null => nullable = true,
                        ColumnOption::Unique {
                            is_primary: true, ..
                        } => {
                            primary_key = true;
                            nullable = false;
                            unique = false;
                        }
                        ColumnOption::Unique {
                            is_primary: false, ..
                        } => {
                            unique = true;
                        }
                        ColumnOption::Default(expr) => {
                            if let Ok(v) = crate::values::eval_literal(expr, &[]) {
                                default_value = Some(v);
                            }
                        }
                        ColumnOption::ForeignKey {
                            foreign_table,
                            referred_columns,
                            on_delete,
                            ..
                        } => {
                            let ref_col = referred_columns
                                .first()
                                .map(|c| c.value.clone())
                                .unwrap_or_else(|| "id".to_string());
                            foreign_keys.push(ForeignKeyDef {
                                column: col_name.clone(),
                                ref_table: object_name_to_string(foreign_table),
                                ref_column: ref_col,
                                on_delete: parse_referential_action(on_delete),
                            });
                        }
                        _ => {}
                    }
                }

                // Apply table-level PK
                if let Some(ref pk_cols) = table_pk {
                    if pk_cols.contains(&col_name) {
                        primary_key = true;
                        nullable = false;
                    }
                }

                // Apply table-level single-column UNIQUE constraints
                if table_unique_single.contains(&col_name) {
                    unique = true;
                }

                columns.push(ColumnSchema {
                    name: col_name,
                    data_type,
                    nullable,
                    unique,
                    primary_key,
                    default_value,
                });
            }

            Ok(TableSchema {
                name: table_name,
                columns,
                foreign_keys,
                indexes: vec![],
                unique_constraints,
            })
        }
        _ => Err(CrdtError::SchemaError(
            "expected CREATE TABLE statement".to_string(),
        )),
    }
}

/// Parse a CREATE INDEX statement into an IndexDef.
pub fn parse_create_index(stmt: &Statement, _table_name: &str) -> CrdtResult<IndexDef> {
    match stmt {
        Statement::CreateIndex(ci) => {
            let index_name = ci
                .name
                .as_ref()
                .map(object_name_to_string)
                .unwrap_or_else(|| "unnamed_index".to_string());
            let table = object_name_to_string(&ci.table_name);
            let columns: Vec<String> = ci.columns.iter().map(|c| c.expr.to_string()).collect();
            Ok(IndexDef {
                name: index_name,
                table,
                columns,
                unique: ci.unique,
            })
        }
        _ => Err(CrdtError::SchemaError(
            "expected CREATE INDEX statement".to_string(),
        )),
    }
}

fn parse_data_type(dt: &SqlType) -> CrdtResult<DataType> {
    match dt {
        SqlType::Text
        | SqlType::Varchar(_)
        | SqlType::Char(_)
        | SqlType::CharVarying(_)
        | SqlType::CharacterVarying(_) => Ok(DataType::Text),
        SqlType::Int(_)
        | SqlType::Integer(_)
        | SqlType::BigInt(_)
        | SqlType::SmallInt(_)
        | SqlType::TinyInt(_)
        | SqlType::Boolean => Ok(DataType::Integer),
        SqlType::Blob(_) | SqlType::Binary(_) | SqlType::Varbinary(_) | SqlType::Bytea => {
            Ok(DataType::Blob)
        }
        _other => Ok(DataType::Text),
    }
}

fn parse_referential_action(action: &Option<ReferentialAction>) -> FkPolicy {
    match action {
        Some(ReferentialAction::Cascade) => FkPolicy::Cascade,
        Some(ReferentialAction::SetNull) => FkPolicy::Orphan,
        _ => FkPolicy::Tombstone, // Default: tombstone semantics
    }
}

pub fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|p| p.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
