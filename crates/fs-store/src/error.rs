//! Error type shared by all backends.

use crate::ObjectId;

/// Result alias for store operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by a [`Store`](crate::Store).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The object does not exist.
    #[error("object {0} not found")]
    NotFound(ObjectId),
    /// The input violates a store rule (empty `name`, bad activity kind, ...).
    #[error("invalid input: {0}")]
    Invalid(String),
    /// Undo/redo cannot be applied because the object changed in the meantime.
    #[error("conflict: {0}")]
    Conflict(String),
    /// ADR 0030: an original, once stored, is never rewritten.
    #[error("the original of object {0} is immutable")]
    OriginalImmutable(ObjectId),
    /// ADR 0029: sensitive objects need an encrypted store.
    #[error("object {0} is marked sensitive but the store is not encrypted")]
    EncryptionRequired(ObjectId),
    /// Key provider failed or the key is unusable.
    #[error("encryption key error: {0}")]
    Key(String),
    /// (De)serialisation of stored JSON failed.
    #[error("serialisation error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The database backend failed.
    #[error("backend error: {0}")]
    Backend(String),
}

impl From<turso::Error> for Error {
    fn from(e: turso::Error) -> Self {
        Error::Backend(e.to_string())
    }
}
