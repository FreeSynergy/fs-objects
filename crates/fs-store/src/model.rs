//! Data model: ids, envelope, stored object, activity (ADR 0020, 0027, 0030, 0032).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

/// UTC timestamp (stored with microsecond precision).
pub type Timestamp = DateTime<Utc>;

/// A DPV category IRI or CURIE, e.g. `pd:SexualPreference` (ADR 0029).
pub type DpvCategory = String;

macro_rules! uuid_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Parse from a UUID; only version 7 is accepted (ADR 0027).
            pub fn from_uuid(u: Uuid) -> Result<Self> {
                if u.get_version_num() == 7 {
                    Ok(Self(u))
                } else {
                    Err(Error::Invalid(format!("{u} is not a UUIDv7")))
                }
            }
            /// The underlying UUID.
            pub fn as_uuid(&self) -> Uuid { self.0 }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.hyphenated().fmt(f) }
        }
        impl FromStr for $name {
            type Err = Error;
            fn from_str(s: &str) -> Result<Self> {
                let s = s.strip_prefix("urn:uuid:").unwrap_or(s);
                let u = Uuid::parse_str(s).map_err(|e| Error::Invalid(e.to_string()))?;
                Self::from_uuid(u)
            }
        }
    };
}

uuid_id!(
    /// Identifier of an object: UUIDv7 (RFC 9562, ADR 0027).
    ObjectId
);
uuid_id!(
    /// Identifier of an activity-log entry: UUIDv7, so id order is log order.
    ActivityId
);

impl ObjectId {
    /// A fresh, time-ordered id.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl Default for ObjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityId {
    /// A fresh id. Callers that need strict monotonicity within a process
    /// (the backends do) use [`ActivityId::next_after`].
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// A fresh id that is strictly greater than `last`.
    pub fn next_after(last: Option<ActivityId>) -> Self {
        let fresh = Uuid::now_v7();
        match last {
            Some(l) if fresh <= l.0 => {
                // Same millisecond (or clock step back): increment the random tail.
                Self(Uuid::from_u128(l.0.as_u128() + 1))
            }
            _ => Self(fresh),
        }
    }
}
impl Default for ActivityId {
    fn default() -> Self {
        Self::new()
    }
}

/// The ActivityStreams 2.0 envelope of an object (ADR 0020). Always has a `name` (ADR 0032).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Internal id (UUIDv7).
    pub id: ObjectId,
    /// External absolute IRI, `https://<instance>/<domain>/<uuid>` (ADR 0027), if assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iri: Option<String>,
    /// AS2 type, e.g. `Profile`, `Event`, `Note`.
    #[serde(rename = "type")]
    pub object_type: String,
    /// Display name derived from the content (ADR 0032). Must not be empty.
    pub name: String,
    #[serde(default, rename = "attributedTo", skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cc: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bto: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bcc: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience: Vec<String>,
    pub published: Timestamp,
    pub updated: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, rename = "inReplyTo", skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    /// DPV categories of sensitive data in this object (ADR 0029).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive: Vec<DpvCategory>,
}

impl Envelope {
    /// New envelope with a fresh id and `published = updated = now`.
    pub fn new(object_type: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: ObjectId::new(),
            iri: None,
            object_type: object_type.into(),
            name: name.into(),
            attributed_to: None,
            to: vec![],
            cc: vec![],
            bto: vec![],
            bcc: vec![],
            audience: vec![],
            published: now,
            updated: now,
            context: None,
            in_reply_to: None,
            sensitive: vec![],
        }
    }
}

/// A received original, kept byte-for-byte (ADR 0030).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blob {
    /// Media type of the original, e.g. `message/rfc822`, `text/vcard`.
    pub media_type: String,
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
        base64::engine::general_purpose::STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

/// Extracted, queryable fields: key -> values (e.g. `email` -> several addresses).
pub type IndexFields = BTreeMap<String, Vec<String>>;

/// An object: envelope + domain content (+ optional original).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredObject {
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
    pub fn new(envelope: Envelope, content_type: impl Into<String>, content: serde_json::Value) -> Self {
        Self { envelope, content_type: content_type.into(), content, original: None, index: BTreeMap::new() }
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

/// AS2 activity kinds recorded in the log (ADR 0020).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActivityKind {
    Create,
    Update,
    Delete,
    Add,
    Remove,
    Offer,
    Accept,
    Reject,
    Undo,
    Read,
}

impl ActivityKind {
    /// Wire/DB name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Update => "Update",
            Self::Delete => "Delete",
            Self::Add => "Add",
            Self::Remove => "Remove",
            Self::Offer => "Offer",
            Self::Accept => "Accept",
            Self::Reject => "Reject",
            Self::Undo => "Undo",
            Self::Read => "Read",
        }
    }
    /// Kinds that change object state (and can be undone).
    pub fn changes_state(&self) -> bool {
        matches!(self, Self::Create | Self::Update | Self::Delete | Self::Undo)
    }
}

impl FromStr for ActivityKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "Create" => Self::Create,
            "Update" => Self::Update,
            "Delete" => Self::Delete,
            "Add" => Self::Add,
            "Remove" => Self::Remove,
            "Offer" => Self::Offer,
            "Accept" => Self::Accept,
            "Reject" => Self::Reject,
            "Undo" => Self::Undo,
            "Read" => Self::Read,
            other => return Err(Error::Invalid(format!("unknown activity kind {other}"))),
        })
    }
}

/// One append-only log entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: ActivityId,
    pub kind: ActivityKind,
    /// The object this activity is about.
    pub object: ObjectId,
    /// IRI/identifier of the acting person/program.
    pub actor: String,
    pub at: Timestamp,
    /// For `Undo`: the activity that is reverted (redo = `Undo` of an `Undo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ActivityId>,
    /// State-changing kinds carry a [`ChangeSet`] (before/after images);
    /// log-only kinds carry whatever the caller supplies.
    pub payload: serde_json::Value,
    /// Handed to / acknowledged by the hub (see `Store::mark_synced`).
    pub synced: bool,
}

impl Activity {
    /// Parse the payload as before/after images (state-changing kinds).
    pub fn change_set(&self) -> Result<ChangeSet> {
        Ok(serde_json::from_value(self.payload.clone())?)
    }
}

/// Before/after images of an object change. This is the object-level unit the
/// hub needs for sync (`after`) and the unit undo/redo works on.
///
/// The immutable `original` is included only when the change creates or
/// removes the object (it is never rewritten, ADR 0030, so repeating it in
/// every update would only waste space).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub before: Option<StoredObject>,
    pub after: Option<StoredObject>,
}

impl ChangeSet {
    pub(crate) fn new(before: Option<StoredObject>, after: Option<StoredObject>) -> Self {
        let (mut before, mut after) = (before, after);
        if before.is_some() && after.is_some() {
            if let Some(b) = before.as_mut() {
                b.original = None;
            }
            if let Some(a) = after.as_mut() {
                a.original = None;
            }
        }
        Self { before, after }
    }
}
