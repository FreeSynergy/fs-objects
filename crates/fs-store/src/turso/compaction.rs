//! Compaction of the activity log against a retention policy.
//!
//! Pattern: Template method — `compact_with` computes the cut-off id from the policy, deletes
//! synced entries up to it and reports; `compact_at` / `clear_history` only differ in the policy.

use chrono::Duration as ChronoDuration;
use turso::Value;

use super::TursoStore;
use super::rows::{int, opt_text};
use super::sql::{collect, exec};
use super::tx::in_tx;
use crate::{CompactionReport, Error, Result, Retention, Timestamp};

/// Highest id from `rows` of a `SELECT MAX(id)`-like query.
fn first_id(rows: &[Vec<Value>]) -> Option<String> {
    rows.first().and_then(|r| opt_text(&r[0]).ok().flatten())
}

impl TursoStore {
    /// Remove synced entries outside `r` at time `now`; `everything` drops every synced entry.
    pub(super) async fn compact_with(
        &self,
        r: Retention,
        now: Timestamp,
        everything: bool,
    ) -> Result<CompactionReport> {
        let inner = self.inner.lock().await;
        let c = &inner.conn;
        in_tx(c, async {
            // Highest id that is outside retention; everything up to it is eligible (a prefix).
            let mut cut: Option<String> = None;
            let mut bump = |cand: Option<String>| {
                if let Some(x) = cand {
                    if cut.as_ref().is_none_or(|c| x > *c) {
                        cut = Some(x);
                    }
                }
            };
            if everything {
                bump(first_id(
                    &collect(c, "SELECT MAX(id) FROM activities", vec![]).await?,
                ));
            }
            if let Some(age) = r.max_age {
                let age =
                    ChronoDuration::from_std(age).map_err(|e| Error::Invalid(e.to_string()))?;
                let cutoff = (now - age).timestamp_micros();
                let sql = "SELECT MAX(id) FROM activities WHERE at < ?";
                bump(first_id(
                    &collect(c, sql, vec![Value::Integer(cutoff)]).await?,
                ));
            }
            if let Some(steps) = r.max_steps {
                let sql = "SELECT id FROM activities ORDER BY id DESC LIMIT 1 OFFSET ?";
                bump(first_id(
                    &collect(c, sql, vec![Value::Integer(steps as i64)]).await?,
                ));
            }
            let mut removed = 0;
            let mut kept_unsynced = 0;
            if let Some(cut) = cut {
                removed = exec(
                    c,
                    "DELETE FROM activities WHERE synced = 1 AND id <= ?",
                    vec![Value::Text(cut.clone())],
                )
                .await? as usize;
                let rows = collect(
                    c,
                    "SELECT COUNT(*) FROM activities WHERE synced = 0 AND id <= ?",
                    vec![Value::Text(cut)],
                )
                .await?;
                kept_unsynced = int(&rows[0][0])? as usize;
            }
            let rows = collect(c, "SELECT COUNT(*) FROM activities", vec![]).await?;
            Ok(CompactionReport {
                removed,
                kept_unsynced,
                remaining: int(&rows[0][0])? as usize,
            })
        })
        .await
    }
}
