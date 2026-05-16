//! SQL execution engine: INSERT, UPDATE, DELETE, SELECT, CREATE TABLE, CREATE INDEX.
//! All writes are cell-level CRDT operations using Lamport versioning.
//! Reads operate on the local materialized replica only (no coordination).

use crate::schema::{object_name_to_string, parse_create_index, parse_create_table};
use crate::values::{eval_literal, eval_predicate, eval_row_expr};
use core::error::{CrdtError, CrdtResult};
use core::types::{Cell, FkPolicy, Row, RowId, TableId, Tombstone, Value};
use index::IndexManager;
use query::QueryResult;
use replication::ReplicaState;
use sqlparser::ast::{
    self as ast, Expr, FromTable, SelectItem, SetExpr, Statement, TableFactor, TableObject,
};
use std::collections::BTreeMap;

pub struct SqlExecutor;

impl SqlExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(
        &self,
        replica: &mut ReplicaState,
        indexes: &mut IndexManager,
        sql: &str,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        let stmts = crate::parser::parse_sql(sql)?;
        let mut result = QueryResult::empty();
        for stmt in &stmts {
            result = self.execute_stmt(replica, indexes, stmt, params)?;
        }
        Ok(result)
    }

    fn execute_stmt(
        &self,
        replica: &mut ReplicaState,
        indexes: &mut IndexManager,
        stmt: &Statement,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        match stmt {
            Statement::CreateTable(_) => {
                let schema = parse_create_table(stmt)?;
                let table_name = schema.name.clone();
                if replica.schemas.contains(&table_name) {
                    return Ok(QueryResult::empty());
                }
                for idx_def in schema.indexes.clone() {
                    indexes.create_index(idx_def);
                }
                replica.schemas.create_table(schema)?;
                replica.storage.create_table(&table_name);
                Ok(QueryResult::empty())
            }

            Statement::CreateIndex(_) => {
                let idx_def = parse_create_index(stmt, "")?;
                let table_str = idx_def.table.clone();
                indexes.create_index(idx_def.clone());
                let rows: Vec<Row> = replica.storage.all_rows(&table_str).cloned().collect();
                indexes.rebuild_table(&table_str, rows.iter());
                Ok(QueryResult::empty())
            }

            Statement::Insert(insert) => self.exec_insert(replica, indexes, insert, params),

            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => self.exec_update(
                replica,
                indexes,
                table,
                assignments,
                selection.as_ref(),
                params,
            ),

            Statement::Delete(delete) => self.exec_delete(replica, indexes, delete, params),

            Statement::Query(query) => self.exec_select(replica, query, params),

            _ => Err(CrdtError::ParseError(
                "unsupported statement type".to_string(),
            )),
        }
    }

    fn exec_insert(
        &self,
        replica: &mut ReplicaState,
        indexes: &mut IndexManager,
        insert: &ast::Insert,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        let table_name = match &insert.table {
            TableObject::TableName(name) => object_name_to_string(name),
            _ => {
                return Err(CrdtError::ParseError(
                    "INSERT: unsupported table object".to_string(),
                ))
            }
        };

        let schema = replica
            .schemas
            .get(&table_name)
            .cloned()
            .ok_or_else(|| CrdtError::TableNotFound(table_name.clone()))?;

        let col_names: Vec<String> = if insert.columns.is_empty() {
            schema.columns.iter().map(|c| c.name.clone()).collect()
        } else {
            insert.columns.iter().map(|c| c.value.clone()).collect()
        };

        let source = match &insert.source {
            Some(s) => s,
            None => return Err(CrdtError::ParseError("INSERT requires VALUES".to_string())),
        };

        let rows_to_insert = match source.body.as_ref() {
            SetExpr::Values(values) => &values.rows,
            _ => {
                return Err(CrdtError::ParseError(
                    "only VALUES supported in INSERT".to_string(),
                ))
            }
        };

        // ── VALIDATION PASS (no writes, no clock ticks) ────────────────────
        // All rows are validated before any row is written (atomicity: if any
        // row fails, none are committed).
        let mut validated: Vec<(RowId, BTreeMap<String, Value>)> = Vec::new();

        for row_vals in rows_to_insert {
            let mut cells: BTreeMap<String, Value> = BTreeMap::new();

            for (col_name, expr) in col_names.iter().zip(row_vals.iter()) {
                let val = eval_literal(expr, params)?;
                cells.insert(col_name.clone(), val);
            }

            // Apply column defaults for missing columns
            for col in &schema.columns {
                if !cells.contains_key(&col.name) {
                    if let Some(default) = &col.default_value {
                        cells.insert(col.name.clone(), default.clone());
                    }
                }
            }

            // NOT NULL checks (covers both explicit NULL and missing column)
            for col in &schema.columns {
                if !col.nullable {
                    match cells.get(&col.name) {
                        None => return Err(CrdtError::NotNullViolation(col.name.clone())),
                        Some(Value::Null) => {
                            return Err(CrdtError::NotNullViolation(col.name.clone()))
                        }
                        _ => {}
                    }
                }
            }

            // Extract primary key → row ID
            let pk_col = schema
                .primary_key_column()
                .ok_or_else(|| CrdtError::SchemaError(format!("table {table_name} has no PK")))?;
            let pk_val = cells
                .get(&pk_col.name)
                .ok_or_else(|| CrdtError::SchemaError(format!("PK {} not provided", pk_col.name)))?
                .clone();
            let row_id = pk_val.to_string();

            // Duplicate PK check (reject if a visible, non-deleted row already exists)
            if replica
                .storage
                .get_visible_row(&table_name, &row_id)
                .is_some()
            {
                return Err(CrdtError::PrimaryKeyViolation(row_id, table_name.clone()));
            }

            // FK and UNIQUE violations are resolved at merge/query time — no immediate
            // rejection here. FK enforcement at INSERT time breaks partition tolerance:
            // the referenced row may exist on a peer that hasn't synced yet.

            validated.push((row_id, cells));
        }

        // ── WRITE PASS (all rows validated; write atomically) ───────────────
        for (row_id, cells) in validated {
            let version = replica.clock.tick();
            core::utils::frontier_update(&mut replica.frontier, &replica.peer_id, version.counter);

            let mut row = Row::new(row_id.clone());
            for (col_name, val) in &cells {
                row.cells
                    .insert(col_name.clone(), Cell::new(val.clone(), version.clone()));
            }

            // Uniqueness claims — single-column constraints
            for col in schema.unique_columns() {
                if let Some(val) = cells.get(&col.name) {
                    if *val != Value::Null {
                        replica.uniqueness.claim(
                            &table_name,
                            &col.name,
                            &val.to_string(),
                            &row_id,
                            version.clone(),
                        );
                    }
                }
            }

            // Uniqueness claims — composite multi-column constraints
            for constraint in schema.composite_unique_constraints() {
                if let Some(val_key) = constraint.value_key_from_values(&cells) {
                    replica.uniqueness.claim(
                        &table_name,
                        &constraint.constraint_key(),
                        &val_key,
                        &row_id,
                        version.clone(),
                    );
                }
            }

            let old = replica.storage.get_row(&table_name, &row_id).cloned();
            replica.storage.upsert_row(&table_name, row.clone())?;
            indexes.update_row(&table_name, old.as_ref(), &row);
        }

        Ok(QueryResult::empty())
    }

    fn exec_update(
        &self,
        replica: &mut ReplicaState,
        indexes: &mut IndexManager,
        table: &ast::TableWithJoins,
        assignments: &[ast::Assignment],
        selection: Option<&Expr>,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        let table_name = match &table.relation {
            TableFactor::Table { name, .. } => object_name_to_string(name),
            _ => {
                return Err(CrdtError::ParseError(
                    "UPDATE: simple table only".to_string(),
                ))
            }
        };

        let schema = replica
            .schemas
            .get(&table_name)
            .cloned()
            .ok_or_else(|| CrdtError::TableNotFound(table_name.clone()))?;

        let matching_ids: Vec<RowId> = replica
            .storage
            .visible_rows(&table_name)
            .filter(|row| {
                selection.is_none_or(|sel| eval_predicate(sel, row, params).unwrap_or(false))
            })
            .map(|r| r.id.clone())
            .collect();

        for row_id in matching_ids {
            let old_row = match replica.storage.get_row(&table_name, &row_id).cloned() {
                Some(r) => r,
                None => continue,
            };

            // Pre-validate all assignments before ticking the clock
            let mut new_vals: Vec<(String, Value)> = Vec::new();
            for assign in assignments {
                let col_name = assign.target.to_string();
                let new_val = eval_literal(&assign.value, params)
                    .or_else(|_| eval_row_expr(&assign.value, &old_row, params))?;

                // NOT NULL check
                if let Some(col) = schema.column(&col_name) {
                    if !col.nullable && new_val == Value::Null {
                        return Err(CrdtError::NotNullViolation(col_name.clone()));
                    }
                }

                // FK and UNIQUE for UPDATE: CRDT claim protocol / tombstone policy handles
                // conflicts at merge/query time — no immediate rejection (partition-tolerant).

                new_vals.push((col_name, new_val));
            }

            let version = replica.clock.tick();
            core::utils::frontier_update(&mut replica.frontier, &replica.peer_id, version.counter);

            let mut updated = old_row.clone();

            for (col_name, new_val) in new_vals {
                if let Some(col) = schema.column(&col_name) {
                    if col.unique && new_val != Value::Null {
                        replica.uniqueness.claim(
                            &table_name,
                            &col_name,
                            &new_val.to_string(),
                            &row_id,
                            version.clone(),
                        );
                    }
                }
                updated
                    .cells
                    .insert(col_name, Cell::new(new_val, version.clone()));
            }

            // Uniqueness claims — composite constraints (re-file with updated cell state)
            for constraint in schema.composite_unique_constraints() {
                if let Some(val_key) = constraint.value_key_from_cells(&updated.cells) {
                    replica.uniqueness.claim(
                        &table_name,
                        &constraint.constraint_key(),
                        &val_key,
                        &row_id,
                        version.clone(),
                    );
                }
            }

            indexes.update_row(&table_name, Some(&old_row), &updated);
            replica.storage.upsert_row(&table_name, updated)?;
        }

        Ok(QueryResult::empty())
    }

    fn exec_delete(
        &self,
        replica: &mut ReplicaState,
        indexes: &mut IndexManager,
        delete: &ast::Delete,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        let table_name = get_delete_table_name(delete)?;

        let matching_ids: Vec<RowId> = replica
            .storage
            .visible_rows(&table_name)
            .filter(|row| {
                delete
                    .selection
                    .as_ref()
                    .is_none_or(|sel| eval_predicate(sel, row, params).unwrap_or(false))
            })
            .map(|r| r.id.clone())
            .collect();

        for row_id in matching_ids {
            let old_row = match replica.storage.get_row(&table_name, &row_id).cloned() {
                Some(r) => r,
                None => continue,
            };

            let version = replica.clock.tick();
            core::utils::frontier_update(&mut replica.frontier, &replica.peer_id, version.counter);

            let mut deleted = old_row.clone();
            deleted.deleted = true;
            deleted.delete_version = Some(version.clone());

            // Register tombstone
            replica.tombstones.insert(core::types::Tombstone {
                row_id: row_id.clone(),
                table_id: table_name.clone(),
                version: version.clone(),
            });

            indexes.update_row(&table_name, Some(&old_row), &deleted);
            replica.storage.upsert_row(&table_name, deleted)?;
        }

        // 1. FK cascades: propagate deletion to children
        enforce_fk_cascades(replica);
        // 2. Tombstone uniqueness losers; cascade-deleted winners may promote their losers
        enforce_uniqueness_tombstones(replica);
        // 3. FK cascades again: cascade from newly tombstoned uniqueness losers
        enforce_fk_cascades(replica);

        Ok(QueryResult::empty())
    }

    fn exec_select(
        &self,
        replica: &ReplicaState,
        query: &ast::Query,
        params: &[Value],
    ) -> CrdtResult<QueryResult> {
        let select = match query.body.as_ref() {
            SetExpr::Select(s) => s,
            _ => return Err(CrdtError::ParseError("only SELECT supported".to_string())),
        };

        if select.from.is_empty() {
            return Ok(QueryResult::empty());
        }

        let table_name = match &select.from[0].relation {
            TableFactor::Table { name, .. } => object_name_to_string(name),
            _ => {
                return Err(CrdtError::ParseError(
                    "FROM must be a simple table".to_string(),
                ))
            }
        };

        let schema = replica
            .schemas
            .get(&table_name)
            .ok_or_else(|| CrdtError::TableNotFound(table_name.clone()))?;

        let all_cols: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();

        let output_cols: Vec<String> = if select.projection.iter().any(|p| {
            matches!(
                p,
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
            )
        }) {
            all_cols.clone()
        } else {
            select
                .projection
                .iter()
                .map(|p| match p {
                    SelectItem::UnnamedExpr(Expr::Identifier(id)) => id.value.clone(),
                    SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) => {
                        parts.last().map(|p| p.value.clone()).unwrap_or_default()
                    }
                    SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                    _ => p.to_string(),
                })
                .collect()
        };

        // Unique columns used to filter uniqueness losers from the result set.
        let unique_cols: Vec<String> = schema
            .unique_columns()
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let composite_constraints = schema.composite_unique_constraints().to_vec();

        // Collect full rows first so ORDER BY can reference any column, even
        // ones not in the SELECT list. Also filter uniqueness losers so SELECT
        // is consistent with snapshot_state.
        let mut matched_rows: Vec<Row> = replica
            .storage
            .visible_rows(&table_name)
            .filter(|row| {
                // WHERE predicate
                if !select
                    .selection
                    .as_ref()
                    .is_none_or(|sel| eval_predicate(sel, row, params).unwrap_or(false))
                {
                    return false;
                }
                // Uniqueness loser filter — single-column constraints
                for col in &unique_cols {
                    if let Some(cell) = row.cells.get(col) {
                        if cell.value != Value::Null {
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
                // Uniqueness loser filter — composite constraints
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
            .cloned()
            .collect();

        // ORDER BY (operates on full row data before projection)
        if let Some(order_by) = &query.order_by {
            matched_rows.sort_by(|a, b| {
                for order_expr in &order_by.exprs {
                    let col_name = match &order_expr.expr {
                        Expr::Identifier(id) => id.value.as_str(),
                        Expr::CompoundIdentifier(parts) => {
                            parts.last().map(|p| p.value.as_str()).unwrap_or("")
                        }
                        _ => continue,
                    };
                    let va = a
                        .cells
                        .get(col_name)
                        .map(|c| &c.value)
                        .unwrap_or(&Value::Null);
                    let vb = b
                        .cells
                        .get(col_name)
                        .map(|c| &c.value)
                        .unwrap_or(&Value::Null);
                    let cmp = va.cmp(vb);
                    let cmp = if order_expr.asc == Some(false) {
                        cmp.reverse()
                    } else {
                        cmp
                    };
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // LIMIT
        if let Some(limit_expr) = &query.limit {
            if let Ok(Value::Integer(n)) = eval_literal(limit_expr, params) {
                if n >= 0 {
                    matched_rows.truncate(n as usize);
                }
            }
        }

        // Project output columns after ordering and limiting
        let rows: Vec<Vec<Value>> = matched_rows
            .iter()
            .map(|row| {
                output_cols
                    .iter()
                    .map(|col| {
                        row.cells
                            .get(col)
                            .map(|c| c.value.clone())
                            .unwrap_or(Value::Null)
                    })
                    .collect()
            })
            .collect();

        Ok(QueryResult::new(output_cols, rows))
    }
}

impl Default for SqlExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine whether a row is the "effective" unique winner for a given constraint key + value.
///
/// A row is the effective winner if:
/// - It is the canonical owner in the uniqueness registry, OR
/// - The canonical owner is deleted/absent AND this row is the highest-versioned surviving loser.
///
/// This handles the "winner-deleted" case: when the winner is tombstoned, the best surviving
/// loser becomes the new effective winner rather than the value disappearing entirely.
pub fn is_effective_unique_winner(
    uniqueness: &crdt::UniquenessRegistry,
    storage: &storage::StorageEngine,
    table_id: &str,
    constraint_key: &str,
    val_str: &str,
    row_id: &str,
) -> bool {
    let canonical_owner = match uniqueness.owner(table_id, constraint_key, val_str) {
        None => return true, // No claim at all → this row is the sole occupant
        Some(o) => o,
    };

    if canonical_owner == row_id {
        return true; // This row is the canonical owner
    }

    // This row is a loser. Only show it if the canonical owner is deleted/gone.
    let owner_alive = storage
        .get_row(table_id, canonical_owner)
        .map(|r| !r.deleted)
        .unwrap_or(false);

    if owner_alive {
        return false; // Owner is live → keep this loser hidden
    }

    // Owner is deleted. Among surviving losers, the highest-versioned one wins.
    let claim = match uniqueness.get_claim(table_id, constraint_key, val_str) {
        Some(c) => c,
        None => return true,
    };

    let my_version = match claim.losers.iter().find(|l| l.row_id == row_id) {
        Some(l) => &l.version,
        None => return false, // Not in losers list → shouldn't be visible
    };

    // Check whether any other alive loser has a strictly higher version
    !claim
        .losers
        .iter()
        .filter(|l| l.row_id != row_id)
        .filter(|l| {
            storage
                .get_row(table_id, &l.row_id)
                .map(|r| !r.deleted)
                .unwrap_or(false)
        })
        .any(|l| l.version > *my_version)
}

/// Orphan every uniqueness-loser row by setting its conflicting unique column(s) to NULL.
///
/// After every sync, the uniqueness registry may designate some rows as losers. Rather than
/// tombstoning them (which makes them invisible and triggers unintended FK cascades), we set the
/// unique column(s) to NULL. The row remains alive and visible in snapshot_state / SELECT,
/// satisfying data-preservation. The existing NULL guard in both filters skips the uniqueness
/// check for NULL values, so uniqueness is still enforced for non-NULL entries.
///
/// Exception: when the canonical owner is already deleted/tombstoned, the best surviving loser
/// is promoted to effective winner (via `is_effective_unique_winner`) and must NOT be orphaned —
/// it keeps its original unique column value and becomes the new visible owner.
///
/// Idempotent: rows already orphaned (unique col already NULL) are skipped.
pub fn enforce_uniqueness_tombstones(replica: &mut replication::ReplicaState) {
    let all_claims: Vec<core::types::UniquenessClaim> =
        replica.uniqueness.all_claims().cloned().collect();

    for claim in &all_claims {
        let owner_alive = replica
            .storage
            .get_row(&claim.table_id, &claim.owner_row)
            .map(|r| !r.deleted)
            .unwrap_or(false);

        for loser in &claim.losers {
            if let Some(row) = replica
                .storage
                .get_row(&claim.table_id, &loser.row_id)
                .cloned()
            {
                if !row.deleted {
                    // Preserve the effective winner when the canonical owner is dead.
                    if !owner_alive
                        && is_effective_unique_winner(
                            &replica.uniqueness,
                            &replica.storage,
                            &claim.table_id,
                            &claim.column_id,
                            &claim.value,
                            &loser.row_id,
                        )
                    {
                        continue;
                    }

                    // column_id is either a single column name or a composite key
                    // (individual column names joined by '\x1f').
                    let cols: Vec<&str> = claim.column_id.split('\x1f').collect();

                    // Skip if all conflicting columns are already NULL (idempotent).
                    let needs_update = cols.iter().any(|col| {
                        row.cells
                            .get(*col)
                            .map(|c| c.value != Value::Null)
                            .unwrap_or(false)
                    });
                    if !needs_update {
                        continue;
                    }

                    let version = replica.clock.tick();
                    core::utils::frontier_update(
                        &mut replica.frontier,
                        &replica.peer_id,
                        version.counter,
                    );
                    let mut orphaned_row = row;
                    for col in &cols {
                        if let Some(cell) = orphaned_row.cells.get_mut(*col) {
                            if cell.value != Value::Null {
                                cell.value = Value::Null;
                                cell.version = version.clone();
                            }
                        }
                    }
                    let _ = replica.storage.upsert_row(&claim.table_id, orphaned_row);
                }
            }
        }
    }
}

/// Apply FK policies (Cascade / Orphan) for all tombstoned rows.
///
/// This function scans every table for tombstoned rows and propagates the declared FK
/// policy to child tables. It loops until no further changes occur, handling multi-level
/// cascade chains (grandchildren, great-grandchildren, etc.).
///
/// Idempotent: rows already tombstoned or already NULL'd are no-ops.
pub fn enforce_fk_cascades(replica: &mut replication::ReplicaState) {
    let tables: Vec<String> = replica.schemas.table_names();

    let mut had_change = true;
    while had_change {
        had_change = false;

        for parent_table in &tables {
            // Collect tombstoned row IDs in this table along with the delete
            // version. Cascades are causal: a parent delete cascades child
            // references that are not newer than the delete, but it must not
            // erase a concurrent/newer child write discovered during merge.
            let tombstoned: BTreeMap<String, core::types::Version> = replica
                .storage
                .all_rows(parent_table)
                .filter(|r| r.deleted)
                .filter_map(|r| {
                    r.delete_version
                        .as_ref()
                        .map(|version| (r.id.clone(), version.clone()))
                })
                .collect();

            if tombstoned.is_empty() {
                continue;
            }

            // Check every other table for FK references pointing here
            for child_table in &tables {
                let child_schema = match replica.schemas.get(child_table).cloned() {
                    Some(s) => s,
                    None => continue,
                };

                for fk in child_schema
                    .foreign_keys
                    .iter()
                    .filter(|fk| &fk.ref_table == parent_table)
                {
                    let fk_col = fk.column.clone();
                    let policy = fk.on_delete.clone();

                    // Find visible child rows whose FK column value is in the tombstoned set
                    let affected: Vec<String> = replica
                        .storage
                        .visible_rows(child_table)
                        .filter(|row| {
                            let Some(cell) = row.cells.get(&fk_col) else {
                                return false;
                            };
                            let Some(delete_version) = tombstoned.get(&cell.value.to_string())
                            else {
                                return false;
                            };
                            cell.version.peer_id == delete_version.peer_id
                                && cell.version.counter <= delete_version.counter
                                || cell.version.peer_id != delete_version.peer_id
                                    && cell.version.counter < delete_version.counter
                        })
                        .map(|r| r.id.clone())
                        .collect();

                    if affected.is_empty() {
                        continue;
                    }

                    match policy {
                        FkPolicy::Cascade => {
                            let version = replica.clock.tick();
                            core::utils::frontier_update(
                                &mut replica.frontier,
                                &replica.peer_id,
                                version.counter,
                            );
                            for child_id in &affected {
                                if let Some(row) =
                                    replica.storage.get_row(child_table, child_id).cloned()
                                {
                                    let mut del = row;
                                    del.deleted = true;
                                    del.delete_version = Some(version.clone());
                                    let _ = replica.storage.upsert_row(child_table, del);
                                    replica.tombstones.insert(Tombstone {
                                        row_id: child_id.clone(),
                                        table_id: child_table.clone(),
                                        version: version.clone(),
                                    });
                                    had_change = true;
                                }
                            }
                        }
                        FkPolicy::Orphan => {
                            let version = replica.clock.tick();
                            core::utils::frontier_update(
                                &mut replica.frontier,
                                &replica.peer_id,
                                version.counter,
                            );
                            for child_id in &affected {
                                if let Some(row) =
                                    replica.storage.get_row(child_table, child_id).cloned()
                                {
                                    let mut upd = row;
                                    upd.cells.insert(
                                        fk_col.clone(),
                                        Cell::new(Value::Null, version.clone()),
                                    );
                                    let _ = replica.storage.upsert_row(child_table, upd);
                                    had_change = true;
                                }
                            }
                        }
                        FkPolicy::Tombstone => {
                            // Child survives with orphaned FK reference — no action needed
                        }
                    }
                }
            }
        }
    }
}

fn get_delete_table_name(delete: &ast::Delete) -> CrdtResult<TableId> {
    // sqlparser 0.54: Delete.from is FromTable enum
    match &delete.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
            match tables.first() {
                Some(twj) => match &twj.relation {
                    TableFactor::Table { name, .. } => Ok(object_name_to_string(name)),
                    _ => Err(CrdtError::ParseError(
                        "DELETE: complex FROM not supported".to_string(),
                    )),
                },
                None => {
                    // Fall back to tables list
                    delete
                        .tables
                        .first()
                        .map(object_name_to_string)
                        .ok_or_else(|| {
                            CrdtError::ParseError("DELETE: no table specified".to_string())
                        })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::types::{ColumnSchema, DataType, FkPolicy, ForeignKeyDef, TableSchema};
    use index::IndexManager;
    use replication::ReplicaState;

    fn setup_replica(peer_id: &str) -> (ReplicaState, IndexManager) {
        let mut replica = ReplicaState::new(peer_id);
        let indexes = IndexManager::new();

        let users_schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: DataType::Text,
                    nullable: false,
                    unique: false,
                    primary_key: true,
                    default_value: None,
                },
                ColumnSchema {
                    name: "email".into(),
                    data_type: DataType::Text,
                    nullable: false,
                    unique: true,
                    primary_key: false,
                    default_value: None,
                },
                ColumnSchema {
                    name: "name".into(),
                    data_type: DataType::Text,
                    nullable: true,
                    unique: false,
                    primary_key: false,
                    default_value: None,
                },
            ],
            foreign_keys: vec![],
            indexes: vec![],
            unique_constraints: vec![],
        };
        replica.schemas.create_table(users_schema).unwrap();
        replica.storage.create_table("users");

        let orders_schema = TableSchema {
            name: "orders".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: DataType::Text,
                    nullable: false,
                    unique: false,
                    primary_key: true,
                    default_value: None,
                },
                ColumnSchema {
                    name: "user_id".into(),
                    data_type: DataType::Text,
                    nullable: false,
                    unique: false,
                    primary_key: false,
                    default_value: None,
                },
                ColumnSchema {
                    name: "status".into(),
                    data_type: DataType::Text,
                    nullable: false,
                    unique: false,
                    primary_key: false,
                    default_value: None,
                },
                ColumnSchema {
                    name: "total_cents".into(),
                    data_type: DataType::Integer,
                    nullable: false,
                    unique: false,
                    primary_key: false,
                    default_value: Some(Value::Integer(0)),
                },
            ],
            foreign_keys: vec![ForeignKeyDef {
                column: "user_id".into(),
                ref_table: "users".into(),
                ref_column: "id".into(),
                on_delete: FkPolicy::Tombstone,
            }],
            indexes: vec![],
            unique_constraints: vec![],
        };
        replica.schemas.create_table(orders_schema).unwrap();
        replica.storage.create_table("orders");

        (replica, indexes)
    }

    fn exec(replica: &mut ReplicaState, indexes: &mut IndexManager, sql: &str) {
        SqlExecutor::new()
            .execute(replica, indexes, sql, &[])
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
    }

    fn sel(replica: &mut ReplicaState, indexes: &mut IndexManager, sql: &str) -> QueryResult {
        SqlExecutor::new()
            .execute(replica, indexes, sql, &[])
            .unwrap_or_else(|e| panic!("{sql}: {e}"))
    }

    #[test]
    fn insert_and_select_star() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')",
        );
        let result = sel(&mut r, &mut idx, "SELECT * FROM users");
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn update_one_column() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')",
        );
        exec(
            &mut r,
            &mut idx,
            "UPDATE users SET name = 'Alice Cooper' WHERE id = 'u1'",
        );
        let result = sel(&mut r, &mut idx, "SELECT name FROM users WHERE id = 'u1'");
        assert_eq!(result.rows[0][0], Value::Text("Alice Cooper".to_string()));
    }

    #[test]
    fn delete_tombstones_row() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')",
        );
        exec(&mut r, &mut idx, "DELETE FROM users WHERE id = 'u1'");
        assert_eq!(r.storage.visible_count("users"), 0);
        assert!(r.storage.get_row("users", "u1").is_some());
        assert!(r.tombstones.contains("users", "u1"));
    }

    #[test]
    fn where_filter_works() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'a@x.com', 'Alice')",
        );
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u2', 'b@x.com', 'Bob')",
        );
        let result = sel(&mut r, &mut idx, "SELECT * FROM users WHERE id = 'u2'");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], Value::Text("u2".to_string()));
    }

    #[test]
    fn order_by_name_asc() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u2', 'b@x.com', 'Bob')",
        );
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'a@x.com', 'Alice')",
        );
        let result = sel(&mut r, &mut idx, "SELECT name FROM users ORDER BY name");
        assert_eq!(result.rows[0][0], Value::Text("Alice".to_string()));
        assert_eq!(result.rows[1][0], Value::Text("Bob".to_string()));
    }

    #[test]
    fn limit() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'a@x.com', 'Alice')",
        );
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u2', 'b@x.com', 'Bob')",
        );
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u3', 'c@x.com', 'Charlie')",
        );
        let result = sel(&mut r, &mut idx, "SELECT * FROM users LIMIT 2");
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn fk_allows_tombstoned_parent() {
        // Tombstone FK policy: child insert succeeds even if parent is tombstoned
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')",
        );
        exec(&mut r, &mut idx, "DELETE FROM users WHERE id = 'u1'");
        // u1 is now tombstoned — FK check should still pass (tombstone FK policy)
        exec(&mut r, &mut idx, "INSERT INTO orders (id, user_id, status, total_cents) VALUES ('o1', 'u1', 'pending', 1200)");
        assert_eq!(r.storage.visible_count("orders"), 1);
    }

    #[test]
    fn uniqueness_claim_registered() {
        let (mut r, mut idx) = setup_replica("A");
        exec(
            &mut r,
            &mut idx,
            "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')",
        );
        assert!(r.uniqueness.is_owner("users", "email", "alice@x.com", "u1"));
    }
}
