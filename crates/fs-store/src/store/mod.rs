//! Backend-neutral store contract: focused traits composed into [`Store`].
//!
//! Pattern: Strategy — the traits are the strategy interface; backends (Turso now, a remote
//! JSON/HTTPS one later) implement them. Only declarations and re-exports live here.

mod cond;
mod facade;
mod feed;
mod log;
mod purge;
mod query;
mod repository;
mod retention;
mod search;
mod undo;
mod unlogged;

pub use cond::{Cond, Op, TimeField};
pub use facade::Store;
pub use feed::ChangeFeed;
pub use log::{ActivityFilter, ActivityLog, LogEntry};
pub use purge::PurgeReport;
pub use query::{Order, Query};
pub use repository::ObjectRepository;
pub use retention::{CompactionReport, HistoryRetention, LogStats, Retention};
pub use search::ObjectQuery;
pub use undo::{UndoRedo, UndoState};
pub use unlogged::UnloggedObjects;
