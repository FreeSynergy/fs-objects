//! Undo/redo behaviour.

use fs_objects::*;
use serde_json::json;

mod common;
use common::*;

#[tokio::test]
async fn undo_redo_basic() {
    let s = mem().await;
    let o = card("Fay");
    let id = o.envelope.id;
    s.put(o.clone(), ME).await.unwrap();
    let mut v2 = o.clone();
    v2.content = json!({"v": 2});
    s.put(v2, ME).await.unwrap();
    assert_eq!(
        s.undo_state(ME).await.unwrap(),
        UndoState {
            undoable: 2,
            redoable: 0
        }
    );

    let u = s.undo(ME, 1).await.unwrap();
    assert_eq!(u[0].kind, ActivityKind::Undo);
    assert_eq!(s.get(id).await.unwrap().unwrap().content, o.content);
    s.redo(ME, 1).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 2}));
    // undo all -> object gone; redo all -> back
    s.undo(ME, 5).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
    assert_eq!(
        s.undo_state(ME).await.unwrap(),
        UndoState {
            undoable: 0,
            redoable: 2
        }
    );
    s.redo(ME, 5).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 2}));
    // new work clears redo
    s.undo(ME, 1).await.unwrap();
    let mut v3 = o.clone();
    v3.content = json!({"v": 3});
    s.put(v3, ME).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 0);
}

#[tokio::test]
async fn undo_is_per_actor_and_detects_conflict() {
    let s = mem().await;
    let o = card("Gus");
    let id = o.envelope.id;
    s.put(o.clone(), "alice").await.unwrap();
    let mut v2 = o.clone();
    v2.content = json!({"v": 2});
    s.put(v2, "bob").await.unwrap();
    // alice cannot undo her create: bob changed the object since
    assert!(matches!(s.undo("alice", 1).await, Err(Error::Conflict(_))));
    assert!(s.get(id).await.unwrap().is_some());
    // nothing for carol
    assert!(s.undo("carol", 3).await.unwrap().is_empty());
    s.undo("bob", 1).await.unwrap();
    s.undo("alice", 1).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
}

#[tokio::test]
async fn undo_after_compaction_of_target_is_graceful() {
    let s = mem().await;
    let o = card("Max");
    s.put(o.clone(), ME).await.unwrap();
    let mut v = o.clone();
    v.content = json!({"v": 2});
    s.put(v, ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    s.mark_synced(all.last().unwrap().id).await.unwrap();
    s.clear_history().await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap(), UndoState::default());
    assert!(s.undo(ME, 3).await.unwrap().is_empty());
}
