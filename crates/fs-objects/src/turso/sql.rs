//! Thin query/execute helpers over a Turso connection.
//!
//! Pattern: Adapter — one place that converts Turso errors and rows to ours.

use turso::{Connection, Value};

use crate::Result;

/// Run a query and collect all rows as value vectors.
pub(super) async fn collect(
    c: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<Vec<Value>>> {
    let mut rows = c.query(sql, turso::params_from_iter(params)).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let mut r = Vec::with_capacity(row.column_count());
        for i in 0..row.column_count() {
            r.push(row.get_value(i)?);
        }
        out.push(r);
    }
    Ok(out)
}

/// Execute a statement, returning the affected row count.
pub(super) async fn exec(c: &Connection, sql: &str, params: Vec<Value>) -> Result<u64> {
    Ok(c.execute(sql, turso::params_from_iter(params)).await?)
}
