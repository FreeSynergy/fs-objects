//! Marks for ids written through `UnloggedObjects`, and the refusal of logged calls on them.
//!
//! Pattern: Guard (proxy check) — the logged paths ask here before they touch an id; the unlogged
//! path records the id in the same transaction as its write, so the mark never lags behind.

use turso::{Connection, Value};

use super::sql::{collect, exec};
use crate::{Error, ObjectId, Result, StoredObject};

fn refusal(id: ObjectId) -> Error {
    Error::Invalid(format!(
        "{id} was written unlogged: a logged call would copy its content into the activity log"
    ))
}

/// Remember that `id` belongs to the unlogged path. Kept after a delete: the id is never logged.
pub(super) async fn mark_unlogged(c: &Connection, id: ObjectId) -> Result<()> {
    exec(
        c,
        "INSERT OR IGNORE INTO unlogged_ids (object_id) VALUES (?)",
        vec![Value::Text(id.to_string())],
    )
    .await?;
    Ok(())
}

/// Refuse a logged call on an id that was written unlogged.
pub(super) async fn refuse_unlogged(c: &Connection, id: ObjectId) -> Result<()> {
    let rows = collect(
        c,
        "SELECT 1 FROM unlogged_ids WHERE object_id = ?",
        vec![Value::Text(id.to_string())],
    )
    .await?;
    if rows.is_empty() {
        Ok(())
    } else {
        Err(refusal(id))
    }
}

/// Refuse a logged read that returns an unlogged object.
pub(super) async fn refuse_unlogged_in(c: &Connection, objs: &[StoredObject]) -> Result<()> {
    for chunk in objs.chunks(200) {
        let ph = vec!["?"; chunk.len()].join(",");
        let params = chunk
            .iter()
            .map(|o| Value::Text(o.envelope.id.to_string()))
            .collect();
        let sql = format!("SELECT object_id FROM unlogged_ids WHERE object_id IN ({ph}) LIMIT 1");
        if let Some(r) = collect(c, &sql, params).await?.first() {
            return Err(refusal(super::rows::text(&r[0])?.parse()?));
        }
    }
    Ok(())
}
