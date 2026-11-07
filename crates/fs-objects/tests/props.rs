//! Property tests for undo/redo and compaction invariants.

use fs_objects::*;
use proptest::prelude::*;
use serde_json::json;

const ME: &str = "me";

#[derive(Clone, Debug)]
enum Op {
    Put(usize, u8),
    Delete(usize),
    Undo(usize),
    Redo(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0usize..3, any::<u8>()).prop_map(|(s, v)| Op::Put(s, v)),
        2 => (0usize..3).prop_map(Op::Delete),
        3 => (0usize..4).prop_map(Op::Undo),
        3 => (0usize..4).prop_map(Op::Redo),
    ]
}

fn obj(id: ObjectId, v: u8) -> StoredObject {
    let mut e = Envelope::new("Note", format!("n{v}"));
    e.id = id;
    StoredObject::new(e, "application/json", json!({ "v": v })).with_index("v", v.to_string())
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

type Slot = Option<u8>;

async fn snapshot(s: &TursoStore, ids: &[ObjectId]) -> Vec<Slot> {
    let mut v = vec![];
    for id in ids {
        v.push(
            s.get(*id)
                .await
                .unwrap()
                .map(|o| o.content["v"].as_u64().unwrap() as u8),
        );
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, .. ProptestConfig::default() })]

    /// The store behaves like a reference model with undo/redo stacks.
    #[test]
    fn undo_redo_matches_model(ops in proptest::collection::vec(op(), 1..25)) {
        rt().block_on(async {
            let s = TursoStore::open_in_memory().await.unwrap();
            let ids: Vec<ObjectId> = (0..3).map(|_| ObjectId::new()).collect();
            let mut state: Vec<Slot> = vec![None; 3];
            // (slot, before, after)
            let mut undo: Vec<(usize, Slot, Slot)> = vec![];
            let mut redo: Vec<(usize, Slot, Slot)> = vec![];
            for op in ops {
                match op {
                    Op::Put(i, v) => {
                        s.put(obj(ids[i], v), ME).await.unwrap();
                        undo.push((i, state[i], Some(v)));
                        state[i] = Some(v);
                        redo.clear();
                    }
                    Op::Delete(i) => {
                        let r = s.delete(ids[i], ME).await;
                        if state[i].is_some() {
                            r.unwrap();
                            undo.push((i, state[i], None));
                            state[i] = None;
                            redo.clear();
                        } else {
                            prop_assert!(matches!(r, Err(Error::NotFound(_))));
                        }
                    }
                    Op::Undo(n) => {
                        let done = s.undo(ME, n).await.unwrap();
                        let k = n.min(undo.len());
                        prop_assert_eq!(done.len(), k);
                        for _ in 0..k {
                            let e = undo.pop().unwrap();
                            state[e.0] = e.1;
                            redo.push(e);
                        }
                    }
                    Op::Redo(n) => {
                        let done = s.redo(ME, n).await.unwrap();
                        let k = n.min(redo.len());
                        prop_assert_eq!(done.len(), k);
                        for _ in 0..k {
                            let e = redo.pop().unwrap();
                            state[e.0] = e.2;
                            undo.push(e);
                        }
                    }
                }
                prop_assert_eq!(snapshot(&s, &ids).await, state.clone());
                let us = s.undo_state(ME).await.unwrap();
                prop_assert_eq!((us.undoable, us.redoable), (undo.len(), redo.len()));
            }
            // index rows follow the content
            for (i, id) in ids.iter().enumerate() {
                let hits = s.query(&Query::new().with_index("v", state[i].map(|v| v.to_string()).unwrap_or("none".into()))).await.unwrap();
                prop_assert_eq!(hits.iter().any(|o| o.envelope.id == *id), state[i].is_some());
            }
            // log ids strictly increase
            let log = s.activities(&ActivityFilter::default()).await.unwrap();
            prop_assert!(log.windows(2).all(|w| w[0].id < w[1].id));
            Ok(())
        })?;
    }

    /// Compaction never changes current state, never drops unsynced entries,
    /// respects the step limit for synced ones, and leaves undo/redo consistent.
    #[test]
    fn compaction_invariants(
        ops in proptest::collection::vec(op(), 1..25),
        sync_at in 0usize..30,
        keep in 0usize..8,
    ) {
        rt().block_on(async {
            let s = TursoStore::open_in_memory().await.unwrap();
            let ids: Vec<ObjectId> = (0..3).map(|_| ObjectId::new()).collect();
            for op in ops {
                match op {
                    Op::Put(i, v) => { s.put(obj(ids[i], v), ME).await.unwrap(); }
                    Op::Delete(i) => { let _ = s.delete(ids[i], ME).await; }
                    Op::Undo(n) => { s.undo(ME, n).await.unwrap(); }
                    Op::Redo(n) => { s.redo(ME, n).await.unwrap(); }
                }
            }
            let before_state = snapshot(&s, &ids).await;
            let log = s.activities(&ActivityFilter::default()).await.unwrap();
            let synced_n = sync_at.min(log.len());
            if synced_n > 0 {
                s.mark_synced(log[synced_n - 1].id).await.unwrap();
            }
            s.set_retention(Retention::steps(keep)).await.unwrap();
            let rep = s.compact().await.unwrap();

            prop_assert_eq!(snapshot(&s, &ids).await, before_state.clone());
            let after = s.activities(&ActivityFilter::default()).await.unwrap();
            // every unsynced entry survived, in order
            let unsynced_before: Vec<_> = log[synced_n..].iter().map(|a| a.id).collect();
            let unsynced_after: Vec<_> = after.iter().filter(|a| !a.synced).map(|a| a.id).collect();
            prop_assert_eq!(unsynced_after, unsynced_before);
            // result is a suffix of the original log
            let tail: Vec<_> = log[log.len() - after.len()..].iter().map(|a| a.id).collect();
            prop_assert_eq!(after.iter().map(|a| a.id).collect::<Vec<_>>(), tail);
            // limit: either within `keep`, or the excess is all unsynced
            prop_assert!(after.len() <= keep.max(log.len() - synced_n));
            prop_assert_eq!(rep.remaining, after.len());

            // redo everything, remember that state; undo everything, redo everything: no
            // conflicts, same state (also proves retained history is self-consistent)
            let us = s.undo_state(ME).await.unwrap();
            s.redo(ME, us.redoable).await.unwrap();
            let top = snapshot(&s, &ids).await;
            let us = s.undo_state(ME).await.unwrap();
            s.undo(ME, us.undoable).await.unwrap();
            let us = s.undo_state(ME).await.unwrap();
            prop_assert_eq!(us.undoable, 0);
            s.redo(ME, us.redoable).await.unwrap();
            prop_assert_eq!(snapshot(&s, &ids).await, top);
            Ok(())
        })?;
    }
}
