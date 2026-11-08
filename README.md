# FreeSynergy Objects — one place for what programs keep

**This is a building block. It is why your data can be undone, kept private and moved without each program inventing its own database.**

FreeSynergy Objects is the object store that every FreeSynergy program embeds. Programs never speak SQL. They keep objects: a contact, an event, a task, a message. Each object has a common outer shell (type, name, who it belongs to, times) and its own content, and every change is written to a log.

What is different here: the log is what makes undo and redo work, also after a restart. It is also what lets the hub tell other programs what changed. Objects marked as sensitive are refused by a store that is not encrypted. There is a separate, deliberately unlogged path for data that must not carry a time, such as anonymous ballots. The shell follows ActivityStreams 2.0, and ids are time-ordered UUIDs.

## What it can do today

Latest tag: `v0.6.0`.

- Store, read, query and delete objects; filter by type, context, owner and fields; order by name or time; page through results.
- Change an object and write its log entry in one step, all or nothing.
- Undo and redo per actor, persistent.
- Retention and compaction of the log; removal of everything logged about one object.
- Encryption at rest through a key provider (Turso page encryption, which is experimental).
- A change feed for programs that want to react.
- A store identity, so programs can tell which store they have.
- Example: `cargo run -p fs-objects --example contacts`.

## What is not there yet

- No key vault, so keys come from the caller.
- Search is substring and prefix only; full-text search is not switched on.
- No remote (HTTPS) store. Large originals are a draft and answer "not supported".
- The store is built on Turso, which is still young; we pin an exact version.

## Part of FreeSynergy

Objects is what lets programs share the same kinds of things. It used to be called fs-store. FreeSynergy is a family of programs that are free for everyone, forever, and that keep your data yours. The documentation, the manifesto and the overall plan live in [fs-documentation](https://github.com/FreeSynergy/fs-documentation); the reading version is at <https://freesynergy.github.io/fs-documentation/>. Start with the [manifesto](https://freesynergy.github.io/fs-documentation/manifesto/) if you want to know why we do this.


## For developers

- Workspace crate: `fs-objects`. `Store` is a facade over seven small traits (`ObjectRepository`, `ObjectQuery`, `ActivityLog`, `ChangeFeed`, `UndoRedo`, `HistoryRetention`, `StoreInfo`); depend only on the one you need. First backend: `TursoStore`.
- The shared types of the envelope live with the contracts in [fs-interfaces](https://github.com/FreeSynergy/fs-interfaces) (`common.capnp`).
- Build and test: `cargo build --workspace` and `cargo test --workspace`.
- Design, schema and decisions: [DESIGN.md](DESIGN.md).
- Licence: MIT OR Apache-2.0.
