//! [`ObjectRepository`]: create, update, read and delete objects.
//!
//! Pattern: Repository — the CRUD contract of the store; every write is atomic with its
//! activity-log entry.

use async_trait::async_trait;

use crate::{Activity, LogEntry, ObjectId, Result, StoredObject};

/// Write and read single objects.
#[async_trait]
pub trait ObjectRepository: Send + Sync {
    /// Create or update (decided by whether `object.envelope.id` exists).
    /// The store sets `updated` to now; `published` of an existing object is kept.
    /// Writes object + `Create`/`Update` activity in one transaction.
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity>;

    /// Like [`ObjectRepository::put`], and append further log-only activities (e.g. `Accept` /
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

    /// Read one object by id.
    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>>;

    /// Delete an object, writing a `Delete` activity (carrying the old image so it can be undone).
    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity>;
}
