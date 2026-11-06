//! Activity log, change stream, transactions.

use fs_store::*;
use serde_json::json;

mod common;
use common::*;

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
