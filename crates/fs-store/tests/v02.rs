//! fs-store 0.2: new kinds, put_and_log, range/search queries, envelope time fields, migration.

use chrono::{TimeZone, Utc};
use fs_store::*;
use serde_json::json;

const ME: &str = "https://example.org/users/me";

fn obj(ty: &str, name: &str) -> StoredObject {
    let mut e = Envelope::new(ty, name);
    e.context = Some("t".into());
    StoredObject::new(e, "application/json", json!({ "n": name }))
}

async fn mem() -> TursoStore {
    TursoStore::open_in_memory().await.unwrap()
}

fn names(v: &[StoredObject]) -> Vec<&str> {
    v.iter().map(|o| o.envelope.name.as_str()).collect()
}

fn at(h: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, h, 0, 0).unwrap()
}

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

#[tokio::test]
async fn index_range_filters() {
    let s = mem().await;
    for (n, d) in [
        ("a", "2026-01-01T00:00:00Z"),
        ("b", "2026-02-01T00:00:00Z"),
        ("c", "2026-03-01T00:00:00Z"),
    ] {
        s.put(obj("Task", n).with_index("due_utc", d), ME)
            .await
            .unwrap();
    }
    let q = Query::new().index_between("due_utc", "2026-01-15", "2026-03-01");
    assert_eq!(names(&s.query(&q).await.unwrap()), ["b"]);
    let q = Query::new().index_op("due_utc", Op::Le, "2026-02-01T00:00:00Z");
    assert_eq!(s.count(&q).await.unwrap(), 2);
    // different key does not match
    let q = Query::new().index_op("other", Op::Ge, "");
    assert_eq!(s.count(&q).await.unwrap(), 0);
}

#[tokio::test]
async fn contains_prefix_case_insensitive_unicode_and_escaping() {
    let s = mem().await;
    for n in [
        "Émilie Dupont",
        "ÉMILE ZOLA",
        "100% Wolle",
        "a_b",
        "axb",
        "back\\slash",
        "Straße",
    ] {
        s.put(obj("Profile", n), ME).await.unwrap();
    }
    let q = |op, v: &str| {
        Query::new()
            .and(Cond::Name {
                op,
                value: v.into(),
            })
            .ordered(Order::NameAsc)
    };
    // case-insensitive incl. non-ASCII
    assert_eq!(
        names(&s.query(&q(Op::Prefix, "émil")).await.unwrap()),
        ["ÉMILE ZOLA", "Émilie Dupont"]
    );
    assert_eq!(
        names(&s.query(&q(Op::Contains, "DUPONT")).await.unwrap()),
        ["Émilie Dupont"]
    );
    // decomposed needle matches precomposed text
    assert_eq!(
        names(&s.query(&q(Op::Prefix, "E\u{301}milie")).await.unwrap()),
        ["Émilie Dupont"]
    );
    assert_eq!(
        names(&s.query(&q(Op::Contains, "STRASSE")).await.unwrap()),
        Vec::<&str>::new()
    );
    assert_eq!(
        names(
            &s.query(&q(Op::Contains, "STRASSE".replace("SS", "ß").as_str()))
                .await
                .unwrap()
        ),
        ["Straße"]
    );
    // LIKE wildcards in the needle are literals
    assert_eq!(
        names(&s.query(&q(Op::Contains, "%")).await.unwrap()),
        ["100% Wolle"]
    );
    assert_eq!(
        names(&s.query(&q(Op::Contains, "a_b")).await.unwrap()),
        ["a_b"]
    );
    assert_eq!(
        names(&s.query(&q(Op::Contains, "_")).await.unwrap()),
        ["a_b"]
    );
    assert_eq!(
        names(&s.query(&q(Op::Prefix, "back\\")).await.unwrap()),
        ["back\\slash"]
    );
    assert_eq!(s.count(&q(Op::Contains, "")).await.unwrap(), 7);
}

#[tokio::test]
async fn name_order_is_case_insensitive() {
    let s = mem().await;
    for n in [
        "NLnet",
        "Nextcloud",
        "zebra",
        "Zorro",
        "Ärger",
        "apple",
        "Banana",
    ] {
        s.put(obj("Profile", n), ME).await.unwrap();
    }
    let v = s
        .query(&Query::new().ordered(Order::NameAsc))
        .await
        .unwrap();
    assert_eq!(
        names(&v),
        [
            "apple",
            "Banana",
            "Nextcloud",
            "NLnet",
            "zebra",
            "Zorro",
            "Ärger"
        ]
    );
    let v = s
        .query(&Query::new().ordered(Order::NameDesc).limit(2))
        .await
        .unwrap();
    assert_eq!(names(&v), ["Ärger", "Zorro"]);
    // pagination keeps order
    let v = s
        .query(&Query::new().ordered(Order::NameAsc).limit(2).offset(2))
        .await
        .unwrap();
    assert_eq!(names(&v), ["Nextcloud", "NLnet"]);
}

#[tokio::test]
async fn index_search_and_or_group() {
    let s = mem().await;
    s.put(
        obj("Profile", "Alice")
            .with_index("email", "Alice@Example.org")
            .with_index("phone", "+4930123"),
        ME,
    )
    .await
    .unwrap();
    s.put(
        obj("Profile", "Bob").with_index("email", "bob@work.example"),
        ME,
    )
    .await
    .unwrap();
    s.put(obj("Event", "Alice party").with_index("email", "x@y.z"), ME)
        .await
        .unwrap();
    let search = |t: &str| {
        let c = |key: &str| Cond::Index {
            key: key.into(),
            op: Op::Contains,
            value: t.into(),
        };
        Query::new()
            .of_type("Profile")
            .or(Cond::Name {
                op: Op::Contains,
                value: t.into(),
            })
            .or(c("email"))
            .or(c("phone"))
            .ordered(Order::NameAsc)
    };
    assert_eq!(names(&s.query(&search("ALICE")).await.unwrap()), ["Alice"]);
    assert_eq!(
        names(&s.query(&search("example")).await.unwrap()),
        ["Alice", "Bob"]
    );
    assert_eq!(names(&s.query(&search("+4930")).await.unwrap()), ["Alice"]);
    assert_eq!(s.count(&search("nothing")).await.unwrap(), 0);
    // OR group AND exact legacy filter AND count
    let q = search("example").with_index("email", "bob@work.example");
    assert_eq!(names(&s.query(&q).await.unwrap()), ["Bob"]);
    // prefix on index
    let q = Query::new().index_op("email", Op::Prefix, "ALICE@");
    assert_eq!(s.count(&q).await.unwrap(), 1);
}

// ---- schema migration -------------------------------------------------------------------

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
        assert_eq!(s.schema_version(), 2);
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
