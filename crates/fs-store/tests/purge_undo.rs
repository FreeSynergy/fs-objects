//! A purge must leave undo and redo of the other objects as they were.

mod common;

use common::*;
use fs_store::*;
use serde_json::json;

fn text(o: &StoredObject) -> String {
    o.content["t"].as_str().unwrap().to_string()
}

async fn set(s: &TursoStore, o: &StoredObject, t: &str) {
    let mut n = o.clone();
    n.content = json!({ "t": t });
    s.put(n, ME).await.unwrap();
}

async fn now(s: &TursoStore, o: &StoredObject) -> String {
    text(&s.get(o.envelope.id).await.unwrap().unwrap())
}

#[tokio::test]
async fn undo_and_redo_of_the_other_object_work_across_the_purge() {
    let s = mem().await;
    let (x, y) = (note("x", "x1"), note("y", "y1"));
    s.put(x.clone(), ME).await.unwrap();
    s.put(y.clone(), ME).await.unwrap();
    set(&s, &x, "x2").await;
    set(&s, &y, "y2").await;
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 4);
    s.purge_object(x.envelope.id).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 2);
    s.undo(ME, 2).await.unwrap();
    assert!(s.get(y.envelope.id).await.unwrap().is_none());
    s.redo(ME, 2).await.unwrap();
    assert_eq!(now(&s, &y).await, "y2");
    assert_eq!(now(&s, &x).await, "x2");
}

#[tokio::test]
async fn a_redo_entry_of_the_other_object_survives_when_the_purged_object_was_undone_after_it() {
    let s = mem().await;
    let (x, y) = (note("x", "x1"), note("y", "y1"));
    s.put(y.clone(), ME).await.unwrap();
    set(&s, &y, "y2").await;
    s.put(x.clone(), ME).await.unwrap();
    s.undo(ME, 2).await.unwrap(); // x's Create, then y's second write; redo stack: [x, y2]
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 2);
    s.purge_object(x.envelope.id).await.unwrap();
    // With the Undo of x left behind without its target, history::compute would have dropped y's redo.
    let st = s.undo_state(ME).await.unwrap();
    assert_eq!((st.undoable, st.redoable), (1, 1));
    assert_eq!(now(&s, &y).await, "y1");
    s.redo(ME, 1).await.unwrap();
    assert_eq!(now(&s, &y).await, "y2");
    s.undo(ME, 2).await.unwrap();
    assert!(s.get(y.envelope.id).await.unwrap().is_none());
}

#[tokio::test]
async fn new_work_on_the_purged_object_no_longer_clears_an_older_redo() {
    // Documented effect: x's write cleared y's redo; with x's entries gone, that redo is available again.
    // It is still checked against the object (Conflict if y changed), so it cannot do wrong.
    let s = mem().await;
    let (x, y) = (note("x", "x1"), note("y", "y1"));
    s.put(y.clone(), ME).await.unwrap();
    set(&s, &y, "y2").await;
    s.undo(ME, 1).await.unwrap();
    s.put(x.clone(), ME).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 0);
    s.purge_object(x.envelope.id).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 1);
    s.redo(ME, 1).await.unwrap();
    assert_eq!(now(&s, &y).await, "y2");
}

#[tokio::test]
async fn a_changed_object_still_conflicts_after_the_purge() {
    let s = mem().await;
    let (x, y) = (note("x", "x1"), note("y", "y1"));
    s.put(y.clone(), ME).await.unwrap();
    set(&s, &y, "y2").await;
    s.put(x.clone(), ME).await.unwrap();
    s.purge_object(x.envelope.id).await.unwrap();
    let mut by_other = y.clone();
    by_other.content = json!({"t": "y3"});
    s.put(by_other, "someone else").await.unwrap();
    assert!(matches!(s.undo(ME, 1).await, Err(Error::Conflict(_))));
    assert_eq!(now(&s, &y).await, "y3");
}

#[tokio::test]
async fn a_write_after_the_purge_is_new_work_and_clears_the_redo_stack() {
    let s = mem().await;
    let (x, c) = (note("x", "x1"), note("c", "c1"));
    s.put(x.clone(), ME).await.unwrap();
    s.purge_object(x.envelope.id).await.unwrap();
    s.put(c.clone(), ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 1);
    set(&s, &x, "x2").await; // redacted entry
    let st = s.undo_state(ME).await.unwrap();
    assert_eq!((st.redoable, st.undoable), (0, 0));
    assert!(s.redo(ME, 1).await.unwrap().is_empty());
    assert!(s.get(c.envelope.id).await.unwrap().is_none());
}
