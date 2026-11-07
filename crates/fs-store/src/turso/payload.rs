//! The payload of a state-changing activity: before/after images, or none for a purged object.
//!
//! Pattern: Specification — the predicate "was this object purged" (`is_purged`) decides the payload: a normal
//! object logs both images, a purged one logs none, so a later write cannot copy the content it was purged for
//! back into the log.

use turso::Connection;

use super::purge::is_purged;
use crate::{ChangeSet, ObjectId, Result, StoredObject};

/// Payload of an entry without images. `before` and `after` are the string `"redacted"`, not null or absent:
/// a reader of fs-store 0.2/0.3 parses them as `Option<StoredObject>`, fails, and so reports an error instead
/// of reading `after == None` as a deletion (the format is fixed with the 0.4 tag; see `DESIGN.md`).
///
/// Returns the payload for one change and whether it is written without images (such an entry is not undoable).
pub(super) async fn payload_for(
    c: &Connection,
    object: ObjectId,
    before: Option<StoredObject>,
    after: Option<StoredObject>,
) -> Result<(serde_json::Value, bool)> {
    if is_purged(c, object).await? {
        return Ok((
            serde_json::json!({ "redacted": true, "before": "redacted", "after": "redacted" }),
            true,
        ));
    }
    Ok((serde_json::to_value(ChangeSet::new(before, after))?, false))
}
