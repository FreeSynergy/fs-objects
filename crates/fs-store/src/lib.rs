//! `fs-store`: the object store every FreeSynergy program embeds.
//!
//! An object is an ActivityStreams 2.0 envelope + domain content (JSContact,
//! JSCalendar, ...) + optional untouched original; every change is recorded
//! in an append-only activity log in the same transaction (ADR 0019, 0020, 0030).
//!
//! Pattern: Facade/Strategy — programs depend on the [`Store`] trait (composed of focused
//! traits, see [`traits`]); [`TursoStore`] is the embedded backend. See `DESIGN.md` for the pattern map.

#![deny(missing_docs)]

mod crypto;
mod error;
pub mod history;
mod model;
mod store;
mod text;
pub mod traits;
mod turso;

pub use crypto::{Cipher, EncryptionKey, KeyProvider, StaticKeyProvider};
pub use error::{Error, Result};
pub use model::{
    Activity, ActivityId, ActivityKind, Blob, ChangeSet, DpvCategory, Envelope, IndexFields,
    ObjectId, StoredObject, Timestamp,
};
pub use store::{
    ActivityFilter, CompactionReport, Cond, LogEntry, LogStats, Op, Order, Query, Retention, Store,
    TimeField, UndoState,
};
pub(crate) use store::{
    ActivityLog, ChangeFeed, HistoryRetention, ObjectQuery, ObjectRepository, UndoRedo,
};
pub use text::sort_key;
pub use turso::{OpenOptions, TursoStore};
