//! Pure undo/redo stack computation over log entries (backend-independent).

use std::collections::HashMap;

use crate::{ActivityId, ActivityKind};

/// The part of a log entry the stack computation needs.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub id: ActivityId,
    pub kind: ActivityKind,
    pub target: Option<ActivityId>,
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
/// * Targets that are no longer in the log (compacted) are ignored.
pub fn compute(steps: &[Step]) -> Stacks {
    let mut s = Stacks::default();
    // Undo entries seen so far: undo-id -> the activity it reverted.
    let mut undone_by: HashMap<ActivityId, ActivityId> = HashMap::new();
    for st in steps {
        match st.kind {
            ActivityKind::Create | ActivityKind::Update | ActivityKind::Delete => {
                s.undo.push(st.id);
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
                }
                // else: target compacted away -> ignored
            }
            _ => {}
        }
    }
    s
}
