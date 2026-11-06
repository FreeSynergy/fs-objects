//! The [`TursoStore`] type and the primitive "apply a change + record its activity" step.
//!
//! Pattern: Adapter — `TursoStore` adapts the embedded Turso engine to the `Store` traits
//! (one trait impl per sibling module). `apply` is the Command executor: a change is
//! (kind, before, after) and is written together with its log entry.

use std::sync::atomic::AtomicBool;

use tokio::sync::{Mutex, broadcast};
use turso::Connection;

use super::activities::insert_activity;
use super::objects::{remove_object, write_object};
use super::rows::now;
use super::schema::SCHEMA_VERSION;
use crate::{Activity, ActivityId, ActivityKind, ChangeSet, Error, ObjectId, Result, StoredObject};

/// Connection plus the last issued activity id (guarded by one mutex).
pub(super) struct Inner {
    pub(super) conn: Connection,
    pub(super) last_activity: Option<ActivityId>,
}

/// Embedded Turso backend.
pub struct TursoStore {
    // Keep the database handle alive for the lifetime of the connection.
    pub(super) _db: turso::Database,
    pub(super) inner: Mutex<Inner>,
    pub(super) tx: broadcast::Sender<Activity>,
    pub(super) encrypted: bool,
    pub(super) allow_sensitive_unencrypted: bool,
    pub(super) fail_before_activity: AtomicBool,
    pub(super) fail_before_extra: AtomicBool,
}

/// One state change of an object, to be written with its activity.
pub(super) struct Change {
    pub(super) kind: ActivityKind,
    pub(super) object: ObjectId,
    pub(super) target: Option<ActivityId>,
    pub(super) before: Option<StoredObject>,
    pub(super) after: Option<StoredObject>,
}

impl TursoStore {
    /// True if the file is encrypted at rest.
    pub fn is_encrypted(&self) -> bool {
        self.encrypted
    }

    /// Schema version of the open database.
    pub fn schema_version(&self) -> i64 {
        SCHEMA_VERSION
    }

    /// Send committed activities to subscribers (no receivers is fine).
    pub(super) fn publish(&self, acts: &[Activity]) {
        for a in acts {
            let _ = self.tx.send(a.clone());
        }
    }

    /// Write `before -> after` for the object plus its activity. Caller holds the lock and a
    /// transaction.
    pub(super) async fn apply(
        &self,
        inner: &mut Inner,
        actor: &str,
        ch: Change,
    ) -> Result<Activity> {
        match &ch.after {
            Some(o) => write_object(&inner.conn, o).await?,
            None => remove_object(&inner.conn, ch.object).await?,
        }
        self.check_fault()?;
        let id = ActivityId::next_after(inner.last_activity);
        let act = Activity {
            id,
            kind: ch.kind,
            object: ch.object,
            actor: actor.to_string(),
            at: now(),
            target: ch.target,
            payload: serde_json::to_value(ChangeSet::new(ch.before, ch.after))?,
            synced: false,
        };
        insert_activity(&inner.conn, &act).await?;
        inner.last_activity = Some(id);
        Ok(act)
    }

    /// Append a log-only activity row. Caller holds the lock and a transaction.
    pub(super) async fn append_log(
        &self,
        inner: &mut Inner,
        actor: &str,
        object: ObjectId,
        kind: ActivityKind,
        payload: serde_json::Value,
    ) -> Result<Activity> {
        if kind.changes_state() {
            return Err(Error::Invalid(format!(
                "{} is written by put/delete/undo/redo only",
                kind.as_str()
            )));
        }
        let id = ActivityId::next_after(inner.last_activity);
        let act = Activity {
            id,
            kind,
            object,
            actor: actor.into(),
            at: now(),
            target: None,
            payload,
            synced: false,
        };
        insert_activity(&inner.conn, &act).await?;
        inner.last_activity = Some(id);
        Ok(act)
    }
}
