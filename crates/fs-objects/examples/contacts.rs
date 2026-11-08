//! Import JSContact cards (as `Profile`) and JSCalendar events (as `Event`) into a store.
//!
//! cargo run -p fs-objects --example contacts -- [contacts.json] [events.json] [db-path]
//!
//! Defaults: the FreeSynergy demo data and a temporary database.

use std::path::PathBuf;
use std::result::Result;

use fs_objects::{Envelope, ObjectId, Order, Query, Store, StoredObject, TursoStore};
use serde_json::Value;

const ACTOR: &str = "https://local.invalid/users/owner";
const DEMO: &str = "/srv/FreeSynergy/fs-documentation/temp/demo-data";

fn s(v: &Value) -> Option<&str> {
    v.as_str().filter(|x| !x.is_empty())
}

/// `urn:uuid:<v7>` -> `ObjectId`, so re-imports update instead of duplicating.
fn id_from_uid(uid: &str) -> ObjectId {
    uid.parse().unwrap_or_default()
}

/// ADR 0032: envelope `name` derived from the content (`name.full` in JSContact).
fn card_to_object(card: &Value) -> Result<StoredObject, String> {
    let uid = s(&card["uid"]).ok_or("card without uid")?;
    let name = s(&card["name"]["full"]).ok_or_else(|| format!("card {uid} has no name.full"))?;
    // ADR 0020: a contact is a Profile describing someone.
    let mut env = Envelope::new("Profile", name);
    env.id = id_from_uid(uid);
    env.iri = Some(format!("https://local.invalid/contacts/{}", env.id));
    env.context = Some("contacts".into());
    env.attributed_to = Some(ACTOR.into());
    env.to = vec![ACTOR.into()];
    let mut o = StoredObject::new(env, "application/jscontact+json", card.clone())
        .with_index("uid", uid)
        .with_index("name_sort", name.to_lowercase());
    if let Some(k) = s(&card["kind"]) {
        o = o.with_index("kind", k);
    }
    for e in card["emails"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let Some(a) = s(&e["address"]) {
            o = o.with_index("email", a.trim().to_lowercase());
        }
    }
    for p in card["phones"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let Some(n) = s(&p["number"]) {
            let norm: String = n
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '+')
                .collect();
            o = o.with_index("phone", norm);
        }
    }
    Ok(o)
}

fn event_to_object(ev: &Value) -> Result<StoredObject, String> {
    let uid = s(&ev["uid"]).ok_or("event without uid")?;
    let title = s(&ev["title"]).ok_or_else(|| format!("event {uid} has no title"))?;
    let mut env = Envelope::new("Event", title);
    env.id = id_from_uid(uid);
    env.iri = Some(format!("https://local.invalid/calendar/{}", env.id));
    env.context = Some("calendar".into());
    env.attributed_to = Some(ACTOR.into());
    env.to = vec![ACTOR.into()];
    let mut o =
        StoredObject::new(env, "application/jscalendar+json", ev.clone()).with_index("uid", uid);
    for k in ["start", "status"] {
        if let Some(v) = s(&ev[k]) {
            o = o.with_index(k, v);
        }
    }
    for p in ev["participants"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if let Some(e) = s(&p["email"]) {
            o = o.with_index("participant_email", e.to_lowercase());
        }
    }
    Ok(o)
}

async fn import(
    store: &dyn Store,
    file: &PathBuf,
    conv: fn(&Value) -> Result<StoredObject, String>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let items: Vec<Value> = serde_json::from_slice(&std::fs::read(file)?)?;
    let mut n = 0;
    for item in &items {
        store.put(conv(item)?, ACTOR).await?;
        n += 1;
    }
    Ok(n)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let contacts = args
        .next()
        .map_or_else(|| format!("{DEMO}/contacts.json").into(), PathBuf::from);
    let events = args
        .next()
        .map_or_else(|| format!("{DEMO}/events.json").into(), PathBuf::from);
    let tmp = tempfile::tempdir()?;
    let db = args
        .next()
        .map_or_else(|| tmp.path().join("contacts.db"), PathBuf::from);

    let store = TursoStore::open(&db).await?;
    let c = import(&store, &contacts, card_to_object).await?;
    let e = import(&store, &events, event_to_object).await?;
    println!(
        "imported {c} Profile objects and {e} Event objects into {}",
        db.display()
    );

    for p in store
        .query(&Query::new().of_type("Profile").ordered(Order::NameAsc))
        .await?
    {
        println!(
            "  Profile  {}  {}  {:?}",
            p.envelope.id,
            p.envelope.name,
            p.index.get("email").cloned().unwrap_or_default()
        );
    }
    for ev in store
        .query(&Query::new().of_type("Event").ordered(Order::NameAsc))
        .await?
    {
        println!("  Event    {}  {}", ev.envelope.id, ev.envelope.name);
    }
    if let Some(addr) = store
        .query(&Query::new().of_type("Event").limit(1))
        .await?
        .first()
        .and_then(|e| e.index.get("participant_email")?.first().cloned())
    {
        let events = store
            .query(
                &Query::new()
                    .of_type("Event")
                    .with_index("participant_email", addr.clone()),
            )
            .await?;
        println!("events with participant {addr}: {}", events.len());
    }

    // The log and undo: the last import step can be undone and redone.
    let stats = store.log_stats().await?;
    println!(
        "log: {} entries, {} unsynced, {} payload bytes",
        stats.entries, stats.unsynced, stats.payload_bytes
    );
    let before = store.count(&Query::new()).await?;
    store.undo(ACTOR, 1).await?;
    println!(
        "objects {before} -> {} after undo",
        store.count(&Query::new()).await?
    );
    store.redo(ACTOR, 1).await?;
    println!(
        "objects back to {} after redo",
        store.count(&Query::new()).await?
    );
    Ok(())
}
