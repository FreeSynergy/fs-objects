//! Object CRUD, originals, persistence, object safety.

use fs_store::*;
use serde_json::json;

mod common;
use common::*;

#[tokio::test]
async fn put_get_roundtrip_and_activity() {
    let s = mem().await;
    let o = card("Alice");
    let id = o.envelope.id;
    let a = s.put(o.clone(), ME).await.unwrap();
    assert_eq!(a.kind, ActivityKind::Create);
    let got = s.get(id).await.unwrap().unwrap();
    assert_eq!(got.content, o.content);
    assert_eq!(got.envelope.to, o.envelope.to);
    assert_eq!(got.index, o.index);
    assert_eq!(got.envelope.name, "Alice");
    // update
    let mut o2 = got.clone();
    o2.content = json!({"@type":"Card","name":{"full":"Alice B"}});
    o2.envelope.name = "Alice B".into();
    let a2 = s.put(o2, ME).await.unwrap();
    assert_eq!(a2.kind, ActivityKind::Update);
    assert!(a2.id > a.id);
    assert_eq!(
        s.get(id).await.unwrap().unwrap().envelope.published,
        got.envelope.published
    );
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|a| !a.synced));
    // payload carries before/after
    let cs = log[1].change_set().unwrap();
    assert_eq!(cs.before.unwrap().envelope.name, "Alice");
    assert_eq!(cs.after.unwrap().envelope.name, "Alice B");
    // delete
    s.delete(id, ME).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
    assert!(matches!(s.delete(id, ME).await, Err(Error::NotFound(_))));
}

#[tokio::test]
async fn name_is_mandatory() {
    let s = mem().await;
    let o = card("  ");
    assert!(matches!(s.put(o, ME).await, Err(Error::Invalid(_))));
}

#[tokio::test]
async fn multi_value_index_roundtrip() {
    let s = mem().await;
    let o = card("Dora").with_index("email", "dora2@example.org");
    let id = o.envelope.id;
    s.put(o, ME).await.unwrap();
    let got = s.get(id).await.unwrap().unwrap();
    assert_eq!(
        got.index["email"],
        ["dora@example.org", "dora2@example.org"]
    );
    assert_eq!(
        s.query(&Query::new().with_index("email", "dora2@example.org"))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn original_is_kept_and_immutable() {
    let s = mem().await;
    let raw = b"BEGIN:VCARD\r\nFN:Eve\r\nEND:VCARD\r\n".to_vec();
    let o = card("Eve").with_original(Blob {
        media_type: "text/vcard".into(),
        bytes: raw.clone(),
    });
    let id = o.envelope.id;
    s.put(o.clone(), ME).await.unwrap();
    // update without original keeps it
    let mut u = card("Eve");
    u.envelope.id = id;
    u.content = json!({"changed": true});
    s.put(u, ME).await.unwrap();
    let got = s.get(id).await.unwrap().unwrap();
    assert_eq!(got.original.as_ref().unwrap().bytes, raw);
    assert_eq!(got.content, json!({"changed": true}));
    // update with a different original is refused
    let mut bad = o.clone();
    bad.original = Some(Blob {
        media_type: "text/vcard".into(),
        bytes: b"other".to_vec(),
    });
    assert!(matches!(
        s.put(bad, ME).await,
        Err(Error::OriginalImmutable(_))
    ));
    // update snapshot omits the original (space), create snapshot has it
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert!(
        log[0]
            .change_set()
            .unwrap()
            .after
            .unwrap()
            .original
            .is_some()
    );
    assert!(
        log[1]
            .change_set()
            .unwrap()
            .after
            .unwrap()
            .original
            .is_none()
    );
    // delete + undo brings the original back
    s.delete(id, ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    assert_eq!(
        s.get(id).await.unwrap().unwrap().original.unwrap().bytes,
        raw
    );
}

#[tokio::test]
async fn dyn_store_is_object_safe() {
    let s: Box<dyn Store> = Box::new(mem().await);
    s.put(card("Sue"), ME).await.unwrap();
    assert_eq!(s.count(&Query::new()).await.unwrap(), 1);
}
