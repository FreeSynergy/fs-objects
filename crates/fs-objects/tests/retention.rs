//! Sync cursor, retention and compaction.

use std::time::Duration;

use fs_objects::*;
use serde_json::json;

mod common;
use common::*;

#[tokio::test]
async fn sync_cursor_and_compaction_keep_unsynced() {
    let s = mem().await;
    let o = card("Kim");
    let id = o.envelope.id;
    let mut last = None;
    for i in 0..10 {
        let mut v = o.clone();
        v.content = json!({"v": i});
        last = Some(s.put(v, ME).await.unwrap());
    }
    s.set_retention(Retention::steps(3)).await.unwrap();
    assert_eq!(s.retention().await.unwrap(), Retention::steps(3));
    // nothing synced: nothing removed
    let r = s.compact().await.unwrap();
    assert_eq!(
        r,
        CompactionReport {
            removed: 0,
            kept_unsynced: 7,
            remaining: 10
        }
    );
    // hub acknowledges the first 6
    let log = s
        .activities(&ActivityFilter {
            unsynced_only: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(log.len(), 10);
    assert_eq!(s.mark_synced(log[5].id).await.unwrap(), 6);
    let r = s.compact().await.unwrap();
    assert_eq!(r.removed, 6);
    assert_eq!(r.kept_unsynced, 1);
    assert_eq!(r.remaining, 4);
    // state unchanged; remaining history still undoable
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 9}));
    let rest = s
        .activities(&ActivityFilter {
            after: Some(log[5].id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rest.len(), 4);
    assert_eq!(rest.last().unwrap().id, last.unwrap().id);
    s.undo(ME, 4).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 5}));
}

#[tokio::test]
async fn compaction_by_age_and_both() {
    let s = mem().await;
    let o = card("Lia");
    for i in 0..4 {
        let mut v = o.clone();
        v.content = json!({"v": i});
        s.put(v, ME).await.unwrap();
    }
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    s.mark_synced(all[3].id).await.unwrap();
    s.set_retention(Retention::age(Duration::from_secs(30 * 86400)))
        .await
        .unwrap();
    assert_eq!(s.compact().await.unwrap().removed, 0);
    let later = chrono::Utc::now() + chrono::Duration::days(31);
    assert_eq!(s.compact_at(later).await.unwrap().removed, 4);
    // both: steps violated even though young
    for i in 0..5 {
        let mut v = o.clone();
        v.content = json!({"w": i});
        s.put(v, ME).await.unwrap();
    }
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    s.mark_synced(all.last().unwrap().id).await.unwrap();
    s.set_retention(Retention {
        max_age: Some(Duration::from_secs(86400)),
        max_steps: Some(2),
    })
    .await
    .unwrap();
    let r = s.compact().await.unwrap();
    assert_eq!((r.removed, r.remaining), (3, 2));
    // clear history keeps only unsynced
    s.put(o.clone(), ME).await.unwrap();
    let r = s.clear_history().await.unwrap();
    assert_eq!((r.removed, r.remaining), (2, 1));
    assert!(s.get(o.envelope.id).await.unwrap().is_some());
}

#[tokio::test]
async fn persistence_across_reopen_and_retention_persists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.db");
    let o = card("Ola");
    let id = o.envelope.id;
    {
        let s = TursoStore::open(&path).await.unwrap();
        s.put(o.clone(), ME).await.unwrap();
        s.set_retention(Retention::steps(7)).await.unwrap();
    }
    let s = TursoStore::open(&path).await.unwrap();
    assert!(s.get(id).await.unwrap().is_some());
    assert_eq!(s.retention().await.unwrap(), Retention::steps(7));
    // undo survives restart (ADR 0042)
    s.undo(ME, 1).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
    // ids stay monotonic across reopen
    let a = s.put(o, ME).await.unwrap();
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(all.last().unwrap().id, a.id);
    assert!(all.windows(2).all(|w| w[0].id < w[1].id));
}
