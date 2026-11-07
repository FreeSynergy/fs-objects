//! [`HistoryRetention`]: retention policy, compaction, sync marks and log statistics.
//!
//! Pattern: Strategy (policy value [`Retention`]) behind a trait.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{ActivityId, Error, ObjectId, PurgeReport, Result, Timestamp};

/// Undo-history retention (ADR 0042 point 10). `None` = unlimited in that dimension.
///
/// If both are set, an entry is eligible for compaction as soon as it violates
/// **either** limit (older than `max_age` or beyond the newest `max_steps`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retention {
    /// Entries older than this are eligible.
    pub max_age: Option<Duration>,
    /// Entries beyond the newest this-many are eligible.
    pub max_steps: Option<usize>,
}

impl Retention {
    /// Keep everything.
    pub const UNLIMITED: Retention = Retention {
        max_age: None,
        max_steps: None,
    };
    /// Keep the newest `n` steps.
    pub fn steps(n: usize) -> Self {
        Self {
            max_age: None,
            max_steps: Some(n),
        }
    }
    /// Keep entries younger than `d`.
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

/// Size of the log, to show "history size" in UIs (ADR 0042).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogStats {
    /// Entries in the log.
    pub entries: usize,
    /// Entries not yet acknowledged by the hub.
    pub unsynced: usize,
    /// Approximate payload bytes.
    pub payload_bytes: usize,
}

/// Retention, compaction and sync bookkeeping of the activity log.
#[async_trait]
pub trait HistoryRetention: Send + Sync {
    /// The stored retention policy (default: unlimited).
    async fn retention(&self) -> Result<Retention>;

    /// Store a new retention policy.
    async fn set_retention(&self, retention: Retention) -> Result<()>;

    /// Compact against the stored retention policy at time `now`.
    /// Removes **synced** entries outside retention (current state is untouched,
    /// it lives in the objects); unsynced entries are always kept.
    async fn compact_at(&self, now: Timestamp) -> Result<CompactionReport>;

    /// [`HistoryRetention::compact_at`] with the current time.
    async fn compact(&self) -> Result<CompactionReport> {
        self.compact_at(chrono::Utc::now()).await
    }

    /// "Reset undo history" (ADR 0042): drop every synced log entry.
    async fn clear_history(&self) -> Result<CompactionReport>;

    /// Mark all entries with id <= `up_to` as synced (acknowledged by the hub).
    /// Returns the number of entries newly marked.
    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize>;

    /// Remove **every** log entry of one object (ADR 0029: an object marked sensitive afterwards must not
    /// keep its old content in a log): `Create`, `Update`, `Delete`, `Undo` and log-only extras, synced or
    /// not. The object itself is not touched. Undo and redo: the entries go together, so no `Undo` is left
    /// without its target, and the other objects' undo and redo entries stay as they were. Later writes on
    /// the object are logged **without images** and are not undoable. Touches only this store; the hub's own
    /// log needs the same call. Default: unsupported, so existing implementations keep compiling.
    async fn purge_object(&self, id: ObjectId) -> Result<PurgeReport> {
        let _ = id;
        Err(Error::Backend(
            "purge_object is not supported by this store".into(),
        ))
    }

    /// Rewrite the database file and its write-ahead log so bytes of removed entries are gone from disk
    /// (what `purge_object` does after its commit). `Ok(true)` = done (or no file), `Ok(false)` = another
    /// reader holds the file; nothing is lost, try again later. Default: unsupported.
    async fn clean_files(&self) -> Result<bool> {
        Err(Error::Backend(
            "clean_files is not supported by this store".into(),
        ))
    }

    /// Log size figures.
    async fn log_stats(&self) -> Result<LogStats>;
}
