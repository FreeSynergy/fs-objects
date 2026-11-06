//! [`ObjectQuery`]: read many objects by a [`Query`].
//!
//! Pattern: Repository (read side) — separated from writes so read-only consumers can depend on
//! this trait alone (and tests can fake it).

use async_trait::async_trait;

use crate::{Query, Result, StoredObject};

/// Query objects.
#[async_trait]
pub trait ObjectQuery: Send + Sync {
    /// Objects matching the query, in its order, honouring `limit`/`offset`.
    async fn query(&self, query: &Query) -> Result<Vec<StoredObject>>;

    /// Number of objects matching the query's filters (limit/offset ignored).
    async fn count(&self, query: &Query) -> Result<usize>;
}
