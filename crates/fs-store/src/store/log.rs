//! [`ActivityLog`]: append log-only entries and read the activity log.
//!
//! Pattern: Repository (log) + Builder-style value types ([`LogEntry`], [`ActivityFilter`]).

use async_trait::async_trait;

use crate::{Activity, ActivityId, ActivityKind, ObjectId, Result};

/// An additional log-only activity for [`ObjectRepository::put_and_log`](crate::traits::ObjectRepository::put_and_log).
#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    /// The log-only kind (not `Create`/`Update`/`Delete`/`Undo`).
    pub kind: ActivityKind,
    /// Caller-defined payload.
    pub payload: serde_json::Value,
}

impl LogEntry {
    /// A log entry of `kind` with `payload`.
    pub fn new(kind: ActivityKind, payload: serde_json::Value) -> Self {
        Self { kind, payload }
    }
}

/// Filter for reading the activity log (ascending id order).
#[derive(Clone, Debug, Default)]
pub struct ActivityFilter {
    /// Only entries with id strictly greater than this (cursor for the hub).
    pub after: Option<ActivityId>,
    /// Only entries about this object.
    pub object: Option<ObjectId>,
    /// Only entries by this actor.
    pub actor: Option<String>,
    /// Only entries not yet acknowledged by the hub.
    pub unsynced_only: bool,
    /// Maximum number of entries.
    pub limit: Option<usize>,
}

/// Append to and read the append-only activity log.
#[async_trait]
pub trait ActivityLog: Send + Sync {
    /// Append a log-only activity (`Add`, `Remove`, `Offer`, `Invite`, `Accept`, `TentativeAccept`, `Reject`, `Read`, `Join`, `Leave`, `Like`, `Move`).
    /// State-changing kinds are produced by put/delete/undo/redo only.
    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity>;

    /// Read log entries matching the filter, ascending by id.
    async fn activities(&self, filter: &ActivityFilter) -> Result<Vec<Activity>>;
}
