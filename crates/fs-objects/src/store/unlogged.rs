//! [`UnloggedObjects`]: objects written with no trace of when, for data that must not carry a time.
//!
//! Pattern: Repository (a separate narrow trait, interface segregation) — it is deliberately not part
//! of the [`Store`](crate::Store) facade, so nobody reaches it by accident, and existing backends need
//! not implement it. Written for anonymous ballots in `fs-surveys`.

use async_trait::async_trait;

use crate::{ObjectId, Result, StoredObject};

/// Write and read single objects without the activity log.
///
/// No activity is written at any moment, so a crash leaves no entry behind. No change-feed event is
/// sent, nothing reaches the hub, and these writes cannot be undone. The store does not stamp the
/// times: `published` and `updated` are stored as given, to the microsecond. Use a fixed time if the
/// time is what must not be known.
///
/// What this does not hide: a fresh [`ObjectId`] is a `UUIDv7` and contains its creation time, so one
/// object per secret is the wrong shape; keep many secrets inside one object, as the ballot box does.
/// Rows keep their insertion order. Someone who copies the file between two writes sees the
/// difference, and old pages can stay on disk until the database reuses them.
///
/// Keep such objects in a store of their own. An id with logged history is refused here; the logged
/// calls must never touch an id written here, because they would copy it into a timed activity.
#[async_trait]
pub trait UnloggedObjects: Send + Sync {
    /// Create or replace. The original follows the same rule as a logged write: it never changes.
    async fn put_unlogged(&self, object: StoredObject) -> Result<()>;

    /// Read one object by id.
    async fn read_unlogged(&self, id: ObjectId) -> Result<Option<StoredObject>>;

    /// Delete an object, if it exists.
    async fn delete_unlogged(&self, id: ObjectId) -> Result<()>;
}
