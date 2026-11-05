use std::time::Duration;

use fs_store::*;
use serde_json::json;

const ME: &str = "https://example.org/users/me";

fn card(name: &str) -> StoredObject {
    let mut e = Envelope::new("Profile", name);
    e.context = Some("contacts".into());
    e.attributed_to = Some(ME.into());
    e.to = vec![ME.into()];
    StoredObject::new(
        e,
        "application/jscontact+json",
        json!({"@type":"Card","name":{"full":name}}),
    )
    .with_index("email", format!("{}@example.org", name.to_lowercase()))
    .with_index("kind", "individual")
}

async fn mem() -> TursoStore {
    TursoStore::open_in_memory().await.unwrap()
}

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
async fn query_by_type_context_and_index() {
    let s = mem().await;
    for n in ["Alice", "Bob", "Carol"] {
        s.put(card(n), ME).await.unwrap();
    }
    let mut ev = Envelope::new("Event", "Lunch");
    ev.context = Some("calendar".into());
    s.put(
        StoredObject::new(ev, "application/jscalendar+json", json!({"title":"Lunch"})),
        ME,
    )
    .await
    .unwrap();

    assert_eq!(
        s.query(&Query::new().of_type("Profile"))
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        s.query(&Query::new().in_context("calendar"))
            .await
            .unwrap()
            .len(),
        1
    );
    let r = s
        .query(&Query::new().with_index("email", "bob@example.org"))
        .await
        .unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].envelope.name, "Bob");
    let r = s
        .query(
            &Query::new()
                .of_type("Profile")
                .with_index("kind", "individual")
                .ordered(Order::NameAsc)
                .limit(2)
                .offset(1),
        )
        .await
        .unwrap();
    assert_eq!(
        r.iter()
            .map(|o| o.envelope.name.as_str())
            .collect::<Vec<_>>(),
        ["Bob", "Carol"]
    );
    assert_eq!(
        s.count(&Query::new().attributed_to(ME)).await.unwrap(),
        4 - 1
    );
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
async fn undo_redo_basic() {
    let s = mem().await;
    let o = card("Fay");
    let id = o.envelope.id;
    s.put(o.clone(), ME).await.unwrap();
    let mut v2 = o.clone();
    v2.content = json!({"v": 2});
    s.put(v2, ME).await.unwrap();
    assert_eq!(
        s.undo_state(ME).await.unwrap(),
        UndoState {
            undoable: 2,
            redoable: 0
        }
    );

    let u = s.undo(ME, 1).await.unwrap();
    assert_eq!(u[0].kind, ActivityKind::Undo);
    assert_eq!(s.get(id).await.unwrap().unwrap().content, o.content);
    s.redo(ME, 1).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 2}));
    // undo all -> object gone; redo all -> back
    s.undo(ME, 5).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
    assert_eq!(
        s.undo_state(ME).await.unwrap(),
        UndoState {
            undoable: 0,
            redoable: 2
        }
    );
    s.redo(ME, 5).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content, json!({"v": 2}));
    // new work clears redo
    s.undo(ME, 1).await.unwrap();
    let mut v3 = o.clone();
    v3.content = json!({"v": 3});
    s.put(v3, ME).await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap().redoable, 0);
}

#[tokio::test]
async fn undo_is_per_actor_and_detects_conflict() {
    let s = mem().await;
    let o = card("Gus");
    let id = o.envelope.id;
    s.put(o.clone(), "alice").await.unwrap();
    let mut v2 = o.clone();
    v2.content = json!({"v": 2});
    s.put(v2, "bob").await.unwrap();
    // alice cannot undo her create: bob changed the object since
    assert!(matches!(s.undo("alice", 1).await, Err(Error::Conflict(_))));
    assert!(s.get(id).await.unwrap().is_some());
    // nothing for carol
    assert!(s.undo("carol", 3).await.unwrap().is_empty());
    s.undo("bob", 1).await.unwrap();
    s.undo("alice", 1).await.unwrap();
    assert!(s.get(id).await.unwrap().is_none());
}

#[tokio::test]
async fn failed_operation_rolls_back_object_and_log() {
    let s = mem().await;
    let o = card("Hal");
    let id = o.envelope.id;
    s.inject_fault_before_activity(true);
    assert!(s.put(o.clone(), ME).await.is_err());
    assert!(
        s.get(id).await.unwrap().is_none(),
        "object write must be rolled back"
    );
    assert!(
        s.activities(&ActivityFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    // works afterwards
    s.put(o, ME).await.unwrap();
    s.inject_fault_before_activity(true);
    assert!(s.delete(id, ME).await.is_err());
    assert!(s.get(id).await.unwrap().is_some());
    assert_eq!(
        s.activities(&ActivityFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn subscribe_sees_committed_activities_only() {
    let s = mem().await;
    let mut rx = s.subscribe();
    let o = card("Ida");
    s.inject_fault_before_activity(true);
    let _ = s.put(o.clone(), ME).await;
    s.put(o.clone(), ME).await.unwrap();
    s.log(ActivityKind::Read, o.envelope.id, ME, json!({}))
        .await
        .unwrap();
    assert_eq!(rx.recv().await.unwrap().kind, ActivityKind::Create);
    assert_eq!(rx.recv().await.unwrap().kind, ActivityKind::Read);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn log_only_kinds() {
    let s = mem().await;
    let o = card("Jo");
    s.put(o.clone(), ME).await.unwrap();
    s.log(
        ActivityKind::Offer,
        o.envelope.id,
        ME,
        json!({"to": "https://x"}),
    )
    .await
    .unwrap();
    assert!(matches!(
        s.log(ActivityKind::Update, o.envelope.id, ME, json!({}))
            .await,
        Err(Error::Invalid(_))
    ));
    // log-only entries are not undoable
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 1);
}

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
async fn undo_after_compaction_of_target_is_graceful() {
    let s = mem().await;
    let o = card("Max");
    s.put(o.clone(), ME).await.unwrap();
    let mut v = o.clone();
    v.content = json!({"v": 2});
    s.put(v, ME).await.unwrap();
    s.undo(ME, 1).await.unwrap();
    let all = s.activities(&ActivityFilter::default()).await.unwrap();
    s.mark_synced(all.last().unwrap().id).await.unwrap();
    s.clear_history().await.unwrap();
    assert_eq!(s.undo_state(ME).await.unwrap(), UndoState::default());
    assert!(s.undo(ME, 3).await.unwrap().is_empty());
}

#[tokio::test]
async fn activity_ids_are_strictly_increasing() {
    let s = mem().await;
    let o = card("Ned");
    for _ in 0..50 {
        s.put(o.clone(), ME).await.unwrap();
    }
    let l = s.activities(&ActivityFilter::default()).await.unwrap();
    assert!(l.windows(2).all(|w| w[0].id < w[1].id));
    assert_eq!(l[0].id.as_uuid().get_version_num(), 7);
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

fn key() -> EncryptionKey {
    EncryptionKey {
        cipher: Cipher::Aes256Gcm,
        key: (0u8..32).collect(),
    }
}

#[tokio::test]
async fn encryption_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("enc.db");
    let secret = "very-secret-needle-0xBEEF";
    let opts = || OpenOptions {
        key_provider: Some(std::sync::Arc::new(StaticKeyProvider(key()))),
        ..Default::default()
    };
    let mut o = card("Pat");
    o.content = json!({"note": secret});
    o.envelope.sensitive = vec!["pd:SexualPreference".into()];
    let id = o.envelope.id;
    {
        let s = TursoStore::open_with(&path, opts()).await.unwrap();
        assert!(s.is_encrypted());
        s.put(o.clone(), ME).await.unwrap();
    }
    // negative control: the same data in a plain store IS visible on disk
    {
        let plain = dir.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        let s = TursoStore::open_with(
            plain.join("p.db"),
            OpenOptions {
                allow_sensitive_unencrypted: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        s.put(o.clone(), ME).await.unwrap();
        drop(s);
        let found = std::fs::read_dir(&plain).unwrap().any(|e| {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            b.windows(secret.len()).any(|w| w == secret.as_bytes())
        });
        assert!(found, "control: plaintext store should contain the needle");
    }
    // plaintext never hits disk (db + wal)
    for e in std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
    {
        let bytes = std::fs::read(e.unwrap().path()).unwrap();
        assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
    }
    let s = TursoStore::open_with(&path, opts()).await.unwrap();
    assert_eq!(s.get(id).await.unwrap().unwrap().content["note"], secret);
}

#[tokio::test]
async fn encrypted_store_rejects_wrong_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("enc2.db");
    let mk = |k: Vec<u8>| OpenOptions {
        key_provider: Some(std::sync::Arc::new(StaticKeyProvider(EncryptionKey {
            cipher: Cipher::Aes256Gcm,
            key: k,
        }))),
        ..Default::default()
    };
    {
        let s = TursoStore::open_with(&path, mk((0u8..32).collect()))
            .await
            .unwrap();
        s.put(card("Quin"), ME).await.unwrap();
    }
    let bad = TursoStore::open_with(&path, mk(vec![9u8; 32])).await;
    let ok = match bad {
        Err(_) => true,
        Ok(s) => s.query(&Query::new()).await.is_err(),
    };
    assert!(ok, "opening/reading with the wrong key must fail");
    assert!(matches!(
        TursoStore::open_with(dir.path().join("x.db"), mk(vec![1u8; 5])).await,
        Err(Error::Key(_))
    ));
}

#[tokio::test]
async fn sensitive_requires_encryption_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let s = TursoStore::open(dir.path().join("plain.db")).await.unwrap();
    let mut o = card("Rex");
    o.envelope.sensitive = vec!["pd:Health".into()];
    assert!(matches!(
        s.put(o.clone(), ME).await,
        Err(Error::EncryptionRequired(_))
    ));
    // explicit opt-out
    let s2 = TursoStore::open_with(
        dir.path().join("plain2.db"),
        OpenOptions {
            allow_sensitive_unencrypted: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    s2.put(o, ME).await.unwrap();
}

#[tokio::test]
async fn dyn_store_is_object_safe() {
    let s: Box<dyn Store> = Box::new(mem().await);
    s.put(card("Sue"), ME).await.unwrap();
    assert_eq!(s.count(&Query::new()).await.unwrap(), 1);
}
