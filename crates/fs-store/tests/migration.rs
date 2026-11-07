//! Schema versioning and migration from 0.1.

use fs_store::*;

mod common;
use common::*;

/// The 0.1 schema, verbatim.
const V1_SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE objects (
    id TEXT PRIMARY KEY, iri TEXT, type TEXT NOT NULL, name TEXT NOT NULL, context TEXT,
    attributed_to TEXT, in_reply_to TEXT, published INTEGER NOT NULL, updated INTEGER NOT NULL,
    content_type TEXT NOT NULL, content TEXT NOT NULL, original BLOB, original_type TEXT,
    addressing TEXT NOT NULL, sensitive TEXT NOT NULL
);
CREATE INDEX objects_type ON objects (type, updated);
CREATE TABLE activities (
    id TEXT PRIMARY KEY, kind TEXT NOT NULL, object_id TEXT NOT NULL, actor TEXT NOT NULL,
    at INTEGER NOT NULL, target TEXT, payload TEXT NOT NULL, synced INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE index_fields (object_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL);
CREATE INDEX index_fields_kv ON index_fields (key, value);
";

#[tokio::test]
async fn migrates_a_0_1_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let id = ObjectId::new();
    {
        let db = turso::Builder::new_local(path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let c = db.connect().unwrap();
        c.execute_batch(V1_SCHEMA).await.unwrap();
        c.execute(
            "INSERT INTO objects VALUES (?, NULL, 'Profile', 'Émile Old', 'contacts', NULL, NULL, 1000000, 2000000, \
             'application/json', '{\"a\":1}', NULL, NULL, '{\"to\":[],\"cc\":[],\"bto\":[],\"bcc\":[],\"audience\":[]}', '[]')",
            (id.to_string(),),
        )
        .await
        .unwrap();
        c.execute(
            "INSERT INTO index_fields VALUES (?, 'email', 'Old@Example.ORG')",
            (id.to_string(),),
        )
        .await
        .unwrap();
        c.execute(
            "INSERT INTO activities VALUES ('0190a000-0000-7000-8000-000000000001', 'Create', ?, 'me', 2000000, NULL, '{}', 1)",
            (id.to_string(),),
        )
        .await
        .unwrap();
    }
    {
        let s = TursoStore::open(&path).await.unwrap();
        assert_eq!(s.schema_version(), 4);
        let o = s.get(id).await.unwrap().unwrap();
        assert_eq!(o.envelope.name, "Émile Old");
        assert_eq!(o.envelope.start_time, None);
        assert_eq!(o.index["email"], ["Old@Example.ORG"]);
        // backfilled search keys work on old rows
        let q = Query::new()
            .name_contains("ÉMILE")
            .index_op("email", Op::Contains, "old@example");
        assert_eq!(s.count(&q).await.unwrap(), 1);
        assert_eq!(
            s.activities(&ActivityFilter::default())
                .await
                .unwrap()
                .len(),
            1
        );
        // new fields work on the migrated store
        let mut o = o;
        o.envelope.start_time = Some(at(9));
        s.put(o, ME).await.unwrap();
    }
    // reopening is a no-op (idempotent) and keeps data
    let s = TursoStore::open(&path).await.unwrap();
    let o = s.get(id).await.unwrap().unwrap();
    assert_eq!(o.envelope.start_time, Some(at(9)));
    assert_eq!(
        s.activities(&ActivityFilter::default())
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn fresh_database_has_version_and_refuses_newer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.db");
    {
        let s = TursoStore::open(&path).await.unwrap();
        s.put(obj("Note", "x"), ME).await.unwrap();
    }
    {
        let db = turso::Builder::new_local(path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let c = db.connect().unwrap();
        c.execute("UPDATE schema_version SET version = 99", ())
            .await
            .unwrap();
    }
    assert!(matches!(
        TursoStore::open(&path).await,
        Err(Error::Backend(_))
    ));
}
