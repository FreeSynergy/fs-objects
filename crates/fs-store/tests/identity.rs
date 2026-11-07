//! `StoreInfo`: the identity (one UUID per store, made once, not in the log) and `is_encrypted`.

use fs_store::traits::StoreInfo;
use fs_store::*;

mod common;
use common::*;

#[tokio::test]
async fn a_new_store_has_its_own_identity_and_it_is_not_a_time_ordered_id() {
    let (a, b) = (mem().await, mem().await);
    assert_ne!(StoreInfo::identity(&a), StoreInfo::identity(&b));
    assert_eq!(StoreInfo::identity(&a).as_uuid().get_version_num(), 4);
    let shown = StoreInfo::identity(&a).to_string();
    assert_eq!(shown.parse::<StoreId>().unwrap(), StoreInfo::identity(&a));
}

#[tokio::test]
async fn the_identity_survives_a_restart_and_a_file_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.db");
    let s = TursoStore::open(&path).await.unwrap();
    let id = StoreInfo::identity(&s);
    s.put(obj("Note", "n"), ME).await.unwrap();
    drop(s);
    let copy = dir.path().join("b.db");
    // A copy is the whole file set: the database and its write-ahead log, if one is left.
    std::fs::copy(&path, &copy).unwrap();
    let wal = dir.path().join("a.db-wal");
    if wal.exists() {
        std::fs::copy(&wal, dir.path().join("b.db-wal")).unwrap();
    }
    assert_eq!(
        StoreInfo::identity(&TursoStore::open(&path).await.unwrap()),
        id
    );
    assert_eq!(
        StoreInfo::identity(&TursoStore::open(&copy).await.unwrap()),
        id
    );
    let other = TursoStore::open(dir.path().join("c.db")).await.unwrap();
    assert_ne!(StoreInfo::identity(&other), id);
}

#[tokio::test]
async fn the_identity_is_nowhere_in_the_log_and_every_change_leaves_it_alone() {
    let s = mem().await;
    let id = StoreInfo::identity(&s).to_string();
    let o = obj("Note", "n");
    s.put(o.clone(), ME).await.unwrap();
    s.delete(o.envelope.id, ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    s.clear_history().await.unwrap();
    s.purge_object(o.envelope.id).await.unwrap();
    assert!(!log_text(&s).await.contains(&id));
    assert_eq!(StoreInfo::identity(&s).to_string(), id);
}

#[tokio::test]
async fn the_facade_and_the_trait_agree_and_is_encrypted_tells_the_truth() {
    let dir = tempfile::tempdir().unwrap();
    let plain = TursoStore::open(dir.path().join("p.db")).await.unwrap();
    let opts = OpenOptions {
        key_provider: Some(std::sync::Arc::new(StaticKeyProvider(key()))),
        ..OpenOptions::default()
    };
    let enc = TursoStore::open_with(dir.path().join("e.db"), opts)
        .await
        .unwrap();
    let dynamic: Vec<&dyn Store> = vec![&plain, &enc];
    assert!(!dynamic[0].is_encrypted());
    assert!(dynamic[1].is_encrypted());
    assert_eq!(dynamic[0].identity(), StoreInfo::identity(&plain));
}

#[tokio::test]
async fn an_older_file_without_identity_gets_exactly_one_on_first_open() {
    // The real version 3 file is in `migration_v3.rs`; here the identity row is removed from a current file
    // to show the first open makes one and later opens keep it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    drop(TursoStore::open(&path).await.unwrap());
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let c = db.connect().unwrap();
    c.execute_batch(
        "DELETE FROM meta WHERE key = 'store_id'; UPDATE schema_version SET version = 3;",
    )
    .await
    .unwrap();
    drop((c, db));
    let first = StoreInfo::identity(&TursoStore::open(&path).await.unwrap());
    let again = StoreInfo::identity(&TursoStore::open(&path).await.unwrap());
    assert_eq!(first, again);
    let rows = raw_sql(&path, "SELECT COUNT(*) FROM meta WHERE key = 'store_id'").await;
    assert_eq!(rows, vec!["Integer(1)".to_string()]);
}
