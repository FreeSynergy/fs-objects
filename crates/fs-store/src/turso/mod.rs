//! Embedded Turso backend of the [`Store`](crate::Store) traits.
//!
//! Pattern: Adapter — `turso` (a foreign SQL engine) behind our `Store` traits; one module per
//! concern so each trait impl stays small. See `DESIGN.md` for the pattern map.

mod activities;
mod codec;
mod compaction;
mod encryption;
mod fault;
mod log_impl;
mod migration;
mod objects;
mod open;
mod options;
mod query_impl;
mod query_sql;
mod repository;
mod retention;
mod rows;
mod schema;
mod sql;
mod store;
mod subscribe;
mod tx;
mod undo;
mod unlogged;
mod validate;

pub use options::OpenOptions;
pub use store::TursoStore;
