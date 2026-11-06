//! 0.2 envelope fields and time-range queries.

use fs_store::*;
use serde_json::json;

mod common;
use common::*;

#[tokio::test]
async fn envelope_fields_roundtrip_and_serde() {
    let s = mem().await;
    let mut o = obj("Event", "Conf");
    o.envelope.describes = Some("https://example.org/x".into());
    o.envelope.start_time = Some(at(9));
    o.envelope.end_time = Some(at(17));
    s.put(o.clone(), ME).await.unwrap();
    let got = s.get(o.envelope.id).await.unwrap().unwrap();
    assert_eq!(got.envelope.describes, o.envelope.describes);
    assert_eq!(got.envelope.start_time, Some(at(9)));
    assert_eq!(got.envelope.end_time, Some(at(17)));
    let j = serde_json::to_value(&got.envelope).unwrap();
    assert_eq!(j["startTime"], json!("2026-03-01T09:00:00Z"));
    assert!(j.get("describes").is_some());
    // absent fields are not serialised and old JSON still parses
    let plain = serde_json::to_value(&obj("Note", "n").envelope).unwrap();
    assert!(plain.get("startTime").is_none());
    let back: Envelope = serde_json::from_value(plain).unwrap();
    assert!(back.start_time.is_none());
    // the Update image in the log carries the new fields too
    let log = s.activities(&ActivityFilter::default()).await.unwrap();
    assert_eq!(
        log[0]
            .change_set()
            .unwrap()
            .after
            .unwrap()
            .envelope
            .start_time,
        Some(at(9))
    );
    // describes filter
    let q = Query::new().and(Cond::Describes {
        op: Op::Eq,
        value: "https://example.org/x".into(),
    });
    assert_eq!(s.count(&q).await.unwrap(), 1);
}

#[tokio::test]
async fn time_range_queries_and_start_order() {
    let s = mem().await;
    for (n, a, b) in [
        ("e8", 8, 9),
        ("e10", 10, 12),
        ("e11", 11, 15),
        ("e20", 20, 21),
    ] {
        let mut o = obj("Event", n);
        o.envelope.start_time = Some(at(a));
        o.envelope.end_time = Some(at(b));
        s.put(o, ME).await.unwrap();
    }
    s.put(obj("Event", "undated"), ME).await.unwrap();
    // overlap with [11:00, 16:00): start < 16 AND end > 11
    let q = Query::new()
        .of_type("Event")
        .time(TimeField::Start, Op::Lt, at(16))
        .time(TimeField::End, Op::Gt, at(11))
        .ordered(Order::StartAsc);
    assert_eq!(names(&s.query(&q).await.unwrap()), ["e10", "e11"]);
    assert_eq!(s.count(&q).await.unwrap(), 2);
    // between, inclusive lower / exclusive upper
    let q =
        Query::new()
            .time(TimeField::Start, Op::Ge, at(10))
            .time(TimeField::Start, Op::Lt, at(20));
    assert_eq!(s.count(&q).await.unwrap(), 2);
    // start order: undated last, both directions
    let all = s
        .query(&Query::new().ordered(Order::StartAsc))
        .await
        .unwrap();
    assert_eq!(names(&all), ["e8", "e10", "e11", "e20", "undated"]);
    let all = s
        .query(&Query::new().ordered(Order::StartDesc))
        .await
        .unwrap();
    assert_eq!(names(&all), ["e20", "e11", "e10", "e8", "undated"]);
    // pagination with filters
    let q = Query::new()
        .time(TimeField::Start, Op::Ge, at(0))
        .ordered(Order::StartAsc)
        .limit(2)
        .offset(1);
    assert_eq!(names(&s.query(&q).await.unwrap()), ["e10", "e11"]);
    // time + Contains is rejected
    let bad = Query::new().time(TimeField::Start, Op::Contains, at(0));
    assert!(s.query(&bad).await.is_err());
}
