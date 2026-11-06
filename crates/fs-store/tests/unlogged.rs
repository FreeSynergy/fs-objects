//! Writes without the activity log leave no entry, no event and no store-made time.

mod common;

use chrono::{TimeZone, Utc};
use common::obj;
use fs_store::traits::UnloggedObjects;
use fs_store::*;

async fn store() -> TursoStore {
    TursoStore::open_in_memory().await.unwrap()
}

#[tokio::test]
async fn no_activity_and_no_event_at_any_moment() {
    let s = store().await;
    let mut feed = s.subscribe();
    let o = obj("Box", "one");
    let id = o.envelope.id;
    s.put_unlogged(o.clone()).await.unwrap();
    assert_eq!(s.log_stats().await.unwrap().entries, 0);
    s.put_unlogged(obj_with_id(&o, "two")).await.unwrap();
    assert_eq!(s.log_stats().await.unwrap().entries, 0);
    s.delete_unlogged(id).await.unwrap();
    assert_eq!(s.log_stats().await.unwrap().entries, 0);
    assert!(feed.try_recv().is_err());
}

fn obj_with_id(o: &StoredObject, name: &str) -> StoredObject {
    let mut n = o.clone();
    n.envelope.name = name.into();
    n
}

#[tokio::test]
async fn times_stay_as_given() {
    let s = store().await;
    let mut o = obj("Box", "one");
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    o.envelope.published = epoch;
    o.envelope.updated = epoch;
    let id = o.envelope.id;
    s.put_unlogged(o.clone()).await.unwrap();
    s.put_unlogged(obj_with_id(&o, "two")).await.unwrap();
    let back = s.read_unlogged(id).await.unwrap().unwrap();
    assert_eq!(back.envelope.name, "two");
    assert_eq!(back.envelope.published, epoch);
    assert_eq!(back.envelope.updated, epoch);
}

#[tokio::test]
async fn replacing_leaves_one_object_and_undo_has_nothing() {
    let s = store().await;
    let o = obj("Box", "one").with_index("k", "a");
    s.put_unlogged(o.clone()).await.unwrap();
    let mut n = obj_with_id(&o, "two");
    n.index.clear();
    let n = n.with_index("k", "b");
    s.put_unlogged(n).await.unwrap();
    assert_eq!(s.count(&Query::new().of_type("Box")).await.unwrap(), 1);
    assert_eq!(
        s.count(&Query::new().with_index("k", "a")).await.unwrap(),
        0
    );
    assert_eq!(s.undo_state(common::ME).await.unwrap().undoable, 0);
}

#[tokio::test]
async fn deleting_what_is_not_there_is_fine() {
    let s = store().await;
    s.delete_unlogged(ObjectId::new()).await.unwrap();
    assert!(s.read_unlogged(ObjectId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn sensitive_is_refused_without_encryption() {
    let dir = tempfile::tempdir().unwrap();
    let s = TursoStore::open(dir.path().join("plain.db")).await.unwrap();
    let mut o = obj("Box", "one");
    o.envelope.sensitive = vec!["dpv:HealthData".to_string()];
    assert!(matches!(
        s.put_unlogged(o).await,
        Err(Error::EncryptionRequired(_))
    ));
}

#[tokio::test]
async fn an_id_with_logged_history_is_refused() {
    let s = store().await;
    let o = obj("Box", "one");
    s.put(o.clone(), common::ME).await.unwrap();
    assert!(matches!(s.put_unlogged(o).await, Err(Error::Invalid(_))));
}
