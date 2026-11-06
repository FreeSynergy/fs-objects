//! Row decoding for objects and activities.
//!
//! Pattern: Adapter — database rows to the public model types.

use std::collections::BTreeMap;

use turso::Value;

use super::rows::{int, opt_int, opt_text, text, ts_from_micros};
use crate::{Activity, Blob, Envelope, Result, StoredObject};

/// Decode a row selected with `OBJECT_COLS` (without the index fields).
pub(super) fn decode_object(row: &[Value]) -> Result<StoredObject> {
    let addressing: BTreeMap<String, Vec<String>> = serde_json::from_str(&text(&row[13])?)?;
    let get = |k: &str| addressing.get(k).cloned().unwrap_or_default();
    let original = match &row[11] {
        Value::Blob(b) => Some(Blob {
            media_type: opt_text(&row[12])?.unwrap_or_else(|| "application/octet-stream".into()),
            bytes: b.clone(),
        }),
        _ => None,
    };
    Ok(StoredObject {
        envelope: Envelope {
            id: text(&row[0])?.parse()?,
            iri: opt_text(&row[1])?,
            object_type: text(&row[2])?,
            name: text(&row[3])?,
            context: opt_text(&row[4])?,
            attributed_to: opt_text(&row[5])?,
            in_reply_to: opt_text(&row[6])?,
            describes: opt_text(&row[15])?,
            start_time: opt_int(&row[16])?.map(ts_from_micros).transpose()?,
            end_time: opt_int(&row[17])?.map(ts_from_micros).transpose()?,
            published: ts_from_micros(int(&row[7])?)?,
            updated: ts_from_micros(int(&row[8])?)?,
            to: get("to"),
            cc: get("cc"),
            bto: get("bto"),
            bcc: get("bcc"),
            audience: get("audience"),
            sensitive: serde_json::from_str(&text(&row[14])?)?,
        },
        content_type: text(&row[9])?,
        content: serde_json::from_str(&text(&row[10])?)?,
        original,
        index: BTreeMap::new(),
    })
}

/// Decode a row selected with `ACTIVITY_COLS`.
pub(super) fn decode_activity(r: &[Value]) -> Result<Activity> {
    Ok(Activity {
        id: text(&r[0])?.parse()?,
        kind: text(&r[1])?.parse()?,
        object: text(&r[2])?.parse()?,
        actor: text(&r[3])?,
        at: ts_from_micros(int(&r[4])?)?,
        target: opt_text(&r[5])?.map(|s| s.parse()).transpose()?,
        payload: serde_json::from_str(&text(&r[6])?)?,
        synced: int(&r[7])? != 0,
    })
}
