//! A version 3 file as fs-store at commit 60df691 wrote it (unlogged marks, no purge tables, no `redacted`
//! column) opens, migrates to 4 in place, and purge, identity and the unlogged guard work on it.

use fs_store::traits::{StoreInfo, UnloggedObjects};
use fs_store::*;

mod common;
use common::*;

/// The version 3 schema, verbatim from 60df691 (`turso/schema.rs`).
const V3_SCHEMA: &str = "
CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version VALUES (3);
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
CREATE TABLE unlogged_ids (object_id TEXT PRIMARY KEY);
CREATE TABLE index_fields (
    object_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
    value_norm TEXT NOT NULL DEFAULT ''
);
";

#[tokio::test]
async fn a_real_version_3_file_migrates_and_everything_new_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v3.db");
    {
        let db = turso::Builder::new_local(path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let c = db.connect().unwrap();
        c.execute_batch(V3_SCHEMA).await.unwrap();
        c.execute_batch(&old_rows_sql()).await.unwrap();
    }
    let s = TursoStore::open(&path).await.unwrap();
    assert_eq!(s.schema_version(), 4);
    assert_old_rows_survive(&s).await;
    let a = note("a", "text");
    s.put(a.clone(), ME).await.unwrap(); // the first logged put used to fail here
    let ballot = obj("Box", "b");
    s.put_unlogged(ballot.clone()).await.unwrap();
    assert!(s.get(ballot.envelope.id).await.is_err());
    s.purge_object(a.envelope.id).await.unwrap();
    s.put(a.clone(), ME).await.unwrap();
    let id = StoreInfo::identity(&s);
    drop(s);
    let s = TursoStore::open(&path).await.unwrap();
    assert_eq!(StoreInfo::identity(&s), id);
    assert!(s.get(a.envelope.id).await.unwrap().is_some());
}
