//! [`StoredObject`]: envelope + domain content (+ immutable original).
//!
//! Pattern: Builder (`with_original`, `with_index`) — fluent construction of an object.

use std::collections::BTreeMap;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::Envelope;

/// A received original, kept byte-for-byte (ADR 0030).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blob {
    /// Media type of the original, e.g. `message/rfc822`, `text/vcard`.
    pub media_type: String,
    /// The raw bytes, base64 in JSON.
    #[serde(with = "b64")]
    pub bytes: Vec<u8>,
}

mod b64 {
    use super::*;
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(b))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

/// Extracted, queryable fields: key -> values (e.g. `email` -> several addresses).
pub type IndexFields = BTreeMap<String, Vec<String>>;

/// An object: envelope + domain content (+ optional original).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredObject {
    /// AS2 envelope.
    pub envelope: Envelope,
    /// Media type of `content`, e.g. `application/jscontact+json`.
    pub content_type: String,
    /// Domain content in its standard (JSContact, JSCalendar, ...).
    pub content: serde_json::Value,
    /// The received original, if any (ADR 0030). Immutable once stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<Blob>,
    /// Domain-extracted index fields (ADR 0020), supplied by the caller.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub index: IndexFields,
}

impl StoredObject {
    /// An object without original or index fields.
    pub fn new(
        envelope: Envelope,
        content_type: impl Into<String>,
        content: serde_json::Value,
    ) -> Self {
        Self {
            envelope,
            content_type: content_type.into(),
            content,
            original: None,
            index: BTreeMap::new(),
        }
    }
    /// Builder: attach the original.
    pub fn with_original(mut self, original: Blob) -> Self {
        self.original = Some(original);
        self
    }
    /// Builder: add an index value.
    pub fn with_index(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.index.entry(key.into()).or_default().push(value.into());
        self
    }

    /// Equality that ignores the original when `snapshot` has none
    /// (update snapshots omit the immutable original).
    pub(crate) fn matches_snapshot(&self, snapshot: &StoredObject) -> bool {
        let mut a = self.clone();
        if snapshot.original.is_none() {
            a.original = None;
        }
        a == *snapshot
    }
}
