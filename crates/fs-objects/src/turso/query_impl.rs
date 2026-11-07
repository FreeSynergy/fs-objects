//! [`ObjectQuery`] for the Turso backend.
//!
//! Pattern: Repository (read side) — runs a declarative `Query` built with the query builder.

use async_trait::async_trait;
use turso::Value;

use super::TursoStore;
use super::codec::decode_object;
use super::guard::refuse_unlogged_in;
use super::objects::attach_index;
use super::query_sql::{order_by, query_where};
use super::rows::int;
use super::schema::OBJECT_COLS;
use super::sql::collect;
use crate::{ObjectQuery, Query, Result, StoredObject};

#[async_trait]
impl ObjectQuery for TursoStore {
    async fn query(&self, q: &Query) -> Result<Vec<StoredObject>> {
        let inner = self.inner.lock().await;
        let (w, mut p) = query_where(q)?;
        let order = order_by(q, &mut p);
        let cols = OBJECT_COLS
            .split(", ")
            .map(|c| format!("o.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        p.push(Value::Integer(q.limit.map_or(i64::MAX, |l| l as i64)));
        p.push(Value::Integer(q.offset as i64));
        let rows = collect(
            &inner.conn,
            &format!("SELECT {cols} FROM objects o{w} ORDER BY {order} LIMIT ? OFFSET ?"),
            p,
        )
        .await?;
        let mut objs = rows
            .iter()
            .map(|r| decode_object(r))
            .collect::<Result<Vec<_>>>()?;
        refuse_unlogged_in(&inner.conn, &objs).await?;
        attach_index(&inner.conn, &mut objs).await?;
        Ok(objs)
    }

    async fn count(&self, q: &Query) -> Result<usize> {
        let inner = self.inner.lock().await;
        let (w, p) = query_where(q)?;
        let rows = collect(
            &inner.conn,
            &format!("SELECT COUNT(*) FROM objects o{w}"),
            p,
        )
        .await?;
        Ok(int(&rows[0][0])? as usize)
    }
}
