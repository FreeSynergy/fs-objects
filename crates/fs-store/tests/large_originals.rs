//! `LargeOriginals`: the target behaviour (ignored: the Turso backend answers `Unsupported`) and the
//! honest answer it gives today.

use fs_store::traits::LargeOriginals;
use fs_store::*;

mod common;
use common::*;

const BIG: usize = 1 << 20;

fn message(name: &str) -> StoredObject {
    obj("Note", name).with_original(Blob {
        media_type: "message/rfc822".into(),
        bytes: (0..BIG).map(|i| (i % 251) as u8).collect(),
    })
}

async fn encrypted(dir: &std::path::Path) -> TursoStore {
    let opts = OpenOptions {
        key_provider: Some(std::sync::Arc::new(StaticKeyProvider(key()))),
        ..OpenOptions::default()
    };
    TursoStore::open_with(dir.join("e.db"), opts).await.unwrap()
}

#[tokio::test]
async fn today_every_method_answers_not_supported() {
    let s = mem().await;
    let msg = |e: Error| e.to_string();
    assert!(msg(s.original_threshold().await.unwrap_err()).contains("not supported"));
    assert!(msg(s.set_original_threshold(Some(1)).await.unwrap_err()).contains("not supported"));
    let r = s.original_ref(ObjectId::new()).await.unwrap_err();
    assert!(msg(r).contains("not supported"));
}

#[tokio::test]
#[ignore = "Unsupported: blob table not built (DESIGN.md, Large originals)"]
async fn the_log_carries_a_reference_not_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let s = encrypted(dir.path()).await;
    s.set_original_threshold(Some(64 * 1024)).await.unwrap();
    let o = message("m");
    s.put(o.clone(), ME).await.unwrap();
    let log = log_text(&s).await;
    assert!(log.len() < 4096, "log is {} bytes", log.len());
    let r = s.original_ref(o.envelope.id).await.unwrap().unwrap();
    assert_eq!(r.size as usize, BIG);
    assert_eq!(r.hash.len(), 64);
    assert!(log.contains(&r.hash));
    assert_eq!(
        s.get(o.envelope.id).await.unwrap().unwrap().original,
        o.original
    );
}

#[tokio::test]
#[ignore = "Unsupported: blob table not built (DESIGN.md, Large originals)"]
async fn equal_bytes_are_kept_once_and_undo_of_a_delete_brings_the_original_back() {
    let dir = tempfile::tempdir().unwrap();
    let s = encrypted(dir.path()).await;
    s.set_original_threshold(Some(64 * 1024)).await.unwrap();
    let (a, b) = (message("a"), message("b"));
    s.put(a.clone(), ME).await.unwrap();
    s.put(b.clone(), ME).await.unwrap();
    let size = std::fs::metadata(dir.path().join("e.db")).unwrap().len();
    assert!(size < (BIG as u64) * 3 / 2, "stored twice: {size}");
    s.delete(a.envelope.id, ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    assert_eq!(
        s.get(a.envelope.id).await.unwrap().unwrap().original,
        a.original
    );
}

#[tokio::test]
#[ignore = "Unsupported: blob table not built (DESIGN.md, Large originals)"]
async fn purge_and_compaction_let_go_of_a_blob_nobody_references() {
    let dir = tempfile::tempdir().unwrap();
    let s = encrypted(dir.path()).await;
    s.set_original_threshold(Some(64 * 1024)).await.unwrap();
    let o = message("m");
    s.put(o.clone(), ME).await.unwrap();
    s.delete(o.envelope.id, ME).await.unwrap();
    s.purge_object(o.envelope.id).await.unwrap();
    assert!(s.original_ref(o.envelope.id).await.unwrap().is_none());
    drop(s);
    let size = std::fs::metadata(dir.path().join("e.db")).unwrap().len();
    assert!(size < (BIG as u64) / 2, "blob still on disk: {size}");
}

#[tokio::test]
#[ignore = "Unsupported: blob table not built (DESIGN.md, Large originals)"]
async fn a_plain_store_refuses_a_threshold() {
    let s = mem().await;
    let e = s.set_original_threshold(Some(1024)).await.unwrap_err();
    assert!(matches!(e, Error::Invalid(_)));
}
