//! [`UnloggedObjects`] for the Turso backend.
//!
//! Pattern: Adapter — object rows are written as usual, in one transaction, with no activity row.

use async_trait::async_trait;
use turso::Value;

use super::TursoStore;
use super::objects::{load_object, remove_object, write_object};
use super::sql::collect;
use super::tx::in_tx;
use super::validate::validate;
use crate::{Error, ObjectId, Result, StoredObject, UnloggedObjects};

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
}
