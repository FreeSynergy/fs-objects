//! [`HistoryRetention`] for the Turso backend: policy, compaction, sync marks, statistics.
//!
//! Pattern: Strategy (policy) — `Retention` is the policy value stored in `meta`; the actual
//! deletion lives in `compaction.rs`.

use async_trait::async_trait;
use turso::Value;

use super::TursoStore;
use super::rows::{int, now, text};
use super::sql::{collect, exec};
use super::tx::in_tx;
use crate::{
    ActivityId, CompactionReport, HistoryRetention, LogStats, Result, Retention, Timestamp,
};

#[async_trait]
impl HistoryRetention for TursoStore {
    async fn retention(&self) -> Result<Retention> {
        let inner = self.inner.lock().await;
        let rows = collect(
            &inner.conn,
            "SELECT value FROM meta WHERE key = 'retention'",
            vec![],
        )
        .await?;
        match rows.first() {
            Some(r) => Ok(serde_json::from_str(&text(&r[0])?)?),
            None => Ok(Retention::UNLIMITED),
        }
    }

    async fn set_retention(&self, r: Retention) -> Result<()> {
        let inner = self.inner.lock().await;
        let c = &inner.conn;
        in_tx(c, async {
            exec(c, "DELETE FROM meta WHERE key = 'retention'", vec![]).await?;
            exec(
                c,
                "INSERT INTO meta (key, value) VALUES ('retention', ?)",
                vec![Value::Text(serde_json::to_string(&r)?)],
            )
            .await?;
            Ok(())
        })
        .await
    }

    async fn compact_at(&self, at: Timestamp) -> Result<CompactionReport> {
        let r = self.retention().await?;
        self.compact_with(r, at, false).await
    }

    async fn clear_history(&self) -> Result<CompactionReport> {
        self.compact_with(Retention::UNLIMITED, now(), true).await
    }

    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize> {
        let inner = self.inner.lock().await;
        let n = exec(
            &inner.conn,
            "UPDATE activities SET synced = 1 WHERE synced = 0 AND id <= ?",
            vec![Value::Text(up_to.to_string())],
        )
        .await?;
        Ok(n as usize)
    }

    async fn log_stats(&self) -> Result<LogStats> {
        let inner = self.inner.lock().await;
        let rows = collect(
            &inner.conn,
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN synced = 0 THEN 1 ELSE 0 END), 0), COALESCE(SUM(LENGTH(payload)), 0) FROM activities",
            vec![],
        )
        .await?;
        let r = &rows[0];
        Ok(LogStats {
            entries: int(&r[0])? as usize,
            unsynced: int(&r[1])? as usize,
            payload_bytes: int(&r[2])? as usize,
        })
    }
}
