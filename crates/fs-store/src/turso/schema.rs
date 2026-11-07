//! SQL schema (tables and indexes) of the Turso backend.
//!
//! Pattern: none (declarative data) — kept apart so the migration can reuse it.

/// Current schema version (table `schema_version`). 1 = fs-store 0.1 (no version table), 2 = 0.2/0.3, 3 = 0.4.
pub const SCHEMA_VERSION: i64 = 3;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS objects (
    id TEXT PRIMARY KEY,
    iri TEXT,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    context TEXT,
    attributed_to TEXT,
    in_reply_to TEXT,
    published INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    content TEXT NOT NULL,
    original BLOB,
    original_type TEXT,
    addressing TEXT NOT NULL,
    sensitive TEXT NOT NULL,
    describes TEXT,
    start_time INTEGER,
    end_time INTEGER,
    name_sort TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS activities (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    actor TEXT NOT NULL,
    at INTEGER NOT NULL,
    target TEXT,
    payload TEXT NOT NULL,
    synced INTEGER NOT NULL DEFAULT 0,
    redacted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS unlogged_ids (object_id TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS purged_objects (object_id TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS index_fields (
    object_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    value_norm TEXT NOT NULL DEFAULT ''
);
";

/// Tables added in version 3 (also run by the migration from 1 and 2).
pub(super) const V3_TABLES: &str = "
CREATE TABLE IF NOT EXISTS unlogged_ids (object_id TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS purged_objects (object_id TEXT PRIMARY KEY);
";

/// Column added to `activities` in version 3 (the migration from 1 and 2 runs it).
pub(super) const V3_ALTER: &str =
    "ALTER TABLE activities ADD COLUMN redacted INTEGER NOT NULL DEFAULT 0;";

/// Indexes (all `IF NOT EXISTS`; run after the tables/columns exist, also by the migration).
pub(super) const INDEXES: &str = "
CREATE INDEX IF NOT EXISTS objects_type ON objects (type, updated);
CREATE INDEX IF NOT EXISTS objects_context ON objects (context);
CREATE INDEX IF NOT EXISTS objects_attributed ON objects (attributed_to);
CREATE INDEX IF NOT EXISTS objects_name_sort ON objects (name_sort);
CREATE INDEX IF NOT EXISTS objects_start ON objects (start_time);
CREATE INDEX IF NOT EXISTS objects_end ON objects (end_time);
CREATE INDEX IF NOT EXISTS objects_describes ON objects (describes);
CREATE INDEX IF NOT EXISTS activities_actor ON activities (actor, id);
CREATE INDEX IF NOT EXISTS activities_object ON activities (object_id, id);
CREATE INDEX IF NOT EXISTS activities_synced ON activities (synced, id);
CREATE INDEX IF NOT EXISTS index_fields_kv ON index_fields (key, value);
CREATE INDEX IF NOT EXISTS index_fields_obj ON index_fields (object_id);
";

/// Column list of `objects`, in the order [`super::codec::decode_object`] expects.
pub(super) const OBJECT_COLS: &str = "id, iri, type, name, context, attributed_to, in_reply_to, published, updated, \
                           content_type, content, original, original_type, addressing, sensitive, \
                           describes, start_time, end_time, name_sort";

/// Column list of `activities`, in the order [`super::codec::decode_activity`] expects.
pub(super) const ACTIVITY_COLS: &str = "id, kind, object_id, actor, at, target, payload, synced";
