//! [`StoreInfo`]: what a store says about itself.
//!
//! Pattern: Strategy (interface segregation) — a consumer that only needs to recognise a store or to
//! check the encryption requirement (ADR 0018: messenger data only on encrypted stores) depends on this
//! one small trait, not on the whole [`Store`](crate::Store).

use crate::StoreId;

/// Identity and protection of a store.
pub trait StoreInfo: Send + Sync {
    /// The identity: a random UUID made once when the store was created, never changed, never in the
    /// activity log. A file copy keeps it, a new store gets another. It is no secret but makes the server
    /// recognisable: do not send it to the hub or put it into content unless that was decided.
    fn identity(&self) -> StoreId;

    /// True if the store is encrypted at rest.
    fn is_encrypted(&self) -> bool;
}
