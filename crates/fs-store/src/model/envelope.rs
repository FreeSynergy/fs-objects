//! The ActivityStreams 2.0 [`Envelope`] of an object.
//!
//! Pattern: Newtype/aliases for typed fields ([`Timestamp`], [`DpvCategory`]); plain data type
//! whose JSON names follow AS2 (standards first, ADR 0014).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ObjectId;

/// UTC timestamp (stored with microsecond precision).
pub type Timestamp = DateTime<Utc>;

/// A DPV category IRI or CURIE, e.g. `pd:SexualPreference` (ADR 0029).
pub type DpvCategory = String;

/// The ActivityStreams 2.0 envelope of an object (ADR 0020). Always has a `name` (ADR 0032).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Internal id (`UUIDv7`).
    pub id: ObjectId,
    /// External absolute IRI, `https://<instance>/<domain>/<uuid>` (ADR 0027), if assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iri: Option<String>,
    /// AS2 type, e.g. `Profile`, `Event`, `Note`.
    #[serde(rename = "type")]
    pub object_type: String,
    /// Display name derived from the content (ADR 0032). Must not be empty.
    pub name: String,
    #[serde(
        default,
        rename = "attributedTo",
        skip_serializing_if = "Option::is_none"
    )]
    /// Actor the object is attributed to.
    pub attributed_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// AS2 primary audience.
    pub to: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// AS2 secondary audience.
    pub cc: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// AS2 private primary audience.
    pub bto: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// AS2 private secondary audience.
    pub bcc: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// AS2 audience.
    pub audience: Vec<String>,
    /// When first stored (kept on update).
    pub published: Timestamp,
    /// When last stored (set by the store).
    pub updated: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// AS2 `context` (grouping, e.g. an address book or calendar).
    pub context: Option<String>,
    #[serde(default, rename = "inReplyTo", skip_serializing_if = "Option::is_none")]
    /// AS2 `inReplyTo`.
    pub in_reply_to: Option<String>,
    /// AS2 `Profile.describes`: the IRI of the thing/person this profile describes (0.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describes: Option<String>,
    /// AS2 `Event.startTime` as UTC instant (0.2). For recurring series: first instance.
    #[serde(default, rename = "startTime", skip_serializing_if = "Option::is_none")]
    pub start_time: Option<Timestamp>,
    /// AS2 `Event.endTime` as UTC instant (0.2). For recurring series: end of the last instance
    /// (the caller decides what an unbounded series uses).
    #[serde(default, rename = "endTime", skip_serializing_if = "Option::is_none")]
    pub end_time: Option<Timestamp>,
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
            describes: None,
            start_time: None,
            end_time: None,
            sensitive: vec![],
        }
    }
}
