//! Unit tests of the pure undo/redo stack computation.

use fs_store::history::{Step, compute};
use fs_store::*;

fn id(n: u128) -> ActivityId {
    ActivityId::from_uuid(uuid::Uuid::from_u128(
        (n << 64) | (0x7u128 << 76) | (0b10u128 << 62),
    ))
    .unwrap()
}

fn step(n: u128, kind: ActivityKind, target: Option<u128>) -> Step {
    Step {
        id: id(n),
        kind,
        target: target.map(id),
    }
}

#[test]
fn stacks_unit() {
    use ActivityKind::*;
    let s = compute(&[
        step(1, Create, None),
        step(2, Update, None),
        step(3, Undo, Some(2)),
    ]);
    assert_eq!(s.undo, vec![id(1)]);
    assert_eq!(s.redo, vec![(id(2), id(3))]);
    // redo = Undo of the Undo
    let s = compute(&[
        step(1, Create, None),
        step(2, Undo, Some(1)),
        step(3, Undo, Some(2)),
    ]);
    assert_eq!(s.undo, vec![id(1)]);
    assert!(s.redo.is_empty());
    // new work clears redo
    let s = compute(&[
        step(1, Create, None),
        step(2, Undo, Some(1)),
        step(3, Create, None),
    ]);
    assert_eq!(s.undo, vec![id(3)]);
    assert!(s.redo.is_empty());
    // compacted targets are ignored
    let s = compute(&[step(5, Undo, Some(1)), step(6, Undo, Some(5))]);
    assert_eq!(s, Default::default());
    // redo entries below an Undo of a compacted step are unreachable and dropped
    let s = compute(&[
        step(2, Create, None),
        step(3, Undo, Some(2)),
        step(4, Undo, Some(1)),
    ]);
    assert!(s.undo.is_empty() && s.redo.is_empty(), "{s:?}");
    // ... but entries undone after it stay
    let s = compute(&[
        step(4, Undo, Some(1)),
        step(5, Create, None),
        step(6, Undo, Some(5)),
    ]);
    assert_eq!(s.redo, vec![(id(5), id(6))]);
    // payloads are irrelevant to stacks
}
