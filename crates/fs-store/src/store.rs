//! The backend-neutral [`Store`] trait and its parameter types.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{Activity, ActivityId, ActivityKind, ObjectId, Result, StoredObject, Timestamp};

/// Sort order of [`Query`] results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// Most recently updated first (default).
    #[default]
    UpdatedDesc,
    UpdatedAsc,
    PublishedAsc,
    PublishedDesc,
    NameAsc,
}

/// Object query. All set filters are AND-ed. Full-text search is not part of v0.1.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub object_type: Option<String>,
    pub context: Option<String>,
    pub attributed_to: Option<String>,
    /// `(key, value)` pairs that must all be present in the object's index fields.
    pub index: Vec<(String, String)>,
    pub order: Order,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn of_type(mut self, t: impl Into<String>) -> Self {
        self.object_type = Some(t.into());
        self
    }
    pub fn in_context(mut self, c: impl Into<String>) -> Self {
        self.context = Some(c.into());
        self
    }
    pub fn attributed_to(mut self, a: impl Into<String>) -> Self {
        self.attributed_to = Some(a.into());
        self
    }
    pub fn with_index(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.index.push((key.into(), value.into()));
        self
    }
    pub fn ordered(mut self, o: Order) -> Self {
        self.order = o;
        self
    }
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }
}

/// Filter for reading the activity log (ascending id order).
#[derive(Clone, Debug, Default)]
pub struct ActivityFilter {
    /// Only entries with id strictly greater than this (cursor for the hub).
    pub after: Option<ActivityId>,
    pub object: Option<ObjectId>,
    pub actor: Option<String>,
    pub unsynced_only: bool,
    pub limit: Option<usize>,
}

/// Undo-history retention (ADR 0042 point 10). `None` = unlimited in that dimension.
///
/// If both are set, an entry is eligible for compaction as soon as it violates
/// **either** limit (older than `max_age` or beyond the newest `max_steps`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retention {
    pub max_age: Option<Duration>,
    pub max_steps: Option<usize>,
}

impl Retention {
    pub const UNLIMITED: Retention = Retention {
        max_age: None,
        max_steps: None,
    };
    pub fn steps(n: usize) -> Self {
        Self {
            max_age: None,
            max_steps: Some(n),
        }
    }
    pub fn age(d: Duration) -> Self {
        Self {
            max_age: Some(d),
            max_steps: None,
        }
    }
}

/// Outcome of a compaction run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactionReport {
    /// Log entries removed.
    pub removed: usize,
    /// Entries that exceeded retention but were kept because they are not yet synced.
    pub kept_unsynced: usize,
    /// Entries remaining in the log.
    pub remaining: usize,
}

/// Undo/redo depth for one actor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndoState {
    pub undoable: usize,
    pub redoable: usize,
}

/// Size of the log, to show "history size" in UIs (ADR 0042).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogStats {
    pub entries: usize,
    pub unsynced: usize,
    /// Approximate payload bytes.
    pub payload_bytes: usize,
}

/// Object store: objects + append-only activity log, every change atomic with its log entry.
///
/// Backend-neutral: a remote JSON/HTTPS backend can implement it too (ADR 0019).
#[async_trait]
pub trait Store: Send + Sync {
    /// Create or update (decided by whether `object.envelope.id` exists).
    /// The store sets `updated` to now; `published` of an existing object is kept.
    /// Writes object + `Create`/`Update` activity in one transaction.
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity>;

    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>>;

    /// Delete an object, writing a `Delete` activity (carrying the old image so it can be undone).
    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity>;

    async fn query(&self, query: &Query) -> Result<Vec<StoredObject>>;

    /// Number of objects matching the query's filters (limit/offset ignored).
    async fn count(&self, query: &Query) -> Result<usize>;

    /// Append a log-only activity (`Add`, `Remove`, `Offer`, `Accept`, `Reject`, `Read`).
    /// State-changing kinds are produced by put/delete/undo/redo only.
    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity>;

    /// Change stream: every committed activity is sent after its commit.
    /// Slow receivers get `RecvError::Lagged` and should re-read the log.
    fn subscribe(&self) -> broadcast::Receiver<Activity>;

    async fn activities(&self, filter: &ActivityFilter) -> Result<Vec<Activity>>;

    /// Undo the last `n` undoable activities of `actor` (most recent first), atomically.
    /// Returns the new `Undo` activities. Fewer than `n` may be possible: returns what was done.
    /// Fails with `Conflict` (and changes nothing) if the object was changed since.
    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;

    /// Redo the last `n` undone activities of `actor` (an `Undo` of an `Undo`).
    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;

    async fn undo_state(&self, actor: &str) -> Result<UndoState>;

    async fn retention(&self) -> Result<Retention>;
    async fn set_retention(&self, retention: Retention) -> Result<()>;

    /// Compact against the stored retention policy at time `now`.
    /// Removes **synced** entries outside retention (current state is untouched,
    /// it lives in the objects); unsynced entries are always kept.
    async fn compact_at(&self, now: Timestamp) -> Result<CompactionReport>;

    async fn compact(&self) -> Result<CompactionReport> {
        self.compact_at(chrono::Utc::now()).await
    }

    /// "Reset undo history" (ADR 0042): drop every synced log entry.
    async fn clear_history(&self) -> Result<CompactionReport>;

    /// Mark all entries with id <= `up_to` as synced (acknowledged by the hub).
    /// Returns the number of entries newly marked.
    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize>;

    async fn log_stats(&self) -> Result<LogStats>;
}
