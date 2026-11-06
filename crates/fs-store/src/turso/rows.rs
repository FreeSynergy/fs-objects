//! Typed access to raw Turso [`Value`]s and the store clock.
//!
//! Pattern: Adapter — turns the foreign dynamic `Value` into our typed fields and errors.

use chrono::{DateTime, SubsecRound, Utc};
use turso::Value;

use crate::{Error, Result, Timestamp};

/// Current time at the store's microsecond precision.
pub(super) fn now() -> Timestamp {
    Utc::now().trunc_subsecs(6)
}

pub(super) fn ts_from_micros(us: i64) -> Result<Timestamp> {
    DateTime::from_timestamp_micros(us).ok_or_else(|| Error::Backend(format!("bad timestamp {us}")))
}

pub(super) fn text(v: &Value) -> Result<String> {
    match v {
        Value::Text(s) => Ok(s.clone()),
        other => Err(Error::Backend(format!("expected text, got {other:?}"))),
    }
}

pub(super) fn opt_text(v: &Value) -> Result<Option<String>> {
    match v {
        Value::Null => Ok(None),
        other => text(other).map(Some),
    }
}

pub(super) fn int(v: &Value) -> Result<i64> {
    match v {
        Value::Integer(i) => Ok(*i),
        other => Err(Error::Backend(format!("expected integer, got {other:?}"))),
    }
}

pub(super) fn opt_int(v: &Value) -> Result<Option<i64>> {
    match v {
        Value::Null => Ok(None),
        other => int(other).map(Some),
    }
}

pub(super) fn opt_ts_val(t: &Option<Timestamp>) -> Value {
    t.map(|t| Value::Integer(t.timestamp_micros()))
        .unwrap_or(Value::Null)
}

pub(super) fn opt_text_val(s: &Option<String>) -> Value {
    s.as_ref()
        .map(|s| Value::Text(s.clone()))
        .unwrap_or(Value::Null)
}
