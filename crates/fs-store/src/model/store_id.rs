//! [`StoreId`]: the identity of one store, a random UUID made once when the store is created.
//!
//! Pattern: Newtype — a store identity cannot be mixed up with an object or activity id.
//!
//! The id is not a secret, but it makes a server recognisable. It is never written to the activity log
//! and so never reaches the hub on its own; do not put it into the hub, an object or any content unless
//! someone decided that. A copy of the database file keeps it; a new store gets a new one.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

/// Identity of a store (random UUID, version 4: it must not carry a creation time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StoreId(Uuid);

impl StoreId {
    /// A fresh random identity. Only the backends call this, once per new store.
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// The underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for StoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

impl FromStr for StoreId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let u = Uuid::parse_str(s).map_err(|e| Error::Invalid(e.to_string()))?;
        if u.get_version_num() == 4 {
            Ok(Self(u))
        } else {
            Err(Error::Invalid(format!("{u} is not a UUIDv4")))
        }
    }
}
