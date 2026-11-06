//! Object rows: load, write and remove an object together with its index rows.
//!
//! Pattern: Repository (internal) — the only module that knows the `objects` / `index_fields`
//! table layout; the trait impls above it work with `StoredObject`s.

use turso::{Connection, Value};

use super::codec::decode_object;
use super::rows::{opt_text_val, opt_ts_val, text};
use super::schema::OBJECT_COLS;
use super::sql::{collect, exec};
use crate::{ObjectId, Result, StoredObject, sort_key};

/// Fill the `index` map of each object from `index_fields` (in chunks).
pub(super) async fn attach_index(c: &Connection, objs: &mut [StoredObject]) -> Result<()> {
    for chunk in objs.chunks_mut(200) {
        let ph = vec!["?"; chunk.len()].join(",");
        let params = chunk
            .iter()
            .map(|o| Value::Text(o.envelope.id.to_string()))
            .collect();
        let rows = collect(
            c,
            &format!("SELECT object_id, key, value FROM index_fields WHERE object_id IN ({ph}) ORDER BY rowid"),
            params,
        )
        .await?;
        for r in rows {
            let oid = text(&r[0])?;
            if let Some(o) = chunk.iter_mut().find(|o| o.envelope.id.to_string() == oid) {
                o.index.entry(text(&r[1])?).or_default().push(text(&r[2])?);
            }
        }
    }
    Ok(())
}

/// Load one object with its index fields.
pub(super) async fn load_object(c: &Connection, id: ObjectId) -> Result<Option<StoredObject>> {
    let rows = collect(
        c,
        &format!("SELECT {OBJECT_COLS} FROM objects WHERE id = ?"),
        vec![Value::Text(id.to_string())],
    )
    .await?;
    let Some(r) = rows.first() else {
        return Ok(None);
    };
    let mut v = [decode_object(r)?];
    attach_index(c, &mut v).await?;
    let [o] = v;
    Ok(Some(o))
}

/// Replace the object row and its index rows.
pub(super) async fn write_object(c: &Connection, o: &StoredObject) -> Result<()> {
    let e = &o.envelope;
    let id = e.id.to_string();
    remove_object(c, e.id).await?;
    let addressing = serde_json::json!({
        "to": e.to, "cc": e.cc, "bto": e.bto, "bcc": e.bcc, "audience": e.audience
    });
    let (orig, orig_type) = match &o.original {
        Some(b) => (
            Value::Blob(b.bytes.clone()),
            Value::Text(b.media_type.clone()),
        ),
        None => (Value::Null, Value::Null),
    };
    exec(
        c,
        &format!(
            "INSERT INTO objects ({OBJECT_COLS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ),
        vec![
            Value::Text(id.clone()),
            opt_text_val(&e.iri),
            Value::Text(e.object_type.clone()),
            Value::Text(e.name.clone()),
            opt_text_val(&e.context),
            opt_text_val(&e.attributed_to),
            opt_text_val(&e.in_reply_to),
            Value::Integer(e.published.timestamp_micros()),
            Value::Integer(e.updated.timestamp_micros()),
            Value::Text(o.content_type.clone()),
            Value::Text(serde_json::to_string(&o.content)?),
            orig,
            orig_type,
            Value::Text(addressing.to_string()),
            Value::Text(serde_json::to_string(&e.sensitive)?),
            opt_text_val(&e.describes),
            opt_ts_val(&e.start_time),
            opt_ts_val(&e.end_time),
            Value::Text(sort_key(&e.name)),
        ],
    )
    .await?;
    for (k, vals) in &o.index {
        for v in vals {
            exec(
                c,
                "INSERT INTO index_fields (object_id, key, value, value_norm) VALUES (?,?,?,?)",
                vec![
                    Value::Text(id.clone()),
                    Value::Text(k.clone()),
                    Value::Text(v.clone()),
                    Value::Text(sort_key(v)),
                ],
            )
            .await?;
        }
    }
    Ok(())
}

/// Delete the object row and its index rows.
pub(super) async fn remove_object(c: &Connection, id: ObjectId) -> Result<()> {
    let id = id.to_string();
    exec(
        c,
        "DELETE FROM objects WHERE id = ?",
        vec![Value::Text(id.clone())],
    )
    .await?;
    exec(
        c,
        "DELETE FROM index_fields WHERE object_id = ?",
        vec![Value::Text(id)],
    )
    .await?;
    Ok(())
}
