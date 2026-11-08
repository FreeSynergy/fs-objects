//! `query_unlogged` (0.6): searches unlogged objects of the caller's types and nothing else.

use fs_objects::traits::UnloggedObjects;
use fs_objects::{Envelope, ObjectId, Query, Store, StoredObject, TursoStore};

fn object(kind: &str, name: &str) -> StoredObject {
    let mut e = Envelope::new(kind, name);
    e.id = ObjectId::new();
    StoredObject::new(e, "Note", serde_json::json!({ "n": name }))
}

#[tokio::test]
async fn finds_only_unlogged_objects_of_the_given_types() {
    let s = TursoStore::open_in_memory().await.unwrap();
    let (a, b) = (object("Mine", "a"), object("Mine", "b"));
    s.put_unlogged(a.clone()).await.unwrap();
    s.put_unlogged(b).await.unwrap();
    s.put_unlogged(object("Theirs", "c")).await.unwrap();
    Store::put(&s, object("Mine", "logged"), "x").await.unwrap();

    let found = s.query_unlogged(&Query::new(), &["Mine"]).await.unwrap();
    let mut names: Vec<_> = found.iter().map(|o| o.envelope.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["a", "b"], "no logged object, no other type");
    assert!(
        Store::query(&s, &Query::new()).await.is_err(),
        "the logged query still refuses"
    );
}

#[tokio::test]
async fn refuses_an_empty_type_list_and_honours_the_query() {
    let s = TursoStore::open_in_memory().await.unwrap();
    s.put_unlogged(object("Mine", "a")).await.unwrap();
    s.put_unlogged(object("Mine", "b")).await.unwrap();
    assert!(s.query_unlogged(&Query::new(), &[]).await.is_err());
    let one = s
        .query_unlogged(&Query::new().limit(1), &["Mine"])
        .await
        .unwrap();
    assert_eq!(one.len(), 1);
}
