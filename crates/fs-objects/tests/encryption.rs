//! Encryption at rest and the sensitive-data policy.

use fs_objects::*;
use serde_json::json;

mod common;
use common::*;

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
