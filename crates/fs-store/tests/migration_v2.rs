//! A version 2 database (fs-store 0.2 / 0.3) opens, migrates in place, and keeps working.

use fs_store::traits::UnloggedObjects;
use fs_store::*;

mod common;
use common::*;

/// The version 2 schema, verbatim.
const V2_SCHEMA: &str = "
CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version VALUES (2);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE objects (
    id TEXT PRIMARY KEY, iri TEXT, type TEXT NOT NULL, name TEXT NOT NULL, context TEXT,
    attributed_to TEXT, in_reply_to TEXT, published INTEGER NOT NULL, updated INTEGER NOT NULL,
    content_type TEXT NOT NULL, content TEXT NOT NULL, original BLOB, original_type TEXT,
    addressing TEXT NOT NULL, sensitive TEXT NOT NULL, describes TEXT, start_time INTEGER,
    end_time INTEGER, name_sort TEXT NOT NULL DEFAULT ''
);
CREATE TABLE activities (
    id TEXT PRIMARY KEY, kind TEXT NOT NULL, object_id TEXT NOT NULL, actor TEXT NOT NULL,
    at INTEGER NOT NULL, target TEXT, payload TEXT NOT NULL, synced INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE index_fields (
    object_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
    value_norm TEXT NOT NULL DEFAULT ''
);
";

/// Write a version 2 file with the schema above.
pub async fn v2_file(path: &std::path::Path) {
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let c = db.connect().unwrap();
    c.execute_batch(V2_SCHEMA).await.unwrap();
}

#[tokio::test]
async fn a_version_2_file_migrates_and_old_and_new_writes_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.db");
    v2_file(&path).await;
    let s = TursoStore::open(&path).await.unwrap();
    assert_eq!(s.schema_version(), 4);
    let a = obj("Note", "a");
    s.put(a.clone(), ME).await.unwrap();
    let b = obj("Box", "b");
    s.put_unlogged(b.clone()).await.unwrap();
    drop(s);
    let s = TursoStore::open(&path).await.unwrap();
    assert_eq!(
        s.get(a.envelope.id).await.unwrap().unwrap().content,
        a.content
    );
    assert!(s.get(b.envelope.id).await.is_err());
    assert!(s.read_unlogged(b.envelope.id).await.unwrap().is_some());
}
