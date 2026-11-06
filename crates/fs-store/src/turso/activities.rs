//! Activity-log rows: load, insert, and the compact step view used by undo.
//!
//! Pattern: Repository (internal) — the only module that knows the `activities` table layout.

use turso::{Connection, Value};

use super::codec::decode_activity;
use super::rows::{opt_text, text};
use super::schema::ACTIVITY_COLS;
use super::sql::{collect, exec};
use crate::history::Step;
use crate::{Activity, ActivityId, Result};

/// Load one activity by id.
pub(super) async fn load_activity(c: &Connection, id: ActivityId) -> Result<Option<Activity>> {
    let rows = collect(
        c,
        &format!("SELECT {ACTIVITY_COLS} FROM activities WHERE id = ?"),
        vec![Value::Text(id.to_string())],
    )
    .await?;
    rows.first().map(|r| decode_activity(r)).transpose()
}

/// Append an activity row.
pub(super) async fn insert_activity(c: &Connection, a: &Activity) -> Result<()> {
    exec(
        c,
        &format!("INSERT INTO activities ({ACTIVITY_COLS}) VALUES (?,?,?,?,?,?,?,?)"),
        vec![
            Value::Text(a.id.to_string()),
            Value::Text(a.kind.as_str().into()),
            Value::Text(a.object.to_string()),
            Value::Text(a.actor.clone()),
            Value::Integer(a.at.timestamp_micros()),
            a.target
                .map_or(Value::Null, |t| Value::Text(t.to_string())),
            Value::Text(a.payload.to_string()),
            Value::Integer(i64::from(a.synced)),
        ],
    )
    .await?;
    Ok(())
}

/// The state-changing steps of one actor in log order (input of `history::compute`).
pub(super) async fn load_steps(c: &Connection, actor: &str) -> Result<Vec<Step>> {
    let rows = collect(
        c,
        "SELECT id, kind, target FROM activities \
         WHERE actor = ? AND kind IN ('Create','Update','Delete','Undo') ORDER BY id",
        vec![Value::Text(actor.into())],
    )
    .await?;
    rows.iter()
        .map(|r| {
            Ok(Step {
                id: text(&r[0])?.parse()?,
                kind: text(&r[1])?.parse()?,
                target: opt_text(&r[2])?.map(|s| s.parse()).transpose()?,
            })
        })
        .collect()
}

/// Highest activity id in the log, if any.
pub(super) async fn last_activity_id(c: &Connection) -> Result<Option<ActivityId>> {
    let rows = collect(c, "SELECT MAX(id) FROM activities", vec![]).await?;
    match rows.first().and_then(|r| r.first()) {
        Some(Value::Text(s)) => Ok(Some(s.parse()?)),
        _ => Ok(None),
    }
}
