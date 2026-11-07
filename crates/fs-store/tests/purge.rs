//! Log purge of one object: what goes, what stays, and that the bytes leave the file.

mod common;

use common::*;
use fs_store::*;
use serde_json::json;

const SECRET: &str = "SECRET-NEEDLE-0123456789";

#[tokio::test]
async fn removes_every_entry_of_the_object_and_only_those() {
    let s = mem().await;
    let (x, y) = (note("x", SECRET), note("y", "other"));
    s.put(x.clone(), ME).await.unwrap();
    s.put(y.clone(), ME).await.unwrap();
    let mut x2 = x.clone();
    x2.content = json!({"t": "clean"});
    let extra = vec![LogEntry::new(ActivityKind::Accept, json!({"note": SECRET}))];
    s.put_and_log(x2, ME, extra).await.unwrap();
    s.undo(ME, 1).await.unwrap(); // an Undo entry about x (the log-only extra is skipped)
    s.log(
        ActivityKind::Read,
        x.envelope.id,
        "other",
        json!({"seen": SECRET}),
    )
    .await
    .unwrap();
    assert!(log_text(&s).await.contains(SECRET));

    let r = s.purge_object(x.envelope.id).await.unwrap();
    assert_eq!(r.removed, 5); // Create, Update, Accept, Undo, Read
    assert_eq!(r.unsynced_removed, 5);
    let left = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].object, y.envelope.id);
    assert!(!log_text(&s).await.contains(SECRET));
    // The object itself is untouched.
    let now = s.get(x.envelope.id).await.unwrap().unwrap();
    assert_eq!(now.content, json!({"t": SECRET}));
}

#[tokio::test]
async fn unknown_and_second_purge_remove_nothing() {
    let s = mem().await;
    let r = s.purge_object(ObjectId::new()).await.unwrap();
    assert_eq!(r, PurgeReport::default());
    let x = note("x", "a");
    s.put(x.clone(), ME).await.unwrap();
    assert_eq!(s.purge_object(x.envelope.id).await.unwrap().removed, 1);
    assert_eq!(s.purge_object(x.envelope.id).await.unwrap().removed, 0);
}

#[tokio::test]
async fn synced_entries_are_counted_apart() {
    let s = mem().await;
    let x = note("x", "a");
    let first = s.put(x.clone(), ME).await.unwrap();
    s.mark_synced(first.id).await.unwrap();
    s.put(note("x2", "b"), ME).await.unwrap();
    let mut x2 = x.clone();
    x2.content = json!({"t": "b"});
    s.put(x2, ME).await.unwrap();
    let r = s.purge_object(x.envelope.id).await.unwrap();
    assert_eq!((r.removed, r.unsynced_removed), (2, 1));
    assert!(r.highest_removed.unwrap() > first.id);
}

#[tokio::test]
async fn the_bytes_leave_the_database_and_the_write_ahead_log() {
    let dir = tempfile::tempdir().unwrap();
    let s = TursoStore::open(dir.path().join("p.db")).await.unwrap();
    let mut x = note("x", &format!("{SECRET}{}", "z".repeat(20_000)));
    s.put(x.clone(), ME).await.unwrap();
    x.content = json!({"t": "clean"});
    s.put(x.clone(), ME).await.unwrap();
    s.put(note("y", "kept"), ME).await.unwrap();
    assert!(
        needle_in_files(dir.path(), SECRET) > 0,
        "the test needs a needle to find"
    );
    s.purge_object(x.envelope.id).await.unwrap();
    assert_eq!(needle_in_files(dir.path(), SECRET), 0);
    // Everything else still reads.
    assert_eq!(s.count(&Query::new().of_type("Note")).await.unwrap(), 2);
    assert_eq!(s.log_stats().await.unwrap().entries, 1);
    drop(s);
    let s = TursoStore::open(dir.path().join("p.db")).await.unwrap();
    assert_eq!(s.count(&Query::new().of_type("Note")).await.unwrap(), 2);
}

#[tokio::test]
async fn an_encrypted_store_purges_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("e.db");
    let opts = || OpenOptions {
        key_provider: Some(std::sync::Arc::new(StaticKeyProvider(key()))),
        ..OpenOptions::default()
    };
    let s = TursoStore::open_with(&path, opts()).await.unwrap();
    let x = note("x", SECRET);
    s.put(x.clone(), ME).await.unwrap();
    s.purge_object(x.envelope.id).await.unwrap();
    drop(s);
    let s = TursoStore::open_with(&path, opts()).await.unwrap();
    assert!(s.get(x.envelope.id).await.unwrap().is_some());
    assert_eq!(s.log_stats().await.unwrap().entries, 0);
}
