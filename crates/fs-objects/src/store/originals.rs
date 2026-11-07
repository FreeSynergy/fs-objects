//! [`LargeOriginals`]: store large originals once and let the activity log only reference them.
//!
//! Pattern: Strategy (policy, interface segregation) — a separate narrow trait outside the [`Store`](crate::Store)
//! facade: the threshold is a policy a program sets, and backends that cannot do it keep compiling. Its
//! data types are value objects. **State: draft, `Unsupported`** — the trait and the tests exist, the
//! Turso backend answers "not supported" (see `DESIGN.md`, "Large originals").

use async_trait::async_trait;

use crate::{Error, ObjectId, Result};

/// Where a large original lives: a content hash into the blob table, not bytes in the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalRef {
    /// SHA-256 of the bytes, lower-case hex. Equal bytes are stored once.
    pub hash: String,
    /// Size in bytes.
    pub size: u64,
    /// Media type of the original.
    pub media_type: String,
}

/// Large originals stored once, the log keeping only an [`OriginalRef`]. Only for **encrypted** stores:
/// the blob table sits in the same encrypted file, but a plain store would keep content that the log
/// purge cannot reach by object id alone (see `DESIGN.md`).
#[async_trait]
pub trait LargeOriginals: Send + Sync {
    /// Originals of at least this many bytes go to the blob table. `None` (the default of a store) keeps
    /// every original inline in the log as before.
    async fn original_threshold(&self) -> Result<Option<usize>> {
        Err(unsupported())
    }

    /// Set the threshold. A plain (unencrypted) store refuses a value other than `None` with
    /// `Error::Invalid`. Only originals written afterwards are affected.
    async fn set_original_threshold(&self, bytes: Option<usize>) -> Result<()> {
        let _ = bytes;
        Err(unsupported())
    }

    /// The reference of an object's original, if it is kept in the blob table.
    async fn original_ref(&self, id: ObjectId) -> Result<Option<OriginalRef>> {
        let _ = id;
        Err(unsupported())
    }
}

fn unsupported() -> Error {
    Error::Backend("large originals are not supported by this store".into())
}
