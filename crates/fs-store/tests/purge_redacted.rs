//! Entries written after a purge carry no images and say so, so nobody reads them as a deletion.

use fs_store::*;
use serde_json::json;

mod common;
use common::*;

#[tokio::test]
async fn a_put_after_a_purge_logs_only_the_redacted_mark_never_after_null() {
    let s = mem().await;
    let mut o = note("x", "secret");
    s.put(o.clone(), ME).await.unwrap();
    s.purge_object(o.envelope.id).await.unwrap();
    o.content = json!({ "t": "new" });
    let upd = s.put(o.clone(), ME).await.unwrap();
    assert_eq!(upd.kind, ActivityKind::Update);
    assert_eq!(upd.payload, json!({ "redacted": true }));
    assert!(upd.is_redacted());
    assert!(upd.change_set().is_err());
    let del = s.delete(o.envelope.id, ME).await.unwrap();
    assert_eq!(del.payload, json!({ "redacted": true }));
    // The object itself is intact: only the log lost its images.
    let fresh = note("y", "again");
    s.put(fresh.clone(), ME).await.unwrap();
    s.purge_object(fresh.envelope.id).await.unwrap();
    let create = s.put(fresh, ME).await.unwrap();
    assert!(create.payload.get("after").is_none() && create.payload.get("before").is_none());
}

#[tokio::test]
async fn normal_entries_are_not_marked() {
    let s = mem().await;
    let a = s.put(note("x", "t"), ME).await.unwrap();
    assert!(!a.is_redacted());
    assert!(a.change_set().unwrap().after.is_some());
}

#[tokio::test]
async fn a_busy_file_is_reported_not_an_error_and_clean_files_finishes_later() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.db");
    let s = TursoStore::open(&path).await.unwrap();
    let o = note("x", "NEEDLE-SECRET-TEXT");
    s.put(o.clone(), ME).await.unwrap();
    s.delete(o.envelope.id, ME).await.unwrap(); // the object row would keep the text
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let reader = db.connect().unwrap();
    reader.execute("BEGIN", ()).await.unwrap();
    let mut rows = reader.query("SELECT * FROM activities", ()).await.unwrap();
    while rows.next().await.unwrap().is_some() {}
    let report = s.purge_object(o.envelope.id).await.unwrap();
    assert_eq!(report.removed, 2);
    assert!(!report.file_cleaned);
    assert!(!log_text(&s).await.contains("NEEDLE"));
    drop((rows, reader, db));
    assert!(s.clean_files().await.unwrap());
    assert_eq!(needle_in_files(dir.path(), "NEEDLE-SECRET-TEXT"), 0);
    let again = s.purge_object(o.envelope.id).await.unwrap();
    assert!(again.file_cleaned);
}
