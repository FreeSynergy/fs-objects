//! The payload of a state-changing activity: before/after images, or none for a purged object.
//!
//! Pattern: Specification — the predicate "was this object purged" (`is_purged`) decides the payload: a normal
//! object logs both images, a purged one logs none, so a later write cannot copy the content it was purged for
//! back into the log.

use turso::Connection;

use super::purge::is_purged;
use crate::{ChangeSet, ObjectId, Result, StoredObject};

/// The payload for one change and whether it is written without images (such an entry is not undoable).
pub(super) async fn payload_for(
    c: &Connection,
    object: ObjectId,
    before: Option<StoredObject>,
    after: Option<StoredObject>,
) -> Result<(serde_json::Value, bool)> {
    if is_purged(c, object).await? {
        return Ok((serde_json::to_value(ChangeSet::default())?, true));
    }
    Ok((serde_json::to_value(ChangeSet::new(before, after))?, false))
}
