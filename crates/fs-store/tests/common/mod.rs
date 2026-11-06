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
