//! [`PurgeReport`]: what a log purge of one object removed.
//!
//! Pattern: none (plain result value, no behaviour).

use crate::ActivityId;

/// Outcome of [`HistoryRetention::purge_object`](crate::traits::HistoryRetention::purge_object).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PurgeReport {
    /// Log entries removed: `Create`, `Update`, `Delete`, `Undo` and log-only extras of the object.
    pub removed: usize,
    /// Of those, entries the hub had not acknowledged. The hub will now never receive them; a hub client
    /// that pushes by cursor does not notice.
    pub unsynced_removed: usize,
    /// The highest id removed, if any. The store remembers the highest id it ever issued, so new
    /// entries stay above it after a reopen.
    pub highest_removed: Option<ActivityId>,
    /// True if the database file and the write-ahead log were rewritten so the removed bytes are gone from
    /// disk. False if the entries are deleted but another reader held the file: call
    /// `HistoryRetention::clean_files` again later. Always true for a store with no file.
    pub file_cleaned: bool,
}
