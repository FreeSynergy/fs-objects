//! The store identity kept in table `meta` (key `store_id`), outside the activity log.
//!
//! Pattern: Template method — `ensure_identity` is one step of the migration transaction: keep the id if
//! there is one, otherwise make exactly one. A copy of the file keeps the row, a new file makes its own.

use turso::{Connection, Value};

use super::rows::text;
use super::sql::{collect, exec};
use crate::{Error, Result, StoreId};

const KEY: &str = "store_id";

/// Make the identity if the database has none (new database, or one from before schema version 4).
pub(super) async fn ensure_identity(c: &Connection) -> Result<()> {
    if stored(c).await?.is_none() {
        exec(
            c,
            "INSERT INTO meta (key, value) VALUES (?, ?)",
            vec![
                Value::Text(KEY.into()),
                Value::Text(StoreId::generate().to_string()),
            ],
        )
        .await?;
    }
    Ok(())
}

/// The identity of an open, migrated database.
pub(super) async fn read_identity(c: &Connection) -> Result<StoreId> {
    stored(c)
        .await?
        .ok_or_else(|| Error::Backend("store has no identity".into()))
}

async fn stored(c: &Connection) -> Result<Option<StoreId>> {
    let rows = collect(
        c,
        "SELECT value FROM meta WHERE key = ?",
        vec![Value::Text(KEY.into())],
    )
    .await?;
    rows.first().map(|r| text(&r[0])?.parse()).transpose()
}
