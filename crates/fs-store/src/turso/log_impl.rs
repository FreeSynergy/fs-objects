//! [`ActivityLog`] for the Turso backend: log-only entries and reading the log.
//!
//! Pattern: Repository (log) — append and filter activities; the hub reads with a cursor.

use async_trait::async_trait;
use turso::Value;

use super::TursoStore;
use super::codec::decode_activity;
use super::schema::ACTIVITY_COLS;
use super::sql::collect;
use super::tx::in_tx;
use crate::{Activity, ActivityFilter, ActivityKind, ActivityLog, ObjectId, Result};

#[async_trait]
impl ActivityLog for TursoStore {
    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity> {
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let act = in_tx(&conn, self.append_log(inner, actor, object, kind, payload)).await?;
        self.publish(std::slice::from_ref(&act));
        Ok(act)
    }

    async fn activities(&self, f: &ActivityFilter) -> Result<Vec<Activity>> {
        let inner = self.inner.lock().await;
        let mut clauses: Vec<&str> = Vec::new();
        let mut p: Vec<Value> = Vec::new();
        if let Some(a) = f.after {
            clauses.push("id > ?");
            p.push(Value::Text(a.to_string()));
        }
        if let Some(o) = f.object {
            clauses.push("object_id = ?");
            p.push(Value::Text(o.to_string()));
        }
        if let Some(a) = &f.actor {
            clauses.push("actor = ?");
            p.push(Value::Text(a.clone()));
        }
        if f.unsynced_only {
            clauses.push("synced = 0");
        }
        let w = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        p.push(Value::Integer(f.limit.map_or(i64::MAX, |l| l as i64)));
        let rows = collect(
            &inner.conn,
            &format!("SELECT {ACTIVITY_COLS} FROM activities{w} ORDER BY id LIMIT ?"),
            p,
        )
        .await?;
        rows.iter().map(|r| decode_activity(r)).collect()
    }
}
