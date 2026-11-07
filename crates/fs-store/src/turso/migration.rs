//! Schema creation and migration to [`SCHEMA_VERSION`].
//!
//! Pattern: Template method — detect the current version, then run the one matching step inside
//! a single transaction (`in_tx`); newer databases are refused.

use turso::{Connection, Value};

use super::identity::ensure_identity;
use super::rows::{opt_int, text};
use super::schema::{INDEXES, SCHEMA, SCHEMA_VERSION, V3_ALTER, V3_TABLES};
use super::sql::{collect, exec};
use super::tx::in_tx;
use crate::{Error, Result, sort_key};

/// Bring the database to [`SCHEMA_VERSION`]: create it, or migrate a 0.1 database.
pub(super) async fn migrate(c: &Connection) -> Result<()> {
    let current = detect_version(c).await?;
    if current > SCHEMA_VERSION {
        return Err(Error::Backend(format!(
            "database schema version {current} is newer than this fs-store ({SCHEMA_VERSION})"
        )));
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    in_tx(c, async {
        if current == 0 {
            c.execute_batch(SCHEMA).await?;
        } else {
            if current == 1 {
                v1_to_v2(c).await?;
            }
            v2_to_v4(c).await?;
        }
        ensure_identity(c).await?;
        c.execute_batch(INDEXES).await?;
        exec(c, "DELETE FROM schema_version", vec![]).await?;
        exec(
            c,
            "INSERT INTO schema_version (version) VALUES (?)",
            vec![Value::Integer(SCHEMA_VERSION)],
        )
        .await?;
        Ok(())
    })
    .await
}

/// 0 = empty database, 1 = 0.1 (tables but no version table), else the stored version.
async fn detect_version(c: &Connection) -> Result<i64> {
    let tables: Vec<String> = collect(
        c,
        "SELECT name FROM sqlite_master WHERE type = 'table'",
        vec![],
    )
    .await?
    .iter()
    .map(|r| text(&r[0]))
    .collect::<Result<_>>()?;
    let has = |n: &str| tables.iter().any(|t| t == n);
    Ok(if has("schema_version") {
        let rows = collect(c, "SELECT MAX(version) FROM schema_version", vec![]).await?;
        rows.first()
            .and_then(|r| opt_int(&r[0]).ok().flatten())
            .unwrap_or(0)
    } else {
        i64::from(has("objects"))
    })
}

/// 1 -> 2: first-class envelope fields, normalised sort/search keys.
async fn v1_to_v2(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
         ALTER TABLE objects ADD COLUMN describes TEXT;
         ALTER TABLE objects ADD COLUMN start_time INTEGER;
         ALTER TABLE objects ADD COLUMN end_time INTEGER;
         ALTER TABLE objects ADD COLUMN name_sort TEXT NOT NULL DEFAULT '';
         ALTER TABLE index_fields ADD COLUMN value_norm TEXT NOT NULL DEFAULT '';",
    )
    .await?;
    for r in collect(c, "SELECT id, name FROM objects", vec![]).await? {
        exec(
            c,
            "UPDATE objects SET name_sort = ? WHERE id = ?",
            vec![Value::Text(sort_key(&text(&r[1])?)), r[0].clone()],
        )
        .await?;
    }
    for r in collect(c, "SELECT rowid, value FROM index_fields", vec![]).await? {
        exec(
            c,
            "UPDATE index_fields SET value_norm = ? WHERE rowid = ?",
            vec![Value::Text(sort_key(&text(&r[1])?)), r[0].clone()],
        )
        .await?;
    }
    Ok(())
}

/// 2 or 3 -> 4: tables and the `redacted` column, each only if missing (a file written by the unreleased
/// version 3 has `unlogged_ids` and no `redacted`, a version 2 file has neither). Existing rows and logs
/// read unchanged. Ids written unlogged by 0.3 cannot be told apart from objects whose log was compacted,
/// so they are not marked; see `DESIGN.md`. The identity is added by `ensure_identity` right after.
async fn v2_to_v4(c: &Connection) -> Result<()> {
    c.execute_batch(V3_TABLES).await?;
    let cols = collect(
        c,
        "SELECT name FROM pragma_table_info('activities')",
        vec![],
    )
    .await?;
    let has_redacted = cols
        .iter()
        .any(|r| text(&r[0]).is_ok_and(|n| n == "redacted"));
    if !has_redacted {
        c.execute_batch(V3_ALTER).await?;
    }
    Ok(())
}
