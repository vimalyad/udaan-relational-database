//! Conversion between sqlparser AST values and our Value type.

use core::error::{CrdtError, CrdtResult};
use core::types::Value;
use sqlparser::ast::{self, Expr};

/// Convert a sqlparser Value to our internal Value.
pub fn from_sql_value(v: &ast::Value) -> CrdtResult<Value> {
    match v {
        ast::Value::Null => Ok(Value::Null),
        ast::Value::Number(n, _) => {
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                Ok(Value::Integer(i))
            } else {
                Err(CrdtError::ParseError(format!("cannot parse number: {s}")))
            }
        }
        ast::Value::SingleQuotedString(s) | ast::Value::DoubleQuotedString(s) => {
            Ok(Value::Text(s.clone()))
        }
        ast::Value::Boolean(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
        other => Err(CrdtError::ParseError(format!("unsupported value: {other:?}"))),
    }
}

/// Evaluate a simple literal expression (no subqueries, no functions).
pub fn eval_literal(expr: &Expr, params: &[Value]) -> CrdtResult<Value> {
    match expr {
        Expr::Value(v) => from_sql_value(v),
        Expr::Identifier(ident) => {
            // Could be a column reference — caller must resolve
            Err(CrdtError::ParseError(format!("unresolved identifier: {}", ident.value)))
        }
        Expr::UnaryOp { op, expr } => {
            let inner = eval_literal(expr, params)?;
            match op {
                ast::UnaryOperator::Minus => match inner {
                    Value::Integer(n) => Ok(Value::Integer(-n)),
                    other => Err(CrdtError::ParseError(format!("cannot negate {other}"))),
                },
                _ => Err(CrdtError::ParseError(format!("unsupported unary op: {op}"))),
            }
        }
        Expr::Nested(inner) => eval_literal(inner, params),
        _ => Err(CrdtError::ParseError(format!("complex expr not supported: {expr:?}"))),
    }
}

/// Evaluate a WHERE predicate against a row's cell map.
pub fn eval_predicate(
    expr: &Expr,
    row: &core::types::Row,
    params: &[Value],
) -> CrdtResult<bool> {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            let lv = eval_row_expr(left, row, params)?;
            let rv = eval_row_expr(right, row, params)?;
            use ast::BinaryOperator::*;
            Ok(match op {
                Eq => lv == rv,
                NotEq => lv != rv,
                Lt => lv < rv,
                LtEq => lv <= rv,
                Gt => lv > rv,
                GtEq => lv >= rv,
                And => {
                    let lb = eval_predicate(left, row, params)?;
                    let rb = eval_predicate(right, row, params)?;
                    lb && rb
                }
                Or => {
                    let lb = eval_predicate(left, row, params)?;
                    let rb = eval_predicate(right, row, params)?;
                    lb || rb
                }
                _ => return Err(CrdtError::ParseError(format!("unsupported op: {op}"))),
            })
        }
        Expr::IsNull(inner) => {
            let v = eval_row_expr(inner, row, params)?;
            Ok(v == Value::Null)
        }
        Expr::IsNotNull(inner) => {
            let v = eval_row_expr(inner, row, params)?;
            Ok(v != Value::Null)
        }
        Expr::Nested(inner) => eval_predicate(inner, row, params),
        other => {
            // Try as bool expression
            let v = eval_row_expr(other, row, params)?;
            Ok(v != Value::Null && v != Value::Integer(0))
        }
    }
}

fn eval_row_expr(expr: &Expr, row: &core::types::Row, params: &[Value]) -> CrdtResult<Value> {
    match expr {
        Expr::Identifier(ident) => {
            let col = &ident.value;
            Ok(row.cells.get(col).map(|c| c.value.clone()).unwrap_or(Value::Null))
        }
        Expr::CompoundIdentifier(parts) => {
            // table.column — use the last part as column name
            let col = parts.last().map(|p| p.value.as_str()).unwrap_or("");
            Ok(row.cells.get(col).map(|c| c.value.clone()).unwrap_or(Value::Null))
        }
        Expr::Value(v) => from_sql_value(v),
        Expr::UnaryOp { op, expr } => {
            let inner = eval_row_expr(expr, row, params)?;
            match op {
                ast::UnaryOperator::Minus => match inner {
                    Value::Integer(n) => Ok(Value::Integer(-n)),
                    other => Err(CrdtError::ParseError(format!("cannot negate {other}"))),
                },
                ast::UnaryOperator::Not => {
                    let b = inner != Value::Null && inner != Value::Integer(0);
                    Ok(Value::Integer(if b { 0 } else { 1 }))
                }
                _ => Err(CrdtError::ParseError(format!("unsupported unary op: {op}"))),
            }
        }
        Expr::Nested(inner) => eval_row_expr(inner, row, params),
        Expr::BinaryOp { left, op, right } => {
            let lv = eval_row_expr(left, row, params)?;
            let rv = eval_row_expr(right, row, params)?;
            use ast::BinaryOperator::*;
            match op {
                Eq => Ok(Value::Integer(if lv == rv { 1 } else { 0 })),
                NotEq => Ok(Value::Integer(if lv != rv { 1 } else { 0 })),
                Lt => Ok(Value::Integer(if lv < rv { 1 } else { 0 })),
                LtEq => Ok(Value::Integer(if lv <= rv { 1 } else { 0 })),
                Gt => Ok(Value::Integer(if lv > rv { 1 } else { 0 })),
                GtEq => Ok(Value::Integer(if lv >= rv { 1 } else { 0 })),
                _ => Err(CrdtError::ParseError(format!("unsupported binary op in expression: {op}"))),
            }
        }
        _ => Err(CrdtError::ParseError(format!("unsupported expression: {expr:?}"))),
    }
}
