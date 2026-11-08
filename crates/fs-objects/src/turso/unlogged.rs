//! [`UnloggedObjects`] for the Turso backend.
//!
//! Pattern: Adapter — object rows are written as usual, in one transaction, with no activity row.

use async_trait::async_trait;
use turso::Value;

use super::TursoStore;
use super::codec::decode_object;
use super::guard::mark_unlogged;
use super::objects::{attach_index, load_object, remove_object, write_object};
use super::query_sql::{order_by, query_where};
use super::schema::OBJECT_COLS;
use super::sql::collect;
use super::tx::in_tx;
use super::validate::validate;
use crate::{Error, ObjectId, Query, Result, StoredObject, UnloggedObjects};

#[async_trait]
impl UnloggedObjects for TursoStore {
    async fn put_unlogged(&self, object: StoredObject) -> Result<()> {
        validate(&object)?;
        if !object.envelope.sensitive.is_empty()
            && !self.encrypted
            && !self.allow_sensitive_unencrypted
        {
            return Err(Error::EncryptionRequired(object.envelope.id));
        }
        let inner = self.inner.lock().await;
        let conn = inner.conn.clone();
        in_tx(&conn, async {
            let mut after = object;
            let logged = collect(
                &inner.conn,
                "SELECT 1 FROM activities WHERE object_id = ? LIMIT 1",
                vec![Value::Text(after.envelope.id.to_string())],
            )
            .await?;
            if !logged.is_empty() {
                return Err(Error::Invalid(format!(
                    "{} has logged history and cannot be written unlogged",
                    after.envelope.id
                )));
            }
            let before = load_object(&inner.conn, after.envelope.id).await?;
            if let Some(b) = before {
                match (&b.original, &after.original) {
                    (Some(_), None) => after.original.clone_from(&b.original),
                    (Some(x), Some(y)) if x != y => {
                        return Err(Error::OriginalImmutable(after.envelope.id));
                    }
                    (None, Some(_)) => return Err(Error::OriginalImmutable(after.envelope.id)),
                    _ => {}
                }
            }
            mark_unlogged(&inner.conn, after.envelope.id).await?;
            write_object(&inner.conn, &after).await
        })
        .await
    }

    async fn read_unlogged(&self, id: ObjectId) -> Result<Option<StoredObject>> {
        let inner = self.inner.lock().await;
        load_object(&inner.conn, id).await
    }

    async fn delete_unlogged(&self, id: ObjectId) -> Result<()> {
        let inner = self.inner.lock().await;
        let conn = inner.conn.clone();
        in_tx(&conn, async { remove_object(&inner.conn, id).await }).await
    }

    async fn query_unlogged(&self, q: &Query, types: &[&str]) -> Result<Vec<StoredObject>> {
        if types.is_empty() {
            return Err(Error::Invalid(
                "query_unlogged needs the object types of the calling program".into(),
            ));
        }
        let inner = self.inner.lock().await;
        let (w, mut p) = query_where(q)?;
        let marks = vec!["?"; types.len()].join(",");
        p.extend(types.iter().map(|t| Value::Text((*t).to_string())));
        let glue = if w.is_empty() { " WHERE" } else { " AND" };
        let w = format!(
            "{w}{glue} o.type IN ({marks}) AND o.id IN (SELECT object_id FROM unlogged_ids)"
        );
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
        attach_index(&inner.conn, &mut objs).await?;
        Ok(objs)
    }
}
