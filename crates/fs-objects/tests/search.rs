//! 0.2 range, search, ordering filters.

use fs_objects::*;

mod common;
use common::*;

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
