//! 0.2 activity kinds and `put_and_log`.

use fs_objects::*;
use serde_json::json;

mod common;
use common::*;

#[test]
fn new_kinds_keep_string_names() {
    for (k, n) in [
        (ActivityKind::Invite, "Invite"),
        (ActivityKind::TentativeAccept, "TentativeAccept"),
        (ActivityKind::Join, "Join"),
        (ActivityKind::Leave, "Leave"),
        (ActivityKind::Like, "Like"),
        (ActivityKind::Move, "Move"),
        (ActivityKind::Create, "Create"),
    ] {
        assert_eq!(k.as_str(), n);
        assert_eq!(n.parse::<ActivityKind>().unwrap(), k);
        assert_eq!(serde_json::to_value(k).unwrap(), json!(n));
        assert!(!k.changes_state() || matches!(k, ActivityKind::Create));
    }
}

#[tokio::test]
async fn new_kinds_can_be_logged() {
    let s = mem().await;
    let o = obj("Event", "Party");
    s.put(o.clone(), ME).await.unwrap();
    for k in [
        ActivityKind::Invite,
        ActivityKind::TentativeAccept,
        ActivityKind::Like,
    ] {
        let a = s.log(k, o.envelope.id, ME, json!({})).await.unwrap();
        assert_eq!(a.kind, k);
    }
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(log[2].kind, ActivityKind::TentativeAccept);
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 1);
}

#[tokio::test]
async fn put_and_log_is_one_transaction() {
    let s = mem().await;
    let mut rx = s.subscribe();
    let mut o = obj("Event", "Meeting");
    let id = o.envelope.id;
    let acts = s
        .put_and_log(
            o.clone(),
            ME,
            vec![LogEntry::new(ActivityKind::Invite, json!({"who": "a"}))],
        )
        .await
        .unwrap();
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[0].kind, ActivityKind::Create);
    assert_eq!(acts[1].kind, ActivityKind::Invite);
    assert!(acts[0].id < acts[1].id);

    o.envelope.name = "Meeting 2".into();
    let acts = s
        .put_and_log(
            o.clone(),
            ME,
            vec![
                LogEntry::new(ActivityKind::TentativeAccept, json!({"n": 1})),
                LogEntry::new(ActivityKind::Read, json!({"n": 2})),
            ],
        )
        .await
        .unwrap();
    assert_eq!(acts.len(), 3);
    assert_eq!(acts[0].kind, ActivityKind::Update);
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(log.len(), 5);
    // stream order = log order, all after commit
    for want in &log {
        assert_eq!(rx.recv().await.unwrap().id, want.id);
    }
    // undo only reverts the update, extras are not undoable
    assert_eq!(s.undo_state(ME).await.unwrap().undoable, 2);

    // state-changing kinds are refused as extras
    assert!(matches!(
        s.put_and_log(o, ME, vec![LogEntry::new(ActivityKind::Delete, json!({}))])
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(s.get(id).await.unwrap().unwrap().envelope.name, "Meeting 2");
}

#[tokio::test]
async fn put_and_log_fault_between_entries_rolls_back_everything() {
    let s = mem().await;
    let mut rx = s.subscribe();
    let mut o = obj("Event", "Meeting");
    let id = o.envelope.id;
    let extra = || vec![LogEntry::new(ActivityKind::Accept, json!({}))];

    // create fails after the Create entry, before the extras
    s.inject_fault_before_extra(true);
    assert!(s.put_and_log(o.clone(), ME, extra()).await.is_err());
    assert!(s.get(id).await.unwrap().is_none());
    assert!(
        s.activities(&ActivityFilter::default())
            .await
            .unwrap()
            .is_empty()
    );

    s.put_and_log(o.clone(), ME, extra()).await.unwrap();
    let state = s.get(id).await.unwrap().unwrap();

    // update fails the same way: object and log unchanged
    o.envelope.name = "Changed".into();
    s.inject_fault_before_extra(true);
    assert!(s.put_and_log(o.clone(), ME, extra()).await.is_err());
    assert_eq!(s.get(id).await.unwrap().unwrap(), state);
    assert_eq!(
        s.activities(&ActivityFilter::default())
            .await
            .unwrap()
            .len(),
        2
    );

    // fault before the first entry as well
    s.inject_fault_before_activity(true);
    assert!(s.put_and_log(o.clone(), ME, extra()).await.is_err());
    assert_eq!(s.get(id).await.unwrap().unwrap(), state);

    // store still works and ids stay strictly increasing
    s.put_and_log(o, ME, extra()).await.unwrap();
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(log.len(), 4);
    assert!(log.windows(2).all(|w| w[0].id < w[1].id));
    // only committed entries were published
    let mut seen = 0;
    while rx.try_recv().is_ok() {
        seen += 1;
    }
    assert_eq!(seen, 4);
}
