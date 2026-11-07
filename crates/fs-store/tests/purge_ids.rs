//! After a purge: ids, the sync marks, later writes, and a failure in the middle.

mod common;

use common::*;
use fs_store::*;
use serde_json::json;

const SECRET: &str = "SECRET-NEEDLE-0123456789";

#[tokio::test]
async fn the_hole_in_the_middle_does_not_confuse_sync_marks_or_compaction() {
    let s = mem().await;
    let (x, y) = (note("x", "a"), note("y", "b"));
    let a = s.put(y.clone(), ME).await.unwrap();
    let b = s.put(x.clone(), ME).await.unwrap();
    let c = s.put(note("y2", "c"), ME).await.unwrap();
    let d = s
        .log(ActivityKind::Read, x.envelope.id, ME, json!({}))
        .await
        .unwrap();
    let e = s.put(note("y3", "d"), ME).await.unwrap();
    s.mark_synced(b.id).await.unwrap(); // a, b synced
    s.purge_object(x.envelope.id).await.unwrap(); // b, d gone
    let ids = |v: Vec<Activity>| v.into_iter().map(|a| a.id).collect::<Vec<_>>();
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(ids(all), vec![a.id, c.id, e.id]);
    // the hub's cursor: after the last id it accepted (b), it gets c and e
    let f = ActivityFilter {
        after: Some(b.id),
        ..ActivityFilter::default()
    };
    assert_eq!(ids(s.activities(&f).await.unwrap()), vec![c.id, e.id]);
    // marking up to d (removed) marks c only, nothing else
    assert_eq!(s.mark_synced(d.id).await.unwrap(), 1);
    let un = ActivityFilter {
        unsynced_only: true,
        ..ActivityFilter::default()
    };
    assert_eq!(ids(s.activities(&un).await.unwrap()), vec![e.id]);
    // compaction removes the synced ones on both sides of the old hole and keeps the unsynced one
    let r = s.clear_history().await.unwrap();
    assert_eq!((r.removed, r.remaining), (2, 1));
    assert_eq!(
        ids(s.activities(&ActivityFilter::default()).await.unwrap()),
        vec![e.id]
    );
}

#[tokio::test]
async fn ids_stay_above_what_was_handed_out_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("h.db");
    let x = note("x", "a");
    let newest = {
        let s = TursoStore::open(&path).await.unwrap();
        s.put(note("y", "b"), ME).await.unwrap();
        let n = s.put(x.clone(), ME).await.unwrap();
        s.purge_object(x.envelope.id).await.unwrap();
        n.id
    };
    let high = raw_sql(&path, "SELECT value FROM meta WHERE key = 'activity_high'").await;
    assert_eq!(high, vec![newest.to_string()]);
    // A hub that accepted an id from the future: the next id must still be above it.
    let future = ActivityId::next_after(Some(newest)).as_uuid().as_u128() + (1u128 << 80);
    let future = uuid::Uuid::from_u128(future).to_string();
    let sql = format!("UPDATE meta SET value = '{future}' WHERE key = 'activity_high'");
    raw_sql(&path, &sql).await;
    let s = TursoStore::open(&path).await.unwrap();
    let next = s.put(note("z", "c"), ME).await.unwrap();
    assert!(next.id.to_string() > future, "{} <= {future}", next.id);
}

#[tokio::test]
async fn later_writes_on_a_purged_object_log_without_images_and_are_not_undoable() {
    let s = mem().await;
    let x = note("x", "old");
    s.put(x.clone(), ME).await.unwrap();
    s.purge_object(x.envelope.id).await.unwrap();
    let mut n = x.clone();
    n.content = json!({"t": SECRET});
    let upd = s.put(n.clone(), ME).await.unwrap();
    assert_eq!(upd.kind, ActivityKind::Update);
    assert!(upd.is_redacted());
    assert!(!log_text(&s).await.contains(SECRET));
    assert!(!log_text(&s).await.contains("old"));
    let del = s.delete(x.envelope.id, ME).await.unwrap();
    assert!(del.is_redacted());
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 0);
    assert!(s.undo(ME, 1).await.unwrap().is_empty());
}

#[tokio::test]
async fn other_objects_keep_their_images() {
    let s = mem().await;
    let (x, y) = (note("x", "a"), note("y", "b"));
    s.put(x.clone(), ME).await.unwrap();
    s.purge_object(x.envelope.id).await.unwrap();
    let a = s.put(y.clone(), ME).await.unwrap();
    assert!(a.change_set().unwrap().after.is_some());
}

#[tokio::test]
async fn a_failure_before_the_commit_changes_nothing() {
    let s = mem().await;
    let x = note("x", SECRET);
    s.put(x.clone(), ME).await.unwrap();
    s.inject_fault_in_purge(true);
    assert!(s.purge_object(x.envelope.id).await.is_err());
    assert!(
        log_text(&s).await.contains(SECRET),
        "the entries must still be there"
    );
    // and the object was not marked: a later write still logs images
    let mut n = x.clone();
    n.content = json!({"t": "next"});
    let upd = s.put(n, ME).await.unwrap();
    assert!(upd.change_set().unwrap().after.is_some());
    assert_eq!(s.purge_object(x.envelope.id).await.unwrap().removed, 2);
}
