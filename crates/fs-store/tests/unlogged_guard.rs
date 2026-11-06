//! A logged call on an id written unlogged is refused, and nothing reaches the log.

mod common;

use common::{ME, obj};
use fs_store::traits::UnloggedObjects;
use fs_store::*;

const SECRET: &str = "PRIVATE-KEY-MATERIAL";

fn secret_box() -> StoredObject {
    let mut o = obj("Box", "keys");
    o.content = serde_json::json!({ "secret": SECRET });
    o
}

async fn log_text(s: &TursoStore) -> String {
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    serde_json::to_string(&all).unwrap()
}

#[tokio::test]
async fn every_logged_call_is_refused_and_the_log_stays_empty() {
    let s = TursoStore::open_in_memory().await.unwrap();
    let o = secret_box();
    let id = o.envelope.id;
    s.put_unlogged(o.clone()).await.unwrap();

    assert!(matches!(s.put(o.clone(), ME).await, Err(Error::Invalid(_))));
    assert!(matches!(
        s.put_and_log(o.clone(), ME, vec![]).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(s.delete(id, ME).await, Err(Error::Invalid(_))));
    assert!(matches!(s.get(id).await, Err(Error::Invalid(_))));
    let q = Query::new().of_type("Box");
    assert!(matches!(s.query(&q).await, Err(Error::Invalid(_))));
    let r = s
        .log(ActivityKind::Read, id, ME, serde_json::json!({}))
        .await;
    assert!(matches!(r, Err(Error::Invalid(_))));

    assert_eq!(s.log_stats().await.unwrap().entries, 0);
    assert!(!log_text(&s).await.contains(SECRET));
    // The object is untouched and the unlogged reads still work.
    let back = s.read_unlogged(id).await.unwrap().unwrap();
    assert_eq!(back.content["secret"], SECRET);
}

#[tokio::test]
async fn the_unlogged_flow_itself_keeps_working() {
    let s = TursoStore::open_in_memory().await.unwrap();
    let mut o = secret_box();
    let id = o.envelope.id;
    s.put_unlogged(o.clone()).await.unwrap();
    o.content = serde_json::json!({ "secret": "second" });
    s.put_unlogged(o.clone()).await.unwrap();
    assert_eq!(
        s.read_unlogged(id).await.unwrap().unwrap().content,
        o.content
    );
    s.delete_unlogged(id).await.unwrap();
    assert!(s.read_unlogged(id).await.unwrap().is_none());
    // The id stays reserved for the unlogged path after the delete.
    assert!(matches!(s.put(o, ME).await, Err(Error::Invalid(_))));
}

#[tokio::test]
async fn other_objects_are_not_touched_and_a_query_that_skips_the_box_works() {
    let s = TursoStore::open_in_memory().await.unwrap();
    s.put_unlogged(secret_box()).await.unwrap();
    let note = obj("Note", "n");
    s.put(note.clone(), ME).await.unwrap();
    assert_eq!(
        s.get(note.envelope.id).await.unwrap().unwrap().content,
        note.content
    );
    let hits = s.query(&Query::new().of_type("Note")).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(s.count(&Query::new().of_type("Box")).await.unwrap(), 1);
    assert_eq!(s.log_stats().await.unwrap().entries, 1);
}

#[tokio::test]
async fn the_mark_survives_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.db");
    let o = secret_box();
    {
        let s = TursoStore::open(&path).await.unwrap();
        s.put_unlogged(o.clone()).await.unwrap();
    }
    let s = TursoStore::open(&path).await.unwrap();
    assert!(matches!(s.put(o.clone(), ME).await, Err(Error::Invalid(_))));
    assert!(s.read_unlogged(o.envelope.id).await.unwrap().is_some());
}

#[tokio::test]
async fn a_refused_unlogged_write_leaves_no_mark() {
    let s = TursoStore::open_in_memory().await.unwrap();
    let n = obj("Note", "n");
    s.put(n.clone(), ME).await.unwrap();
    // An id with logged history is refused by the unlogged path ...
    assert!(s.put_unlogged(n.clone()).await.is_err());
    // ... and the failed call has not reserved it away from the logged path.
    s.put(n, ME).await.unwrap();
}
