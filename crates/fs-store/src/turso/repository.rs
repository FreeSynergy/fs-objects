//! [`ObjectRepository`] for the Turso backend: put, put-and-log, get, delete.
//!
//! Pattern: Repository + Command — each write builds a `Change` (kind, before, after) that
//! `TursoStore::apply` writes together with its activity inside one transaction.

use async_trait::async_trait;
use chrono::SubsecRound;

use super::TursoStore;
use super::activities::last_activity_id;
use super::objects::load_object;
use super::rows::now;
use super::store::Change;
use super::tx::in_tx;
use super::validate::validate;
use crate::{
    Activity, ActivityKind, Error, LogEntry, ObjectId, ObjectRepository, Result, StoredObject,
};

/// Decide `Create`/`Update` for `object` given the stored `before`, and normalise the new image
/// (store-owned timestamps, immutable original).
fn plan_put(
    object: StoredObject,
    before: Option<&StoredObject>,
) -> Result<(ActivityKind, StoredObject)> {
    let id = object.envelope.id;
    let mut after = object;
    let t = now();
    after.envelope.published = after.envelope.published.trunc_subsecs(6);
    after.envelope.updated = t;
    after.envelope.start_time = after.envelope.start_time.map(|t| t.trunc_subsecs(6));
    after.envelope.end_time = after.envelope.end_time.map(|t| t.trunc_subsecs(6));
    let kind = match before {
        None => ActivityKind::Create,
        Some(b) => {
            after.envelope.published = b.envelope.published;
            after.envelope.updated = t.max(b.envelope.updated);
            match (&b.original, &after.original) {
                (Some(_), None) => after.original.clone_from(&b.original),
                (Some(x), Some(y)) if x != y => return Err(Error::OriginalImmutable(id)),
                (None, Some(_)) => return Err(Error::OriginalImmutable(id)),
                _ => {}
            }
            ActivityKind::Update
        }
    };
    Ok((kind, after))
}

#[async_trait]
impl ObjectRepository for TursoStore {
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity> {
        let mut acts = self.put_and_log(object, actor, vec![]).await?;
        Ok(acts.remove(0))
    }

    async fn put_and_log(
        &self,
        object: StoredObject,
        actor: &str,
        extra: Vec<LogEntry>,
    ) -> Result<Vec<Activity>> {
        validate(&object)?;
        if let Some(e) = extra.iter().find(|e| e.kind.changes_state()) {
            return Err(Error::Invalid(format!(
                "{} cannot be an extra entry (state-changing kinds come from put/delete/undo/redo)",
                e.kind.as_str()
            )));
        }
        if !object.envelope.sensitive.is_empty()
            && !self.encrypted
            && !self.allow_sensitive_unencrypted
        {
            return Err(Error::EncryptionRequired(object.envelope.id));
        }
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let res = in_tx(&conn, async {
            let id = object.envelope.id;
            let before = load_object(&inner.conn, id).await?;
            let (kind, after) = plan_put(object, before.as_ref())?;
            let first = self
                .apply(
                    inner,
                    actor,
                    Change {
                        kind,
                        object: id,
                        target: None,
                        before,
                        after: Some(after),
                    },
                )
                .await?;
            let mut acts = vec![first];
            self.check_extra_fault()?;
            for e in extra {
                let act = self.append_log(inner, actor, id, e.kind, e.payload).await?;
                acts.push(act);
            }
            Ok(acts)
        })
        .await;
        match res {
            Ok(acts) => {
                self.publish(&acts);
                Ok(acts)
            }
            Err(e) => {
                // last_activity may have advanced inside the rolled-back transaction: re-read it.
                if let Ok(last) = last_activity_id(&inner.conn).await {
                    inner.last_activity = last;
                }
                Err(e)
            }
        }
    }

    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>> {
        let inner = self.inner.lock().await;
        load_object(&inner.conn, id).await
    }

    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity> {
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let act = in_tx(&conn, async {
            let before = load_object(&inner.conn, id)
                .await?
                .ok_or(Error::NotFound(id))?;
            self.apply(
                inner,
                actor,
                Change {
                    kind: ActivityKind::Delete,
                    object: id,
                    target: None,
                    before: Some(before),
                    after: None,
                },
            )
            .await
        })
        .await?;
        self.publish(std::slice::from_ref(&act));
        Ok(act)
    }
}
