//! `fs-store`: the object store every FreeSynergy program embeds.
//!
//! An object is an ActivityStreams 2.0 envelope + domain content (JSContact,
//! JSCalendar, ...) + optional untouched original; every change is recorded
//! in an append-only activity log in the same transaction (ADR 0019, 0020, 0030).
//! See `DESIGN.md`.

mod crypto;
mod error;
pub mod history;
mod model;
mod store;
mod turso_store;

pub use crypto::{Cipher, EncryptionKey, KeyProvider, StaticKeyProvider};
pub use error::{Error, Result};
pub use model::{
    Activity, ActivityId, ActivityKind, Blob, ChangeSet, DpvCategory, Envelope, IndexFields,
    ObjectId, StoredObject, Timestamp,
};
pub use store::{
    ActivityFilter, CompactionReport, LogStats, Order, Query, Retention, Store, UndoState,
};
pub use turso_store::{OpenOptions, TursoStore};
