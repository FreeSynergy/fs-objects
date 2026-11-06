//! Newtype identifiers: [`ObjectId`] and [`ActivityId`] (UUIDv7, ADR 0027).
//!
//! Pattern: Newtype — distinct id types so an object id can never be passed where an activity id
//! is expected; the `uuid_id!` macro keeps both definitions identical.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

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
