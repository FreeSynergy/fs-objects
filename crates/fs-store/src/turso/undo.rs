//! [`UndoRedo`] for the Turso backend.
//!
//! Pattern: Command — an activity is a recorded command with before/after images; undo applies
//! its inverse as a new `Undo` activity, redo is an `Undo` of an `Undo`. The stacks come from the
//! pure `history::compute`, so they are persistent.

use async_trait::async_trait;

use super::TursoStore;
use super::activities::{load_activity, load_steps};
use super::objects::load_object;
use super::store::{Change, Inner};
use super::tx::in_tx;
use crate::{Activity, ActivityId, ActivityKind, Error, Result, UndoRedo, UndoState, history};

impl TursoStore {
    /// Revert activity `x_id` by writing its `before` image as a new `Undo` activity.
    async fn revert(&self, inner: &mut Inner, x_id: ActivityId, actor: &str) -> Result<Activity> {
        let x = load_activity(&inner.conn, x_id)
            .await?
            .ok_or_else(|| Error::Conflict(format!("activity {x_id} no longer in the log")))?;
        let cs = x.change_set()?;
        let current = load_object(&inner.conn, x.object).await?;
        let consistent = match (&current, &cs.after) {
            (None, None) => true,
            (Some(c), Some(a)) => c.matches_snapshot(a),
            _ => false,
        };
        if !consistent {
            return Err(Error::Conflict(format!(
                "object {} changed since activity {x_id}",
                x.object
            )));
        }
        let mut restored = cs.before;
        if let (Some(r), Some(c)) = (restored.as_mut(), current.as_ref()) {
            if r.original.is_none() {
                r.original = c.original.clone();
            }
        }
        let ch = Change {
            kind: ActivityKind::Undo,
            object: x.object,
            target: Some(x_id),
            before: current,
            after: restored,
        };
        self.apply(inner, actor, ch).await
    }

    async fn step_back(&self, actor: &str, n: usize, redo: bool) -> Result<Vec<Activity>> {
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let done = in_tx(&conn, async {
            let mut done = Vec::new();
            for _ in 0..n {
                let stacks = history::compute(&load_steps(&inner.conn, actor).await?);
                let target = if redo {
                    stacks.redo.last().map(|(_, u)| *u)
                } else {
                    stacks.undo.last().copied()
                };
                let Some(t) = target else { break };
                done.push(self.revert(inner, t, actor).await?);
            }
            Ok(done)
        })
        .await?;
        self.publish(&done);
        Ok(done)
    }
}

#[async_trait]
impl UndoRedo for TursoStore {
    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        self.step_back(actor, n, false).await
    }

    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        self.step_back(actor, n, true).await
    }

    async fn undo_state(&self, actor: &str) -> Result<UndoState> {
        let inner = self.inner.lock().await;
        let s = history::compute(&load_steps(&inner.conn, actor).await?);
        Ok(UndoState {
            undoable: s.undo.len(),
            redoable: s.redo.len(),
        })
    }
}
