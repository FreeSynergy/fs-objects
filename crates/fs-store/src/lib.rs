//! `fs-store`: the object store every FreeSynergy program embeds.
//!
//! An object is an ActivityStreams 2.0 envelope + domain content (JSContact,
//! JSCalendar, ...) + optional untouched original; every change is recorded
//! in an append-only activity log in the same transaction (ADR 0019, 0020, 0030). The one
//! exception is the separate trait [`traits::UnloggedObjects`], for data that must not carry a time.
//!
//! Pattern: Facade/Strategy — programs depend on the [`Store`] trait (composed of focused
//! traits, see [`traits`]); [`TursoStore`] is the embedded backend. See `DESIGN.md` for the pattern map.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]
// Pedantic lints that would only add noise to this crate's small, documented API:
// every fallible fn returns the crate `Result` (documented once on `Error`), results of pure
// getters/builders are used by callers anyway, and the SQL layer converts between the
// database's `i64` and in-memory sizes where values are bounded by construction.
#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

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
    UnloggedObjects,
};
pub use text::sort_key;
pub use turso::{OpenOptions, TursoStore};
