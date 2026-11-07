//! [`UndoRedo`]: per-actor persistent undo and redo.
//!
//! Pattern: Command — undo/redo apply the inverse of recorded activities.

use async_trait::async_trait;

use crate::{Activity, Result};

/// Undo/redo depth for one actor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UndoState {
    /// Activities `undo` could revert.
    pub undoable: usize,
    /// Undone activities `redo` could restore.
    pub redoable: usize,
}

/// Undo and redo of an actor's changes.
#[async_trait]
pub trait UndoRedo: Send + Sync {
    /// Undo the last `n` undoable activities of `actor` (most recent first), atomically.
    /// Returns the new `Undo` activities. Fewer than `n` may be possible: returns what was done.
    /// Fails with `Conflict` (and changes nothing) if the object was changed since.
    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;

    /// Redo the last `n` undone activities of `actor` (an `Undo` of an `Undo`).
    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>>;

    /// How many undo/redo steps `actor` has.
    async fn undo_state(&self, actor: &str) -> Result<UndoState>;
}
