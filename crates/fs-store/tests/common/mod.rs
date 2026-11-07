//! Shared helpers for the integration tests.
#![allow(dead_code)]

use chrono::{TimeZone, Utc};
use fs_store::*;
use serde_json::json;

/// The acting user in all tests.
pub const ME: &str = "https://example.org/users/me";

/// A contact card `Profile` with email and kind index fields.
pub fn card(name: &str) -> StoredObject {
    let mut e = Envelope::new("Profile", name);
    e.context = Some("contacts".into());
    e.attributed_to = Some(ME.into());
    e.to = vec![ME.into()];
    StoredObject::new(
        e,
        "application/jscontact+json",
        json!({"@type":"Card","name":{"full":name}}),
    )
    .with_index("email", format!("{}@example.org", name.to_lowercase()))
    .with_index("kind", "individual")
}

/// A generic object of type `ty` in context `t`.
pub fn obj(ty: &str, name: &str) -> StoredObject {
    let mut e = Envelope::new(ty, name);
    e.context = Some("t".into());
    StoredObject::new(e, "application/json", json!({ "n": name }))
}

/// A fresh in-memory store.
pub async fn mem() -> TursoStore {
    TursoStore::open_in_memory().await.unwrap()
}

/// Names of the objects, in order.
pub fn names(v: &[StoredObject]) -> Vec<&str> {
    v.iter().map(|o| o.envelope.name.as_str()).collect()
}

/// 2026-03-01 at `h`:00 UTC.
pub fn at(h: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, h, 0, 0).unwrap()
}

/// A valid AES-256 key for encryption tests.
pub fn key() -> EncryptionKey {
    EncryptionKey {
        cipher: Cipher::Aes256Gcm,
        key: (0u8..32).collect(),
    }
}

/// The whole activity log as JSON text (to look for content in it).
pub async fn log_text(s: &TursoStore) -> String {
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    serde_json::to_string(&all).unwrap()
}

/// How often `needle` occurs in every file of `dir` (database and write-ahead log).
pub fn needle_in_files(dir: &std::path::Path, needle: &str) -> usize {
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let b = std::fs::read(e.unwrap().path()).unwrap();
        n += b
            .windows(needle.len())
            .filter(|w| *w == needle.as_bytes())
            .count();
    }
    n
}

/// A note whose content is `text`.
pub fn note(name: &str, text: &str) -> StoredObject {
    let mut o = obj("Note", name);
    o.content = serde_json::json!({ "t": text });
    o
}

/// Raw SQL on a closed database file (rows of the first column as text).
pub async fn raw_sql(path: &std::path::Path, sql: &str) -> Vec<String> {
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let c = db.connect().unwrap();
    let mut rows = c.query(sql, ()).await.unwrap();
    let mut out = vec![];
    while let Some(r) = rows.next().await.unwrap() {
        out.push(match r.get_value(0).unwrap() {
            turso::Value::Text(t) => t,
            v => format!("{v:?}"),
        });
    }
    out
}
