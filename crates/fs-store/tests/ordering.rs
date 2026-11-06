//! Ordering of query results (name key, index field).

use fs_store::*;

mod common;
use common::*;

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
async fn order_by_index_field() {
    let s = mem().await;
    for (n, k) in [
        ("Ada Lovelace", "lovelace, ada"),
        ("Zed Abel", "Abel, zed"),
        ("Mid Dle", "Müller, mid"),
    ] {
        s.put(obj("Profile", n).with_index("name_sort", k), ME)
            .await
            .unwrap();
    }
    s.put(obj("Profile", "No Key"), ME).await.unwrap();
    let q = Query::new().of_type("Profile").order_by_index("name_sort");
    assert_eq!(
        names(&s.query(&q).await.unwrap()),
        ["Zed Abel", "Ada Lovelace", "Mid Dle", "No Key"]
    );
    let q = q.limit(1).offset(1);
    assert_eq!(names(&s.query(&q).await.unwrap()), ["Ada Lovelace"]);
}
