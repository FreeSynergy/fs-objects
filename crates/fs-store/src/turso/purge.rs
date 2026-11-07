//! Log purge of one object.
//!
//! Pattern: Template method — one transaction: remember the high-water mark, count, delete, mark the object
//! as purged. Marking makes later writes log without images (`payload.rs`); the high-water mark keeps
//! activity ids above anything the hub accepted, also when the purged entry was the newest.

use turso::{Connection, Value};

use super::TursoStore;
use super::activities::remember_high_water;
use super::rows::{int, text};
use super::sql::{collect, exec};
use super::tx::in_tx;
use crate::{ObjectId, PurgeReport, Result};

/// True if `id` was purged: later writes on it are logged without images.
pub(super) async fn is_purged(c: &Connection, id: ObjectId) -> Result<bool> {
    let sql = "SELECT 1 FROM purged_objects WHERE object_id = ?";
    Ok(!collect(c, sql, vec![Value::Text(id.to_string())])
        .await?
        .is_empty())
}

impl TursoStore {
    /// See `HistoryRetention::purge_object`.
    pub(super) async fn purge(&self, id: ObjectId) -> Result<PurgeReport> {
        let inner = self.inner.lock().await;
        let c = &inner.conn;
        let key = vec![Value::Text(id.to_string())];
        let report = in_tx(c, async {
            remember_high_water(c).await?;
            let stat = collect(
                c,
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN synced = 0 THEN 1 ELSE 0 END), 0), MAX(id) \
                 FROM activities WHERE object_id = ?",
                key.clone(),
            )
            .await?;
            let row = &stat[0];
            let removed = int(&row[0])? as usize;
            let highest_removed = match &row[2] {
                Value::Text(_) => Some(text(&row[2])?.parse()?),
                _ => None,
            };
            let unsynced_removed = int(&row[1])? as usize;
            exec(c, "DELETE FROM activities WHERE object_id = ?", key.clone()).await?;
            self.check_purge_fault()?;
            exec(
                c,
                "INSERT OR IGNORE INTO purged_objects (object_id) VALUES (?)",
                key.clone(),
            )
            .await?;
            Ok(PurgeReport {
                removed,
                unsynced_removed,
                highest_removed,
            })
        })
        .await?;
        self.clean_file(c).await?;
        Ok(report)
    }

    /// Make the freed pages and the write-ahead log forget the removed bytes: Turso 0.8.3 ignores
    /// `PRAGMA secure_delete`, and deleted rows stay in free pages and in old WAL frames. Checkpoint
    /// (truncating the WAL), then `VACUUM` rewrites the file. Measured: the needle is gone from both files.
    async fn clean_file(&self, c: &Connection) -> Result<()> {
        if !self.on_disk {
            return Ok(());
        }
        for sql in ["PRAGMA wal_checkpoint(TRUNCATE)", "VACUUM"] {
            let mut rows = c.query(sql, ()).await?;
            while rows.next().await?.is_some() {}
        }
        Ok(())
    }
}
