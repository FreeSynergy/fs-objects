//! Activity-log entries: [`ActivityKind`], [`Activity`] and the [`ChangeSet`] payload.
//!
//! Pattern: Command (data side) — a state-changing activity carries `before`/`after` images, so
//! it can be applied (hub sync) and inverted (undo) without replaying anything.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{ActivityId, Error, ObjectId, Result, StoredObject, Timestamp};

/// AS2 activity kinds recorded in the log (ADR 0020).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActivityKind {
    /// Object created (state-changing).
    Create,
    /// Object updated (state-changing).
    Update,
    /// Object deleted (state-changing).
    Delete,
    /// AS2 `Add`. Log-only.
    Add,
    /// AS2 `Remove`. Log-only.
    Remove,
    /// AS2 `Offer`. Log-only.
    Offer,
    /// AS2 `Accept`. Log-only.
    Accept,
    /// AS2 `Reject`. Log-only.
    Reject,
    /// Revert of another activity (state-changing).
    Undo,
    /// AS2 `Read`. Log-only.
    Read,
    /// AS2 `Invite` (subtype of `Offer`), e.g. event invitation. Log-only.
    Invite,
    /// AS2 `TentativeAccept` (subtype of `Accept`). Log-only.
    TentativeAccept,
    /// AS2 `Join`. Log-only (reserved for later use).
    Join,
    /// AS2 `Leave`. Log-only (reserved for later use).
    Leave,
    /// AS2 `Like`. Log-only (reserved for later use).
    Like,
    /// AS2 `Move`. Log-only (reserved for later use).
    Move,
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
            Self::Invite => "Invite",
            Self::TentativeAccept => "TentativeAccept",
            Self::Join => "Join",
            Self::Leave => "Leave",
            Self::Like => "Like",
            Self::Move => "Move",
        }
    }
    /// Kinds that change object state (and can be undone).
    pub fn changes_state(&self) -> bool {
        matches!(
            self,
            Self::Create | Self::Update | Self::Delete | Self::Undo
        )
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
            "Invite" => Self::Invite,
            "TentativeAccept" => Self::TentativeAccept,
            "Join" => Self::Join,
            "Leave" => Self::Leave,
            "Like" => Self::Like,
            "Move" => Self::Move,
            other => return Err(Error::Invalid(format!("unknown activity kind {other}"))),
        })
    }
}

/// One append-only log entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// Log id (UUIDv7, strictly increasing per store).
    pub id: ActivityId,
    /// What happened.
    pub kind: ActivityKind,
    /// The object this activity is about.
    pub object: ObjectId,
    /// IRI/identifier of the acting person/program.
    pub actor: String,
    /// When it was written.
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
    /// Image before the change (`None` for creation).
    pub before: Option<StoredObject>,
    /// Image after the change (`None` for deletion).
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
