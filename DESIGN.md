# fs-store – Design

`fs-store` is the object store every FreeSynergy program embeds (ADR 0019). Programs never speak SQL or Turso;
they use the `Store` trait. The first backend is `TursoStore` (embedded `turso` crate, **no Turso sync**).

## Structure and pattern map (ADR 0043)

`Store` is a **facade** over seven focused traits (module `fs_store::traits`): `ObjectRepository` (put, put_and_log,
get, delete), `ObjectQuery` (query, count), `ActivityLog` (log, activities), `ChangeFeed` (subscribe), `UndoRedo`
(undo, redo, undo_state), `HistoryRetention` (retention, compaction, mark_synced, purge_object, log_stats), `StoreInfo`
(identity, is_encrypted). Any type implementing all
seven is a `Store` (blanket impl). Every file is one concept, <= 200 lines (tests included).

| Module | Pattern | Why |
|---|---|---|
| `store/facade.rs` | Facade | one name (`Store`) for dependents, `Arc<dyn Store>` unchanged |
| `store/{repository,search,log,feed,undo,retention}.rs` | Strategy (interface segregation) / Repository | consumers depend on the one trait they need; backends are swappable (remote later) |
| `store/query.rs` | Builder | `Query::new().of_type(..).name_contains(..)`, backend-neutral |
| `store/cond.rs` | Interpreter-style data | conditions are values; the backend translates them |
| `store/feed.rs`, `turso/subscribe.rs` | Observer | `broadcast` channel, sent after commit |
| `turso/guard.rs` | Guard | logged paths ask whether an id belongs to the unlogged path; the mark is written in the same transaction as the unlogged write |
| `store/unlogged.rs`, `turso/unlogged.rs` | Repository (separate narrow trait) | data that must not carry a time; outside the `Store` facade so nobody reaches it by accident |
| `store/undo.rs`, `turso/undo.rs`, `history.rs`, `model/activity.rs` | Command | activities carry before/after images; undo applies the inverse, stacks are pure (`history::compute`) |
| `turso/*` (whole tree) | Adapter | Turso behind the `Store` traits |
| `turso/store.rs` (`apply`) | Command executor | one change (kind, before, after) written together with its activity |
| `turso/tx.rs` | Template method | `BEGIN IMMEDIATE`, step, `COMMIT`/`ROLLBACK`; Drop cannot await a rollback, so a closure wrapper replaces the RAII guard |
| `turso/migration.rs` | Template method | detect version, run the matching step in one transaction |
| `turso/open.rs` | Factory | file / in-memory / with options |
| `turso/encryption.rs`, `crypto.rs` | Strategy | `KeyProvider` supplies the key; Turso page encryption is applied |
| `store/purge.rs`, `turso/purge.rs` | Template method | `purge_object`: one transaction (remember high-water mark, count, delete, mark object), then checkpoint + `VACUUM` on disk stores |
| `store/info.rs`, `turso/info.rs`, `turso/identity.rs` | Strategy (segregation) / Adapter / Template method | `StoreInfo`: who is this store, is it encrypted; the identity row is made in the migration transaction |
| `model/store_id.rs` | Newtype | `StoreId` is not an object or activity id |
| `store/originals.rs`, `turso/originals.rs` | Strategy (policy) / Adapter | `LargeOriginals`: draft, Turso answers `Error::Backend("... not supported")` |
| `turso/payload.rs` | Specification | `is_purged(object)` decides whether a write logs images |
| `turso/{objects,activities}.rs` | Repository (internal) | only modules that know the table layout |
| `turso/query_sql.rs` | Builder (internal) | `Query` to `WHERE`/`ORDER BY` + parameters |
| `turso/{codec,rows,sql}.rs` | Adapter | foreign rows/values to typed model |
| `store/retention.rs`, `turso/{retention,compaction}.rs` | Strategy / Template method | `Retention` is the policy; `compact_with` is the fixed skeleton |
| `model/ids.rs` | Newtype | `ObjectId` and `ActivityId` cannot be mixed up |
| `model/{envelope,object}.rs` | Newtype aliases / Builder | typed fields; `with_original`, `with_index` |

## Migration for dependents

None required: `Store`, `TursoStore`, `Query`, ... keep their paths and signatures; fs-contacts, fs-calendar, fs-maps
and fs-hub compile unchanged. New: the focused traits in `fs_store::traits`. Do not import `Store` and a focused trait
for the same concrete type in one scope (calls become ambiguous); use `dyn ObjectQuery` etc. for narrow dependencies.

## State

Version 0.4.0 (schema version 4).

| Area | State |
|---|---|
| Types: `ObjectId`/`ActivityId` (UUIDv7), `Envelope`, `StoredObject`, `Blob`, `Activity`, `ChangeSet` | done |
| `Store` trait (async, `async-trait`, object-safe) | done; since the ADR 0043 refactor a facade over six focused traits (`fs_store::traits`) |
| `TursoStore` (file or in-memory) | done, `turso = "=0.8.3"` pinned (default features off: no mimalloc, no tantivy FTS) |
| Atomic object change + activity | done, one `BEGIN IMMEDIATE` transaction; rollback test via fault injection |
| `put_and_log(object, actor, extra)` (0.2) | done: `Create`/`Update` plus any number of log-only extras (`Accept`, `TentativeAccept`, ...) in one transaction; fault-injection test (`inject_fault_before_extra`) proves all-or-nothing, nothing published on failure |
| `ActivityKind` (0.2) | added `Invite`, `TentativeAccept`, `Join`, `Leave`, `Like`, `Move` (log-only); stored as the plain string name, so 0.1 data reads unchanged |
| Query (0.2) | type / context / attributed_to / exact index pairs (as before) plus `Cond` filters: `Op::{Eq,Gt,Ge,Lt,Le,Prefix,Contains}` on index fields, name, `describes` (Eq only) and envelope times (`start`/`end`/`published`/`updated`); `any` = OR-group, everything AND-ed; `limit`/`offset`/`count` unchanged |
| Ordering (0.2) | `order_by_index(key)` (smallest normalised value, e.g. contacts "surname, given"); `NameAsc/NameDesc` case-insensitive and Unicode-normalised via `name_sort`; `StartAsc/StartDesc` (undated last) |
| Envelope (0.2) | optional `describes` (AS2 Profile), `start_time`/`end_time` (AS2 `startTime`/`endTime`) as columns |
| Schema versioning (0.2) | table `schema_version`; version 2 now; a 0.1 database (no table) is migrated in one transaction on open (ALTER + backfill of sort keys); a newer database is refused. Migration test builds a verbatim 0.1 file |
| Full-text search | substring/prefix only (below); tokenised FTS **not yet** (Turso's tantivy-based `fts` feature is not enabled) |
| Change stream (`subscribe`) | done: `tokio::sync::broadcast<Activity>`, sent after commit |
| Undo / redo per actor | done, persistent, survives restart |
| Retention (`max_age`, `max_steps`), `compact()`, `clear_history()`, `mark_synced()` | done |
| Encryption at rest | done via `KeyProvider` + Turso page encryption (experimental in Turso); no key vault yet |
| Sensitive data (ADR 0029) | envelope carries DPV categories; unencrypted store refuses sensitive objects unless opted out. No auto-link logic here (hub concern) |
| Hub sync | only the data side: `activities(unsynced_only / after)` + `mark_synced`. No IPC (belongs to fs-hub) |
| `UnloggedObjects` (0.3): `put_unlogged`, `read_unlogged`, `delete_unlogged`: no activity at any moment, no event, no undo, times as given. A separate trait, not part of `Store`; for anonymous ballots (`fs-surveys`). Old pages can stay on disk until reused. **0.4: the logged calls (`put`, `put_and_log`, `delete`, `get`, `query`, `log`) refuse an id written through it** (`Error::Invalid`, table `unlogged_ids`, kept after a delete) | done |
| Log purge (0.4) | done, see "Log purge of one object" |
| Identity and `is_encrypted` (0.4) | done: `StoreInfo`, `StoreId`, schema version 4 |
| Large originals (0.4) | **draft, `Unsupported`**: trait `LargeOriginals` + 4 ignored target tests, see below |
| Remote (HTTPS) backend | not started; trait is backend-neutral |
| Tests | see `cargo test` (4 ignored target tests for large originals); split by feature in `tests/` (crud, query, search, ordering, envelope, kinds, log, undo, retention, encryption, migration, migration_v2, migration_v3, history, props, unlogged, unlogged_guard, purge, purge_ids, purge_undo, purge_redacted, identity, large_originals; helpers in `tests/common`) plus unit tests for `sort_key` |
| Code layout (ADR 0043) | done: `src/{model,store,turso}/` trees, no file over 200 lines, `#![deny(missing_docs)]`, `Pattern:` line in every module doc |
| Example | `cargo run -p fs-store --example contacts` imports the demo contacts (`Profile`) and events (`Event`) |

## Object model (ADR 0020, 0030, 0032)

* `Envelope`: `id` (UUIDv7), `iri`, `type`, **`name` (mandatory, non-empty)**, `attributedTo`, `to/cc/bto/bcc/audience`,
  `published`, `updated`, `context`, `inReplyTo`, `sensitive` (DPV category strings). JSON names follow AS2.
* `StoredObject` = envelope + `content_type` + `content` (JSON in the domain standard) + optional `original` (`Blob`) +
  `index` (domain-extracted key -> values, supplied by the caller; domain crates decide what to extract).
* The **original is immutable** (ADR 0030): it can be set on creation only; an update without one keeps it, an update
  with a different one fails with `OriginalImmutable`.
* The store owns timestamps: `updated` = now on every put, `published` of an existing object is kept. Timestamps are
  microsecond precision.
* Internal ids are UUIDv7. Activity ids are made strictly monotonic per store (same-millisecond bump), so id order is log order.

## Schema (Turso)

* `schema_version(version)` - one row, currently 4 (1 = 0.1, which had no table; 2 = 0.2/0.3; 3 = unreleased, purge and unlogged marks; 4 = 0.4, store identity).
* `objects(id PK, iri, type, name, context, attributed_to, in_reply_to, published, updated, content_type, content JSON,
  original BLOB, original_type, addressing JSON, sensitive JSON, describes, start_time, end_time, name_sort)`
* `activities(id PK UUIDv7, kind, object_id, actor, at, target, payload JSON, synced)` - append-only except for compaction
  and the `synced` flag.
* `index_fields(object_id, key, value, value_norm)` - multi-valued extracted columns (emails, phones, uid, ...); `value_norm` is the search key.
* `meta(key, value)` - retention policy, `store_id` (identity), `activity_high` (highest activity id ever issued).
* `unlogged_ids`, `purged_objects` - marks for the unlogged guard and the log purge.

## Search, ranges and ordering (0.2)

* **Sort/search key** `sort_key(s)` = NFC, `to_lowercase()` (full Unicode, not ASCII-only), NFC again. Computed in Rust and stored
  (`objects.name_sort`, `index_fields.value_norm`) because the SQL engine only folds ASCII. Consequences: ordering is by code point of
  that key, not a locale collation (`Ärger` sorts after `Zebra`; `ß` is not `ss`); precomposed and decomposed forms are equal.
  Changing the key function later needs a reindex migration (schema version bump).
* `Prefix` / `Contains` run `LIKE ? ESCAPE '\'` on the key column with `\`, `%`, `_` escaped in the needle (tested). They are
  scans (no index helps `%x%`).
* `Gt/Ge/Lt/Le` on index fields compare the **raw stored text, binary**: store ISO-8601 UTC strings (`2026-03-01T09:00:00Z`) and
  they order chronologically. Envelope times are integers (microseconds), compared as instants; a missing value never matches.
* `start_time`/`end_time` in the envelope are indexed columns; "events overlapping `[from, to)`" is
  `start < to AND end > from`. The store does not interpret them (a recurring series sets `end_time` to its last end, or a far-future
  sentinel if unbounded; that is the domain crate's call).
* `describes` supports `Eq` only.

## Activity log and undo (ADR 0020, 0042)

* Every state change writes the object row, its index rows and the activity in **one transaction**. Turso CDC is not used.
* Payload of `Create/Update/Delete/Undo` is a `ChangeSet {before, after}` (full images). `after` is what the hub applies;
  both images make undo possible without replaying. The original is included only when the change creates or removes the
  object (it never changes, so copying it into every update would waste space).
* Log-only kinds (`Add, Remove, Offer, Invite, Accept, TentativeAccept, Reject, Read, Join, Leave, Like, Move`) are written with `Store::log`
  or, together with an object change, `Store::put_and_log`; they carry a caller payload and
  are not undoable. No RFC 6902 patch is generated: the images carry the same information.
* **Undo** reverts the last `n` undoable activities of one actor, newest first, atomically, and appends an `Undo` activity
  (`target` = reverted activity). **Redo** is an `Undo` of that `Undo` (pure AS2). New work by the actor clears the redo
  stack. Stacks are computed from the log (`history::compute`), so they are persistent.
* Undo checks that the object still equals the activity's `after` image; if someone else (another actor, or the same
  actor in a way outside the stack) changed it, the whole call fails with `Conflict` and changes nothing.

## Retention and compaction (ADR 0042)

* `Retention { max_age, max_steps }`, either or both, stored in `meta`. With both set, an entry is eligible as soon as it
  violates **either** limit.
* `compact()` / `compact_at(now)` delete eligible entries that are **synced**. Since sync acknowledgement (`mark_synced(up_to)`)
  is a prefix of the log and eligibility is a prefix too, the log after compaction is always a suffix of the old log.
  Unsynced entries are never removed (reported as `kept_unsynced`). Current state lives in `objects` and is never touched.
* "Squash" = the intermediate steps fall away; what stays is the current object plus the retained recent steps. A deleted
  object's old image disappears with its compacted `Delete` entry.
* After compaction, an `Undo` whose target is gone is ignored, and redo entries that could only be reached through it are
  dropped (otherwise redo would hit a conflict). Property tests check this.
* `clear_history()` = "reset undo history": drops every synced entry. `log_stats()` gives the numbers for a size display.
* The store never compacts by itself; the program calls `compact()` (e.g. on save or idle).

## Encryption at rest (ADR 0024, 0029)

Turso's page encryption (`experimental_encryption` + `EncryptionOpts`) works in the embedded engine **without** the sync
engine (which does not support local encryption; we do not use it). `KeyProvider::key_for(store_name)` returns an
`EncryptionKey { cipher, key }`; `OpenOptions.key_provider` enables it. The whole file and WAL are encrypted (tested: a
needle string is on disk in a plain store and absent in an encrypted one; a wrong key cannot read).
The key vault (ADR 0024) will implement `KeyProvider`. Limits: Turso marks this feature experimental, there is no
passphrase KDF (the provider must supply raw 16/32-byte keys), and rekeying means rewriting the database.
Sensitive objects (non-empty `envelope.sensitive`) are refused by an unencrypted store (`EncryptionRequired`) unless
`allow_sensitive_unencrypted` is set.

## Concurrency

One `turso::Connection` behind a `tokio::sync::Mutex`; each operation is serialised and transactional. Turso's
multi-process mode is experimental and not used: one process owns one database file (the hub never opens program files).

## Turso notes

* Pinned `=0.8.3` (stable; 0.9.0-pre exists). Default features are off to avoid mimalloc and the tantivy FTS stack. Even so,
  `turso` pulls `turso_sync_engine` as a transitive dependency; it is compiled but unused.
* Used: `Builder::new_local`, `execute/query/execute_batch`, manual `BEGIN IMMEDIATE/COMMIT/ROLLBACK`. Worked as expected:
  `EXISTS` subqueries, `LIMIT/OFFSET`, `rowid` ordering, `COALESCE/SUM/LENGTH`, `:memory:`.
* Dependency licences: all MIT/Apache-compatible (a few BSD-3/ISC/Unicode-3.0/Unlicense, and a build-time `bindgen`).

## Open points

* Tokenised full-text search / relevance ranking (Turso FTS or own index table); locale collation (CLDR) for ordering.
* `describes` is exact-match only; `location` (AS2) is not an envelope field yet.
* A 0.1 database is migrated without `start_time`/`end_time`: domain crates must rewrite those objects to fill them.
* Delete/undo failures can leave a gap in activity ids (harmless; `put_and_log` re-reads the last id after a rollback).
* Remote backend over JSON/HTTPS.
* Tombstones for synced+compacted deletes if the hub needs them later (ADR 0020 open detail).
* Whether `max_steps` should count per actor instead of globally.

## 0.4 decisions

* **Unlogged ids are marked, logged calls refuse them.** `put_unlogged` records the id in `unlogged_ids` in its own
  transaction. `put`, `put_and_log`, `delete`, `get`, `log` on such an id return `Error::Invalid` (no new `Error`
  variant: the enum is exhaustive, a variant would break dependents). `query` fails if any hit is a marked id, so
  keep unlogged objects in a store of their own or in a type no logged query asks for; `count` is not refused (a number
  only). `read_unlogged`, `put_unlogged`, `delete_unlogged` are unchanged; the mark outlives a delete.
  **Limit:** an object written unlogged by 0.2/0.3 has no mark. It cannot be told from a logged object whose
  entries were compacted, so the migration does not guess; the mark appears with the next `put_unlogged` of that id
  (ballot boxes and the guard state are rewritten constantly; a mail key only when replaced). A store that
  mixed both paths in 0.3 should call `put_unlogged` once on each such id after the upgrade.
* Schema version 3 was never tagged (commit 60df691); the tables and the `redacted` column of 3 are created in the step to 4 only where missing, so a real version 3 file migrates (tested from a verbatim 60df691 schema). 0.3 cannot open a version 3 or 4 file.

## Log purge of one object (ADR 0029, 0.4)

`HistoryRetention::purge_object(id) -> PurgeReport` removes **every** log entry of the object (`Create`, `Update`, `Delete`,
`Undo`, log-only extras; synced or not). The object row is untouched. One transaction; on disk stores it then runs a WAL
checkpoint and `VACUUM` (Turso 0.8.3 ignores `secure_delete`), tested: the needle leaves the database and the WAL. The default
method answers `Error::Backend("... not supported")`, so older implementations of the trait compile.
`PurgeReport` is `#[non_exhaustive]` and has `file_cleaned`: the commit is what removes the entries; if the file clean-up cannot run
(another reader holds the file, "database is locked") the call still returns `Ok` with `file_cleaned: false`, and
`HistoryRetention::clean_files()` repeats only the clean-up (`Ok(false)` while still busy). Tested with an open read transaction.

* **Undo/redo:** entries go together, so no `Undo` loses its target. Other objects' entries and their stacks are unchanged
  (tested: undo and redo on a second object across the purge, also a redo entry below a purged undo).
* **Sync:** the hole in the middle is fine for `mark_synced` (a mark by id range) and compaction (a prefix by id). Unsynced
  entries removed are reported (`unsynced_removed`): the hub will never get them.
* **Trap decided:** the next `put`/`delete` on a purged object would write a `before` image with the full content again.
  The object id is therefore recorded in `purged_objects` and later writes are logged **without images**. Such an entry is
  not undoable. Its payload is exactly `{"redacted":true}`: **no `before` and no `after` key**, `kind` unchanged (a `Create`
  stays `Create`), `Activity::is_redacted()` is true, and `Activity::change_set()` returns `Error::Invalid` for it.
  **Hub contract:** a log entry with `payload.redacted == true` carries no object state and must **not** be read as a
  deletion (`after` absent is not `after: null`). The consumer skips the entry and fetches the object's current copy from
  the store (it is not applied from the log). Chosen over documenting the trap because a silent re-leak defeats the purpose.
* **Ids:** the highest activity id ever issued is kept in `meta.activity_high` (written by purge and compaction); the last id
  after a reopen is the larger of log maximum and that mark, so `ActivityId::next_after` stays above what the hub accepted.
* **Scope:** only this store. The hub's own log needs the same call.

## Store identity (0.4)

`StoreInfo::identity() -> StoreId` (a random UUIDv4, no creation time) and `is_encrypted()`. The id is made once, in the
migration transaction that creates the database (or lifts a version 2/3 file to 4), kept in `meta.store_id`, never in the
activity log and not changeable by the API. A copy of the whole file set (database and write-ahead log) keeps it; a new
store gets its own (tested, also that a migration makes exactly one). It is no secret but makes the server
recognisable: it must not go to the hub, into objects or into content unless someone decided that. Use: detect two
handles to the same store, and let programs check `is_encrypted()` (ADR 0018). Custom backends of the focused traits must
now also implement `StoreInfo` to be a `Store`; the same holds for a direct `impl Store` (a break in `fs-messenger`
`tests/report_write_fails.rs`).

**Copies.** Copy the database **with** its `-wal` file, or only after the store is closed or `clean_files()` ran: the `.db` alone
of an open store is an older state (a new identity and no objects). A restored backup or a clone of the file set carries the
same identity as the original; two live copies are then two handles with one id, and the program has to tell them apart.

## Large originals (0.4, draft)

Measured by Thread 2: a 5 MiB message costs 6.9 MiB of log (the original is base64 in the `Create`/`Delete` image).
**Design:**
* Table `blobs(hash PK, media_type, size, bytes)`, hash = SHA-256 (the crate already has `sha2`). An original of at least
  the threshold (`LargeOriginals::set_original_threshold`) is written once there; the object row and the log images carry
  an `OriginalRef {hash, size, media_type}` instead of the bytes. Equal bytes are stored once. `get` resolves the reference.
* The index fields are left out of the images (they are derived from the content and rebuilt on undo).
* **Delete / undo:** a deleted object's blob stays while a log entry references it, so undoing the `Delete` restores the
  original. **Log purge / compaction:** after removing entries, blobs with no object row and no remaining entry are deleted
  in the same transaction (then the purge's `VACUUM`). **Immutability (ADR 0030)** is unchanged: same hash means same original.
* **Encryption:** only on an encrypted store; a plain store refuses a threshold (`Error::Invalid`), so a shared blob
  table never holds content the purge cannot reach in a store without file encryption.
* **Not built:** the Turso backend implements nothing yet; every method answers `Error::Backend("... not supported ...")`.
  The four target tests in `tests/large_originals.rs` are `#[ignore]` and fail until it exists. Open:
  the threshold applies to originals written afterwards only.
