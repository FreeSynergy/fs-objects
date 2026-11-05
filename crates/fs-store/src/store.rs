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
    /// Case-insensitive, Unicode-normalised name order (see [`crate::sort_key`]); ties by id.
    NameAsc,
    NameDesc,
    /// By envelope `start_time`; objects without one come last.
    StartAsc,
    /// By envelope `start_time`, latest first; objects without one come last.
    StartDesc,
}

/// Comparison of a [`Cond`].
///
/// * `Eq`, `Gt`, `Ge`, `Lt`, `Le`: for index fields compared as **binary text** on the stored value
///   (ISO-8601 UTC strings therefore order chronologically); for time fields as instants;
///   for the name on the normalised sort key.
/// * `Prefix`, `Contains`: **case-insensitive and Unicode-normalised** (both sides go through
///   [`crate::sort_key`]), implemented with `LIKE ... ESCAPE '\'`, so `%`, `_` and `\` in the
///   needle are literals. Not available for time fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    Prefix,
    Contains,
}

/// Envelope time field usable in a [`Cond::Time`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeField {
    Start,
    End,
    Published,
    Updated,
}

/// One filter condition.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// Some value of the index field `key` satisfies `op value`.
    Index { key: String, op: Op, value: String },
    /// The envelope name (compared on its normalised sort key).
    Name { op: Op, value: String },
    /// Envelope `describes` (exact for `Eq`, otherwise as `Op` documents).
    Describes { op: Op, value: String },
    /// An envelope timestamp. Objects with no value for the field never match.
    Time {
        field: TimeField,
        op: Op,
        value: Timestamp,
    },
}

/// Object query. All set filters are AND-ed; `any` is an OR-group that is AND-ed with the rest
/// (empty = no constraint). Full-text search (tokenised) is not provided; use `Cond` with
/// `Op::Contains` / `Op::Prefix`.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub object_type: Option<String>,
    pub context: Option<String>,
    pub attributed_to: Option<String>,
    /// `(key, value)` pairs that must all be present in the object's index fields.
    pub index: Vec<(String, String)>,
    /// Further conditions, all of which must hold (0.2).
    pub conds: Vec<Cond>,
    /// At least one of these must hold, if non-empty (0.2); e.g. a search over name, e-mail, phone.
    pub any: Vec<Cond>,
    pub order: Order,
    /// If set, order by this index field instead of `order`: smallest normalised value of the key
    /// ascending (objects without the key last), ties by name key, then id (0.2).
    pub order_by_index: Option<String>,
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
    /// Add a condition that must hold.
    pub fn and(mut self, c: Cond) -> Self {
        self.conds.push(c);
        self
    }
    /// Add a condition to the OR-group.
    pub fn or(mut self, c: Cond) -> Self {
        self.any.push(c);
        self
    }
    /// Index field `key` has a value satisfying `op value`.
    pub fn index_op(self, key: impl Into<String>, op: Op, value: impl Into<String>) -> Self {
        self.and(Cond::Index {
            key: key.into(),
            op,
            value: value.into(),
        })
    }
    /// Index field `key` has a value in `[from, to)` (binary text order).
    pub fn index_between(
        self,
        key: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        let key = key.into();
        self.index_op(key.clone(), Op::Ge, from)
            .index_op(key, Op::Lt, to)
    }
    /// Envelope time condition.
    pub fn time(self, field: TimeField, op: Op, value: Timestamp) -> Self {
        self.and(Cond::Time { field, op, value })
    }
    /// Case-insensitive substring in the name.
    pub fn name_contains(self, v: impl Into<String>) -> Self {
        self.and(Cond::Name {
            op: Op::Contains,
            value: v.into(),
        })
    }
    /// Case-insensitive name prefix.
    pub fn name_prefix(self, v: impl Into<String>) -> Self {
        self.and(Cond::Name {
            op: Op::Prefix,
            value: v.into(),
        })
    }
    /// Order by an index field (see [`Query::order_by_index`] field).
    pub fn order_by_index(mut self, key: impl Into<String>) -> Self {
        self.order_by_index = Some(key.into());
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

/// An additional log-only activity for [`Store::put_and_log`].
#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub kind: ActivityKind,
    pub payload: serde_json::Value,
}

impl LogEntry {
    pub fn new(kind: ActivityKind, payload: serde_json::Value) -> Self {
        Self { kind, payload }
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

    /// Like [`Store::put`], and append further log-only activities (e.g. `Accept` /
    /// `TentativeAccept` / `Reject` for an invitation reply) **in the same transaction**: all or
    /// nothing. The extra entries refer to the same object and actor and follow the
    /// `Create`/`Update` entry in log order. Returns `Create`/`Update` first, then the extras.
    /// State-changing kinds are refused as extras.
    async fn put_and_log(
        &self,
        object: StoredObject,
        actor: &str,
        extra: Vec<LogEntry>,
    ) -> Result<Vec<Activity>>;

    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>>;

    /// Delete an object, writing a `Delete` activity (carrying the old image so it can be undone).
    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity>;

    async fn query(&self, query: &Query) -> Result<Vec<StoredObject>>;

    /// Number of objects matching the query's filters (limit/offset ignored).
    async fn count(&self, query: &Query) -> Result<usize>;

    /// Append a log-only activity (`Add`, `Remove`, `Offer`, `Invite`, `Accept`, `TentativeAccept`, `Reject`, `Read`, `Join`, `Leave`, `Like`, `Move`).
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
