//! Pure undo/redo stack computation over log entries (backend-independent).
//!
//! Pattern: Command (history) — folds the recorded commands into undo/redo stacks; pure, so it
//! is unit-tested without a database.

use std::collections::HashMap;

use crate::{ActivityId, ActivityKind};

/// The part of a log entry the stack computation needs.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    /// Id of the log entry.
    pub id: ActivityId,
    /// Kind of the log entry.
    pub kind: ActivityKind,
    /// For `Undo`: the reverted activity.
    pub target: Option<ActivityId>,
    /// Written without images after a purge (ADR 0029): not undoable, but it is new work, so it clears redo.
    pub redacted: bool,
}

/// Undo/redo stacks of one actor; the last element is the top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stacks {
    /// Activities that `undo` would revert next.
    pub undo: Vec<ActivityId>,
    /// `(activity, the Undo that reverted it)`; `redo` reverts the `Undo`.
    pub redo: Vec<(ActivityId, ActivityId)>,
}

/// Compute the stacks from one actor's steps in log (id) order.
///
/// * `Create`/`Update`/`Delete` push onto undo and clear redo (new work forks history).
/// * `Undo` of a regular activity moves it from undo to redo.
/// * `Undo` of an `Undo` (= redo) moves the original back to undo.
/// * An `Undo` whose target is no longer in the log (compacted) is ignored, and the
///   redo entries below it are dropped (they could only be redone through that target).
pub fn compute(steps: &[Step]) -> Stacks {
    let mut s = Stacks::default();
    // Undo entries seen so far: undo-id -> the activity it reverted.
    let mut undone_by: HashMap<ActivityId, ActivityId> = HashMap::new();
    for st in steps {
        match st.kind {
            ActivityKind::Create | ActivityKind::Update | ActivityKind::Delete => {
                if !st.redacted {
                    s.undo.push(st.id);
                }
                s.redo.clear();
            }
            ActivityKind::Undo => {
                let Some(t) = st.target else { continue };
                if let Some(&orig) = undone_by.get(&t) {
                    // redo: `t` was an Undo of `orig`
                    if let Some(pos) = s.redo.iter().position(|(a, u)| *a == orig && *u == t) {
                        s.redo.remove(pos);
                        s.undo.push(orig);
                    }
                } else if let Some(pos) = s.undo.iter().position(|a| *a == t) {
                    s.undo.remove(pos);
                    s.redo.push((t, st.id));
                    undone_by.insert(st.id, t);
                } else {
                    // The target was compacted away. Everything undone before this step
                    // would have to be redone *after* it (stack order), which is impossible
                    // now, so those redo entries are dropped.
                    s.redo.clear();
                }
            }
            _ => {}
        }
    }
    s
}
