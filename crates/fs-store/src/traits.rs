//! The focused store traits that [`Store`](crate::Store) is composed of.
//!
//! Pattern: Strategy (interface segregation) — a consumer that only reads can depend on
//! `dyn ObjectQuery`, one that only listens on `dyn ChangeFeed`. They live in their own module
//! (not the crate root) on purpose: `Store` repeats their methods as a facade, so importing both
//! `Store` and a focused trait for the same concrete type makes calls ambiguous. Import the
//! focused traits only where you do not import `Store`.

pub use crate::store::{
    ActivityLog, ChangeFeed, HistoryRetention, ObjectQuery, ObjectRepository, UndoRedo,
};
