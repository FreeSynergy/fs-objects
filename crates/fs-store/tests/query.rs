//! Basic queries by type, context and index.

use fs_store::*;
use serde_json::json;

mod common;
use common::*;

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
