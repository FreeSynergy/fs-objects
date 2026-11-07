//! [`PurgeReport`]: what a log purge of one object removed.
//!
//! Pattern: none (plain result value of `HistoryRetention::purge_object`).

use crate::ActivityId;

/// Outcome of [`HistoryRetention::purge_object`](crate::traits::HistoryRetention::purge_object).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PurgeReport {
    /// Log entries removed: `Create`, `Update`, `Delete`, `Undo` and log-only extras of the object.
    pub removed: usize,
    /// Of those, entries the hub had not acknowledged. The hub will now never receive them; a hub client
    /// that pushes by cursor does not notice.
    pub unsynced_removed: usize,
    /// The highest id removed, if any. The store remembers the highest id it ever issued, so new
    /// entries stay above it after a reopen.
    pub highest_removed: Option<ActivityId>,
}
