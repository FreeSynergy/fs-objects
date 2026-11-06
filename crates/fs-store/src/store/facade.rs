//! The [`Store`] facade: the whole object store as one trait.
//!
//! Pattern: Facade — `Store` re-exposes the six focused traits ([`ObjectRepository`],
//! [`ObjectQuery`], [`ActivityLog`], [`ChangeFeed`], [`UndoRedo`], [`HistoryRetention`]) under one
//! name, so dependents keep writing `Arc<dyn Store>` and `use fs_store::Store` while small
//! consumers can depend on just the trait they need. It is blanket-implemented: any type that
//! implements the six traits is a `Store`, and its methods are callable with only `Store` in scope.

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::{
    Activity, ActivityFilter, ActivityId, ActivityKind, ActivityLog, ChangeFeed, CompactionReport,
    HistoryRetention, LogEntry, LogStats, ObjectId, ObjectQuery, ObjectRepository, Query, Result,
    Retention, StoredObject, Timestamp, UndoRedo, UndoState,
};

/// Object store: objects + append-only activity log, every change atomic with its log entry.
///
/// Backend-neutral: a remote JSON/HTTPS backend can implement the focused traits too (ADR 0019).
/// See the focused traits for the documentation of each method.
#[async_trait]
pub trait Store: Send + Sync {
    /// See [`ObjectRepository::put`].
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity>;
    /// See [`ObjectRepository::put_and_log`].
    async fn put_and_log(
        &self,
        object: StoredObject,
        actor: &str,
        extra: Vec<LogEntry>,
    ) -> Result<Vec<Activity>>;
    /// See [`ObjectRepository::get`].
    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>>;
    /// See [`ObjectRepository::delete`].
    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity>;
    /// See [`ObjectQuery::query`].
    async fn query(&self, query: &Query) -> Result<Vec<StoredObject>>;
    /// See [`ObjectQuery::count`].
    async fn count(&self, query: &Query) -> Result<usize>;
    /// See [`ActivityLog::log`].
    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity>;
    /// See [`ChangeFeed::subscribe`].
    fn subscribe(&self) -> broadcast::Receiver<Activity>;
    /// See [`ActivityLog::activities`].
    async fn activities(&self, filter: &ActivityFilter) -> Result<Vec<Activity>>;
    /// See [`UndoRedo::undo`].
    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;
    /// See [`UndoRedo::redo`].
    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;
    /// See [`UndoRedo::undo_state`].
    async fn undo_state(&self, actor: &str) -> Result<UndoState>;
    /// See [`HistoryRetention::retention`].
    async fn retention(&self) -> Result<Retention>;
    /// See [`HistoryRetention::set_retention`].
    async fn set_retention(&self, retention: Retention) -> Result<()>;
    /// See [`HistoryRetention::compact_at`].
    async fn compact_at(&self, now: Timestamp) -> Result<CompactionReport>;
    /// See [`HistoryRetention::compact`].
    async fn compact(&self) -> Result<CompactionReport>;
    /// See [`HistoryRetention::clear_history`].
    async fn clear_history(&self) -> Result<CompactionReport>;
    /// See [`HistoryRetention::mark_synced`].
    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize>;
    /// See [`HistoryRetention::log_stats`].
    async fn log_stats(&self) -> Result<LogStats>;
}

#[async_trait]
impl<T> Store for T
where
    T: ObjectRepository + ObjectQuery + ActivityLog + ChangeFeed + UndoRedo + HistoryRetention,
{
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity> {
        ObjectRepository::put(self, object, actor).await
    }
    async fn put_and_log(
        &self,
        object: StoredObject,
        actor: &str,
        extra: Vec<LogEntry>,
    ) -> Result<Vec<Activity>> {
        ObjectRepository::put_and_log(self, object, actor, extra).await
    }
    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>> {
        ObjectRepository::get(self, id).await
    }
    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity> {
        ObjectRepository::delete(self, id, actor).await
    }
    async fn query(&self, query: &Query) -> Result<Vec<StoredObject>> {
        ObjectQuery::query(self, query).await
    }
    async fn count(&self, query: &Query) -> Result<usize> {
        ObjectQuery::count(self, query).await
    }
    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity> {
        ActivityLog::log(self, kind, object, actor, payload).await
    }
    fn subscribe(&self) -> broadcast::Receiver<Activity> {
        ChangeFeed::subscribe(self)
    }
    async fn activities(&self, filter: &ActivityFilter) -> Result<Vec<Activity>> {
        ActivityLog::activities(self, filter).await
    }
    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        UndoRedo::undo(self, actor, n).await
    }
    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        UndoRedo::redo(self, actor, n).await
    }
    async fn undo_state(&self, actor: &str) -> Result<UndoState> {
        UndoRedo::undo_state(self, actor).await
    }
    async fn retention(&self) -> Result<Retention> {
        HistoryRetention::retention(self).await
    }
    async fn set_retention(&self, retention: Retention) -> Result<()> {
        HistoryRetention::set_retention(self, retention).await
    }
    async fn compact_at(&self, now: Timestamp) -> Result<CompactionReport> {
        HistoryRetention::compact_at(self, now).await
    }
    async fn compact(&self) -> Result<CompactionReport> {
        HistoryRetention::compact(self).await
    }
    async fn clear_history(&self) -> Result<CompactionReport> {
        HistoryRetention::clear_history(self).await
    }
    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize> {
        HistoryRetention::mark_synced(self, up_to).await
    }
    async fn log_stats(&self) -> Result<LogStats> {
        HistoryRetention::log_stats(self).await
    }
}
