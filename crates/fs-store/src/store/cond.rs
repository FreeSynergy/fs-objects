//! Filter vocabulary of queries: [`Op`], [`TimeField`], [`Cond`].
//!
//! Pattern: Interpreter-style data — conditions are plain values; each backend translates them
//! (the Turso backend into SQL), so callers never write backend syntax.

use crate::Timestamp;

/// Comparison of a [`Cond`].
///
/// * `Eq`, `Gt`, `Ge`, `Lt`, `Le`: for index fields compared as **binary text** on the stored value
///   (ISO-8601 UTC strings therefore order chronologically); for time fields as instants;
///   for the name on the normalised sort key.
/// * `Prefix`, `Contains`: **case-insensitive and Unicode-normalised** (both sides go through
///   [`crate::sort_key`]), implemented with `LIKE ... ESCAPE '\'`, so `%`, `_` and `\` in the
///   needle are literals. Not available for time fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Equal.
    Eq,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Starts with (normalised, case-insensitive).
    Prefix,
    /// Contains (normalised, case-insensitive).
    Contains,
}

/// Envelope time field usable in a [`Cond::Time`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeField {
    /// Envelope `start_time`.
    Start,
    /// Envelope `end_time`.
    End,
    /// Envelope `published`.
    Published,
    /// Envelope `updated`.
    Updated,
}

/// One filter condition.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// Some value of the index field `key` satisfies `op value`.
    Index {
        /// Index field name.
        key: String,
        /// Comparison.
        op: Op,
        /// Right-hand side.
        value: String,
    },
    /// The envelope name (compared on its normalised sort key).
    Name {
        /// Comparison.
        op: Op,
        /// Right-hand side.
        value: String,
    },
    /// Envelope `describes` (exact for `Eq`, otherwise as `Op` documents).
    Describes {
        /// Comparison (`Eq` only).
        op: Op,
        /// Right-hand side.
        value: String,
    },
    /// An envelope timestamp. Objects with no value for the field never match.
    Time {
        /// Which timestamp.
        field: TimeField,
        /// Comparison (not `Prefix`/`Contains`).
        op: Op,
        /// Right-hand side.
        value: Timestamp,
    },
}
