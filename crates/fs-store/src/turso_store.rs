//! [`Store`] backend on the embedded `turso` crate (no Turso sync, ADR 0019).
//!
//! One connection behind an async mutex: every public operation takes the
//! lock, opens `BEGIN IMMEDIATE`, writes the object change **and** its
//! activity, and commits (or rolls back) as one unit.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, SubsecRound, Utc};
use tokio::sync::{Mutex, broadcast};
use turso::{Connection, Value};

use crate::history::{self, Step};
use crate::text::like_escape;
use crate::{
    Activity, ActivityFilter, ActivityId, ActivityKind, Blob, ChangeSet, CompactionReport, Cond,
    Envelope, Error, KeyProvider, LogEntry, LogStats, ObjectId, Op, Order, Query, Result,
    Retention, Store, StoredObject, TimeField, Timestamp, UndoState, sort_key,
};

/// Current schema version (table `schema_version`). 1 = fs-store 0.1 (no version table).
pub const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
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
    synced INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS index_fields (
    object_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    value_norm TEXT NOT NULL DEFAULT ''
);
";

/// Indexes (all `IF NOT EXISTS`; run after the tables/columns exist, also by the migration).
const INDEXES: &str = "
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

const OBJECT_COLS: &str = "id, iri, type, name, context, attributed_to, in_reply_to, published, updated, \
                           content_type, content, original, original_type, addressing, sensitive, \
                           describes, start_time, end_time, name_sort";

/// Options for opening a [`TursoStore`].
#[derive(Clone)]
pub struct OpenOptions {
    /// Name handed to the key provider (default `"fs-store"`).
    pub store_name: String,
    /// If set, the database file (and WAL) is encrypted with this key (ADR 0024).
    pub key_provider: Option<Arc<dyn KeyProvider>>,
    /// ADR 0029: sensitive objects are only accepted by an encrypted store.
    /// Set `true` to opt out (e.g. tests, or when the volume itself is encrypted).
    pub allow_sensitive_unencrypted: bool,
    /// Capacity of the change-stream channel.
    pub subscribe_capacity: usize,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            store_name: "fs-store".into(),
            key_provider: None,
            allow_sensitive_unencrypted: false,
            subscribe_capacity: 1024,
        }
    }
}

struct Inner {
    conn: Connection,
    last_activity: Option<ActivityId>,
}

/// Embedded Turso backend.
pub struct TursoStore {
    // Keep the database handle alive for the lifetime of the connection.
    _db: turso::Database,
    inner: Mutex<Inner>,
    tx: broadcast::Sender<Activity>,
    encrypted: bool,
    allow_sensitive_unencrypted: bool,
    fail_before_activity: AtomicBool,
    fail_before_extra: AtomicBool,
}

fn now() -> Timestamp {
    Utc::now().trunc_subsecs(6)
}

fn ts_from_micros(us: i64) -> Result<Timestamp> {
    DateTime::from_timestamp_micros(us).ok_or_else(|| Error::Backend(format!("bad timestamp {us}")))
}

fn text(v: &Value) -> Result<String> {
    match v {
        Value::Text(s) => Ok(s.clone()),
        other => Err(Error::Backend(format!("expected text, got {other:?}"))),
    }
}
fn opt_text(v: &Value) -> Result<Option<String>> {
    match v {
        Value::Null => Ok(None),
        other => text(other).map(Some),
    }
}
fn int(v: &Value) -> Result<i64> {
    match v {
        Value::Integer(i) => Ok(*i),
        other => Err(Error::Backend(format!("expected integer, got {other:?}"))),
    }
}
fn opt_int(v: &Value) -> Result<Option<i64>> {
    match v {
        Value::Null => Ok(None),
        other => int(other).map(Some),
    }
}
fn opt_ts_val(t: &Option<Timestamp>) -> Value {
    t.map(|t| Value::Integer(t.timestamp_micros()))
        .unwrap_or(Value::Null)
}
fn opt_text_val(s: &Option<String>) -> Value {
    s.as_ref()
        .map(|s| Value::Text(s.clone()))
        .unwrap_or(Value::Null)
}

async fn collect(c: &Connection, sql: &str, params: Vec<Value>) -> Result<Vec<Vec<Value>>> {
    let mut rows = c.query(sql, turso::params_from_iter(params)).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let mut r = Vec::with_capacity(row.column_count());
        for i in 0..row.column_count() {
            r.push(row.get_value(i)?);
        }
        out.push(r);
    }
    Ok(out)
}

async fn exec(c: &Connection, sql: &str, params: Vec<Value>) -> Result<u64> {
    Ok(c.execute(sql, turso::params_from_iter(params)).await?)
}

/// Run `fut` inside `BEGIN IMMEDIATE ... COMMIT`, rolling back on error.
async fn in_tx<T>(c: &Connection, fut: impl Future<Output = Result<T>>) -> Result<T> {
    c.execute("BEGIN IMMEDIATE", ()).await?;
    match fut.await {
        Ok(v) => {
            if let Err(e) = c.execute("COMMIT", ()).await {
                let _ = c.execute("ROLLBACK", ()).await;
                return Err(e.into());
            }
            Ok(v)
        }
        Err(e) => {
            let _ = c.execute("ROLLBACK", ()).await;
            Err(e)
        }
    }
}

fn decode_object(row: &[Value]) -> Result<StoredObject> {
    let addressing: BTreeMap<String, Vec<String>> = serde_json::from_str(&text(&row[13])?)?;
    let get = |k: &str| addressing.get(k).cloned().unwrap_or_default();
    let original = match &row[11] {
        Value::Blob(b) => Some(Blob {
            media_type: opt_text(&row[12])?.unwrap_or_else(|| "application/octet-stream".into()),
            bytes: b.clone(),
        }),
        _ => None,
    };
    Ok(StoredObject {
        envelope: Envelope {
            id: text(&row[0])?.parse()?,
            iri: opt_text(&row[1])?,
            object_type: text(&row[2])?,
            name: text(&row[3])?,
            context: opt_text(&row[4])?,
            attributed_to: opt_text(&row[5])?,
            in_reply_to: opt_text(&row[6])?,
            describes: opt_text(&row[15])?,
            start_time: opt_int(&row[16])?.map(ts_from_micros).transpose()?,
            end_time: opt_int(&row[17])?.map(ts_from_micros).transpose()?,
            published: ts_from_micros(int(&row[7])?)?,
            updated: ts_from_micros(int(&row[8])?)?,
            to: get("to"),
            cc: get("cc"),
            bto: get("bto"),
            bcc: get("bcc"),
            audience: get("audience"),
            sensitive: serde_json::from_str(&text(&row[14])?)?,
        },
        content_type: text(&row[9])?,
        content: serde_json::from_str(&text(&row[10])?)?,
        original,
        index: BTreeMap::new(),
    })
}

async fn attach_index(c: &Connection, objs: &mut [StoredObject]) -> Result<()> {
    for chunk in objs.chunks_mut(200) {
        let ph = vec!["?"; chunk.len()].join(",");
        let params = chunk
            .iter()
            .map(|o| Value::Text(o.envelope.id.to_string()))
            .collect();
        let rows = collect(
            c,
            &format!("SELECT object_id, key, value FROM index_fields WHERE object_id IN ({ph}) ORDER BY rowid"),
            params,
        )
        .await?;
        for r in rows {
            let oid = text(&r[0])?;
            if let Some(o) = chunk.iter_mut().find(|o| o.envelope.id.to_string() == oid) {
                o.index.entry(text(&r[1])?).or_default().push(text(&r[2])?);
            }
        }
    }
    Ok(())
}

async fn load_object(c: &Connection, id: ObjectId) -> Result<Option<StoredObject>> {
    let rows = collect(
        c,
        &format!("SELECT {OBJECT_COLS} FROM objects WHERE id = ?"),
        vec![Value::Text(id.to_string())],
    )
    .await?;
    let Some(r) = rows.first() else {
        return Ok(None);
    };
    let mut v = [decode_object(r)?];
    attach_index(c, &mut v).await?;
    let [o] = v;
    Ok(Some(o))
}

async fn write_object(c: &Connection, o: &StoredObject) -> Result<()> {
    let e = &o.envelope;
    let id = e.id.to_string();
    exec(
        c,
        "DELETE FROM objects WHERE id = ?",
        vec![Value::Text(id.clone())],
    )
    .await?;
    exec(
        c,
        "DELETE FROM index_fields WHERE object_id = ?",
        vec![Value::Text(id.clone())],
    )
    .await?;
    let addressing = serde_json::json!({
        "to": e.to, "cc": e.cc, "bto": e.bto, "bcc": e.bcc, "audience": e.audience
    });
    let (orig, orig_type) = match &o.original {
        Some(b) => (
            Value::Blob(b.bytes.clone()),
            Value::Text(b.media_type.clone()),
        ),
        None => (Value::Null, Value::Null),
    };
    exec(
        c,
        &format!(
            "INSERT INTO objects ({OBJECT_COLS}) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        ),
        vec![
            Value::Text(id.clone()),
            opt_text_val(&e.iri),
            Value::Text(e.object_type.clone()),
            Value::Text(e.name.clone()),
            opt_text_val(&e.context),
            opt_text_val(&e.attributed_to),
            opt_text_val(&e.in_reply_to),
            Value::Integer(e.published.timestamp_micros()),
            Value::Integer(e.updated.timestamp_micros()),
            Value::Text(o.content_type.clone()),
            Value::Text(serde_json::to_string(&o.content)?),
            orig,
            orig_type,
            Value::Text(addressing.to_string()),
            Value::Text(serde_json::to_string(&e.sensitive)?),
            opt_text_val(&e.describes),
            opt_ts_val(&e.start_time),
            opt_ts_val(&e.end_time),
            Value::Text(sort_key(&e.name)),
        ],
    )
    .await?;
    for (k, vals) in &o.index {
        for v in vals {
            exec(
                c,
                "INSERT INTO index_fields (object_id, key, value, value_norm) VALUES (?,?,?,?)",
                vec![
                    Value::Text(id.clone()),
                    Value::Text(k.clone()),
                    Value::Text(v.clone()),
                    Value::Text(sort_key(v)),
                ],
            )
            .await?;
        }
    }
    Ok(())
}

async fn remove_object(c: &Connection, id: ObjectId) -> Result<()> {
    let id = id.to_string();
    exec(
        c,
        "DELETE FROM objects WHERE id = ?",
        vec![Value::Text(id.clone())],
    )
    .await?;
    exec(
        c,
        "DELETE FROM index_fields WHERE object_id = ?",
        vec![Value::Text(id)],
    )
    .await?;
    Ok(())
}

const ACTIVITY_COLS: &str = "id, kind, object_id, actor, at, target, payload, synced";

fn decode_activity(r: &[Value]) -> Result<Activity> {
    Ok(Activity {
        id: text(&r[0])?.parse()?,
        kind: text(&r[1])?.parse()?,
        object: text(&r[2])?.parse()?,
        actor: text(&r[3])?,
        at: ts_from_micros(int(&r[4])?)?,
        target: opt_text(&r[5])?.map(|s| s.parse()).transpose()?,
        payload: serde_json::from_str(&text(&r[6])?)?,
        synced: int(&r[7])? != 0,
    })
}

async fn load_activity(c: &Connection, id: ActivityId) -> Result<Option<Activity>> {
    let rows = collect(
        c,
        &format!("SELECT {ACTIVITY_COLS} FROM activities WHERE id = ?"),
        vec![Value::Text(id.to_string())],
    )
    .await?;
    rows.first().map(|r| decode_activity(r)).transpose()
}

async fn insert_activity(c: &Connection, a: &Activity) -> Result<()> {
    exec(
        c,
        &format!("INSERT INTO activities ({ACTIVITY_COLS}) VALUES (?,?,?,?,?,?,?,?)"),
        vec![
            Value::Text(a.id.to_string()),
            Value::Text(a.kind.as_str().into()),
            Value::Text(a.object.to_string()),
            Value::Text(a.actor.clone()),
            Value::Integer(a.at.timestamp_micros()),
            a.target
                .map(|t| Value::Text(t.to_string()))
                .unwrap_or(Value::Null),
            Value::Text(a.payload.to_string()),
            Value::Integer(a.synced as i64),
        ],
    )
    .await?;
    Ok(())
}

async fn load_steps(c: &Connection, actor: &str) -> Result<Vec<Step>> {
    let rows = collect(
        c,
        "SELECT id, kind, target FROM activities \
         WHERE actor = ? AND kind IN ('Create','Update','Delete','Undo') ORDER BY id",
        vec![Value::Text(actor.into())],
    )
    .await?;
    rows.iter()
        .map(|r| {
            Ok(Step {
                id: text(&r[0])?.parse()?,
                kind: text(&r[1])?.parse()?,
                target: opt_text(&r[2])?.map(|s| s.parse()).transpose()?,
            })
        })
        .collect()
}

fn cmp_sql(op: Op) -> Option<&'static str> {
    Some(match op {
        Op::Eq => "=",
        Op::Gt => ">",
        Op::Ge => ">=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Prefix | Op::Contains => return None,
    })
}

/// `LIKE` pattern with escaped needle (for use with `ESCAPE '\'`).
fn like_pattern(op: Op, needle: &str) -> String {
    let e = like_escape(needle);
    if op == Op::Prefix {
        format!("{e}%")
    } else {
        format!("%{e}%")
    }
}

/// Condition on a column that holds an already-normalised key (`name_sort`, `value_norm`).
fn text_cond(col: &str, op: Op, value: &str, p: &mut Vec<Value>) -> String {
    let key = sort_key(value);
    match cmp_sql(op) {
        Some(c) => {
            p.push(Value::Text(key));
            format!("{col} {c} ?")
        }
        None => {
            p.push(Value::Text(like_pattern(op, &key)));
            format!("{col} LIKE ? ESCAPE '\\'")
        }
    }
}

fn cond_sql(c: &Cond, p: &mut Vec<Value>) -> Result<String> {
    Ok(match c {
        Cond::Index { key, op, value } => {
            let mut inner = vec![Value::Text(key.clone())];
            // Eq and ranges: exact/binary on `value`; Prefix/Contains: normalised.
            let cond = match op {
                Op::Prefix | Op::Contains => text_cond("f.value_norm", *op, value, &mut inner),
                _ => {
                    inner.push(Value::Text(value.clone()));
                    format!("f.value {} ?", cmp_sql(*op).expect("comparison"))
                }
            };
            p.extend(inner);
            format!(
                "EXISTS (SELECT 1 FROM index_fields f WHERE f.object_id = o.id AND f.key = ? AND {cond})"
            )
        }
        Cond::Name { op, value } => text_cond("o.name_sort", *op, value, p),
        Cond::Describes { op, value } => {
            if *op != Op::Eq {
                return Err(Error::Invalid("describes supports only Op::Eq".into()));
            }
            p.push(Value::Text(value.clone()));
            "o.describes = ?".into()
        }
        Cond::Time { field, op, value } => {
            let col = match field {
                TimeField::Start => "o.start_time",
                TimeField::End => "o.end_time",
                TimeField::Published => "o.published",
                TimeField::Updated => "o.updated",
            };
            let Some(c) = cmp_sql(*op) else {
                return Err(Error::Invalid(
                    "Prefix/Contains do not apply to times".into(),
                ));
            };
            p.push(Value::Integer(value.timestamp_micros()));
            format!("{col} {c} ?")
        }
    })
}

fn query_where(q: &Query) -> Result<(String, Vec<Value>)> {
    let mut clauses: Vec<String> = Vec::new();
    let mut p: Vec<Value> = Vec::new();
    if let Some(t) = &q.object_type {
        clauses.push("o.type = ?".into());
        p.push(Value::Text(t.clone()));
    }
    if let Some(t) = &q.context {
        clauses.push("o.context = ?".into());
        p.push(Value::Text(t.clone()));
    }
    if let Some(t) = &q.attributed_to {
        clauses.push("o.attributed_to = ?".into());
        p.push(Value::Text(t.clone()));
    }
    for (k, v) in &q.index {
        clauses.push(
            "EXISTS (SELECT 1 FROM index_fields f WHERE f.object_id = o.id AND f.key = ? AND f.value = ?)".into(),
        );
        p.push(Value::Text(k.clone()));
        p.push(Value::Text(v.clone()));
    }
    for c in &q.conds {
        clauses.push(cond_sql(c, &mut p)?);
    }
    if !q.any.is_empty() {
        let parts = q
            .any
            .iter()
            .map(|c| cond_sql(c, &mut p))
            .collect::<Result<Vec<_>>>()?;
        clauses.push(format!("({})", parts.join(" OR ")));
    }
    let w = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    Ok((w, p))
}

/// Bring the database to [`SCHEMA_VERSION`]: create it, or migrate a 0.1 database.
async fn migrate(c: &Connection) -> Result<()> {
    let tables: Vec<String> = collect(
        c,
        "SELECT name FROM sqlite_master WHERE type = 'table'",
        vec![],
    )
    .await?
    .iter()
    .map(|r| text(&r[0]))
    .collect::<Result<_>>()?;
    let has = |n: &str| tables.iter().any(|t| t == n);
    let current = if has("schema_version") {
        let rows = collect(c, "SELECT MAX(version) FROM schema_version", vec![]).await?;
        rows.first()
            .and_then(|r| opt_int(&r[0]).ok().flatten())
            .unwrap_or(0)
    } else if has("objects") {
        1
    } else {
        0
    };
    if current > SCHEMA_VERSION {
        return Err(Error::Backend(format!(
            "database schema version {current} is newer than this fs-store ({SCHEMA_VERSION})"
        )));
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    in_tx(c, async {
        if current == 0 {
            c.execute_batch(SCHEMA).await?;
        } else {
            // 1 -> 2: first-class envelope fields, normalised sort/search keys.
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
                 ALTER TABLE objects ADD COLUMN describes TEXT;
                 ALTER TABLE objects ADD COLUMN start_time INTEGER;
                 ALTER TABLE objects ADD COLUMN end_time INTEGER;
                 ALTER TABLE objects ADD COLUMN name_sort TEXT NOT NULL DEFAULT '';
                 ALTER TABLE index_fields ADD COLUMN value_norm TEXT NOT NULL DEFAULT '';",
            )
            .await?;
            for r in collect(c, "SELECT id, name FROM objects", vec![]).await? {
                exec(
                    c,
                    "UPDATE objects SET name_sort = ? WHERE id = ?",
                    vec![Value::Text(sort_key(&text(&r[1])?)), r[0].clone()],
                )
                .await?;
            }
            for r in collect(c, "SELECT rowid, value FROM index_fields", vec![]).await? {
                exec(
                    c,
                    "UPDATE index_fields SET value_norm = ? WHERE rowid = ?",
                    vec![Value::Text(sort_key(&text(&r[1])?)), r[0].clone()],
                )
                .await?;
            }
        }
        c.execute_batch(INDEXES).await?;
        exec(c, "DELETE FROM schema_version", vec![]).await?;
        exec(
            c,
            "INSERT INTO schema_version (version) VALUES (?)",
            vec![Value::Integer(SCHEMA_VERSION)],
        )
        .await?;
        Ok(())
    })
    .await
}

impl TursoStore {
    /// Open (creating if needed) a store in the file at `path`.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, OpenOptions::default()).await
    }

    /// Open with options (encryption, sensitive-data policy).
    pub async fn open_with(path: impl AsRef<Path>, opts: OpenOptions) -> Result<Self> {
        let p = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::Invalid("path is not UTF-8".into()))?;
        Self::open_raw(p, opts).await
    }

    /// Volatile in-memory store (tests, caches).
    pub async fn open_in_memory() -> Result<Self> {
        let o = OpenOptions {
            allow_sensitive_unencrypted: true,
            ..OpenOptions::default()
        };
        Self::open_raw(":memory:", o).await
    }

    async fn open_raw(path: &str, opts: OpenOptions) -> Result<Self> {
        let mut b = turso::Builder::new_local(path);
        let mut encrypted = false;
        if let Some(kp) = &opts.key_provider {
            let key = kp.key_for(&opts.store_name)?;
            key.validate()?;
            b = b
                .experimental_encryption(true)
                .with_encryption(turso::EncryptionOpts {
                    cipher: key.cipher.turso_name().into(),
                    hexkey: key.hex(),
                });
            encrypted = true;
        }
        let db = b.build().await?;
        let conn = db.connect()?;
        migrate(&conn).await?;
        let last = collect(&conn, "SELECT MAX(id) FROM activities", vec![]).await?;
        let last_activity = match last.first().and_then(|r| r.first()) {
            Some(Value::Text(s)) => Some(s.parse()?),
            _ => None,
        };
        let (tx, _) = broadcast::channel(opts.subscribe_capacity.max(1));
        Ok(Self {
            _db: db,
            inner: Mutex::new(Inner {
                conn,
                last_activity,
            }),
            tx,
            encrypted,
            allow_sensitive_unencrypted: opts.allow_sensitive_unencrypted,
            fail_before_activity: AtomicBool::new(false),
            fail_before_extra: AtomicBool::new(false),
        })
    }

    /// True if the file is encrypted at rest.
    pub fn is_encrypted(&self) -> bool {
        self.encrypted
    }

    /// Test hook: make the next state-changing operation fail after the object
    /// write but before the activity write, to prove the transaction rolls back.
    #[doc(hidden)]
    pub fn inject_fault_before_activity(&self, on: bool) {
        self.fail_before_activity.store(on, Ordering::SeqCst);
    }

    /// Test hook: make the next `put_and_log` fail after the `Create`/`Update` entry is written
    /// but before the extra entries, to prove the whole call rolls back.
    #[doc(hidden)]
    pub fn inject_fault_before_extra(&self, on: bool) {
        self.fail_before_extra.store(on, Ordering::SeqCst);
    }

    /// Schema version of the open database.
    pub fn schema_version(&self) -> i64 {
        SCHEMA_VERSION
    }

    fn check_fault(&self) -> Result<()> {
        if self.fail_before_activity.swap(false, Ordering::SeqCst) {
            return Err(Error::Backend("injected fault".into()));
        }
        Ok(())
    }

    fn publish(&self, acts: &[Activity]) {
        for a in acts {
            let _ = self.tx.send(a.clone()); // no receivers is fine
        }
    }

    /// Write `before -> after` for `object` plus its activity. Caller holds the lock and a transaction.
    #[allow(clippy::too_many_arguments)]
    async fn apply(
        &self,
        inner: &mut Inner,
        kind: ActivityKind,
        actor: &str,
        object: ObjectId,
        target: Option<ActivityId>,
        before: Option<StoredObject>,
        after: Option<StoredObject>,
    ) -> Result<Activity> {
        match &after {
            Some(o) => write_object(&inner.conn, o).await?,
            None => remove_object(&inner.conn, object).await?,
        }
        self.check_fault()?;
        let id = ActivityId::next_after(inner.last_activity);
        let act = Activity {
            id,
            kind,
            object,
            actor: actor.to_string(),
            at: now(),
            target,
            payload: serde_json::to_value(ChangeSet::new(before, after))?,
            synced: false,
        };
        insert_activity(&inner.conn, &act).await?;
        inner.last_activity = Some(id);
        Ok(act)
    }

    async fn revert(&self, inner: &mut Inner, x_id: ActivityId, actor: &str) -> Result<Activity> {
        let x = load_activity(&inner.conn, x_id)
            .await?
            .ok_or_else(|| Error::Conflict(format!("activity {x_id} no longer in the log")))?;
        let cs = x.change_set()?;
        let current = load_object(&inner.conn, x.object).await?;
        let consistent = match (&current, &cs.after) {
            (None, None) => true,
            (Some(c), Some(a)) => c.matches_snapshot(a),
            _ => false,
        };
        if !consistent {
            return Err(Error::Conflict(format!(
                "object {} changed since activity {x_id}",
                x.object
            )));
        }
        let mut restored = cs.before;
        if let (Some(r), Some(c)) = (restored.as_mut(), current.as_ref()) {
            if r.original.is_none() {
                r.original = c.original.clone();
            }
        }
        self.apply(
            inner,
            ActivityKind::Undo,
            actor,
            x.object,
            Some(x_id),
            current,
            restored,
        )
        .await
    }

    async fn step_back(&self, actor: &str, n: usize, redo: bool) -> Result<Vec<Activity>> {
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let done = in_tx(&conn, async {
            let mut done = Vec::new();
            for _ in 0..n {
                let stacks = history::compute(&load_steps(&inner.conn, actor).await?);
                let target = if redo {
                    stacks.redo.last().map(|(_, u)| *u)
                } else {
                    stacks.undo.last().copied()
                };
                let Some(t) = target else { break };
                done.push(self.revert(inner, t, actor).await?);
            }
            Ok(done)
        })
        .await?;
        self.publish(&done);
        Ok(done)
    }

    async fn compact_with(
        &self,
        r: Retention,
        now: Timestamp,
        everything: bool,
    ) -> Result<CompactionReport> {
        let inner = self.inner.lock().await;
        let c = &inner.conn;
        in_tx(c, async {
            // Highest id that is outside retention; everything up to it is eligible (a prefix).
            let mut cut: Option<String> = None;
            let mut bump = |cand: Option<String>| {
                if let Some(x) = cand {
                    if cut.as_ref().is_none_or(|c| x > *c) {
                        cut = Some(x);
                    }
                }
            };
            if everything {
                let rows = collect(c, "SELECT MAX(id) FROM activities", vec![]).await?;
                bump(rows.first().and_then(|r| opt_text(&r[0]).ok().flatten()));
            }
            if let Some(age) = r.max_age {
                let age =
                    ChronoDuration::from_std(age).map_err(|e| Error::Invalid(e.to_string()))?;
                let cutoff = (now - age).timestamp_micros();
                let rows = collect(
                    c,
                    "SELECT MAX(id) FROM activities WHERE at < ?",
                    vec![Value::Integer(cutoff)],
                )
                .await?;
                bump(rows.first().and_then(|r| opt_text(&r[0]).ok().flatten()));
            }
            if let Some(steps) = r.max_steps {
                let rows = collect(
                    c,
                    "SELECT id FROM activities ORDER BY id DESC LIMIT 1 OFFSET ?",
                    vec![Value::Integer(steps as i64)],
                )
                .await?;
                bump(rows.first().and_then(|r| opt_text(&r[0]).ok().flatten()));
            }
            let mut removed = 0;
            let mut kept_unsynced = 0;
            if let Some(cut) = cut {
                removed = exec(
                    c,
                    "DELETE FROM activities WHERE synced = 1 AND id <= ?",
                    vec![Value::Text(cut.clone())],
                )
                .await? as usize;
                let rows = collect(
                    c,
                    "SELECT COUNT(*) FROM activities WHERE synced = 0 AND id <= ?",
                    vec![Value::Text(cut)],
                )
                .await?;
                kept_unsynced = int(&rows[0][0])? as usize;
            }
            let rows = collect(c, "SELECT COUNT(*) FROM activities", vec![]).await?;
            Ok(CompactionReport {
                removed,
                kept_unsynced,
                remaining: int(&rows[0][0])? as usize,
            })
        })
        .await
    }
}

fn validate(o: &StoredObject) -> Result<()> {
    if o.envelope.name.trim().is_empty() {
        return Err(Error::Invalid(
            "envelope.name must not be empty (ADR 0032)".into(),
        ));
    }
    if o.envelope.object_type.trim().is_empty() {
        return Err(Error::Invalid("envelope.type must not be empty".into()));
    }
    if o.content_type.trim().is_empty() {
        return Err(Error::Invalid("content_type must not be empty".into()));
    }
    for (k, v) in &o.index {
        if k.is_empty() || v.iter().any(|x| x.is_empty()) {
            return Err(Error::Invalid(
                "index keys and values must not be empty".into(),
            ));
        }
    }
    Ok(())
}

#[async_trait]
impl Store for TursoStore {
    async fn put(&self, object: StoredObject, actor: &str) -> Result<Activity> {
        let mut acts = self.put_and_log(object, actor, vec![]).await?;
        Ok(acts.remove(0))
    }

    async fn put_and_log(
        &self,
        object: StoredObject,
        actor: &str,
        extra: Vec<LogEntry>,
    ) -> Result<Vec<Activity>> {
        validate(&object)?;
        if let Some(e) = extra.iter().find(|e| e.kind.changes_state()) {
            return Err(Error::Invalid(format!(
                "{} cannot be an extra entry (state-changing kinds come from put/delete/undo/redo)",
                e.kind.as_str()
            )));
        }
        if !object.envelope.sensitive.is_empty()
            && !self.encrypted
            && !self.allow_sensitive_unencrypted
        {
            return Err(Error::EncryptionRequired(object.envelope.id));
        }
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let acts = in_tx(&conn, async {
            let id = object.envelope.id;
            let before = load_object(&inner.conn, id).await?;
            let mut after = object;
            let t = now();
            after.envelope.published = after.envelope.published.trunc_subsecs(6);
            after.envelope.updated = t;
            after.envelope.start_time = after.envelope.start_time.map(|t| t.trunc_subsecs(6));
            after.envelope.end_time = after.envelope.end_time.map(|t| t.trunc_subsecs(6));
            let kind = match &before {
                None => ActivityKind::Create,
                Some(b) => {
                    after.envelope.published = b.envelope.published;
                    after.envelope.updated = t.max(b.envelope.updated);
                    match (&b.original, &after.original) {
                        (Some(_), None) => after.original = b.original.clone(),
                        (Some(x), Some(y)) if x != y => return Err(Error::OriginalImmutable(id)),
                        (None, Some(_)) => return Err(Error::OriginalImmutable(id)),
                        _ => {}
                    }
                    ActivityKind::Update
                }
            };
            let first = self
                .apply(inner, kind, actor, id, None, before, Some(after))
                .await?;
            let mut acts = vec![first];
            if self.fail_before_extra.swap(false, Ordering::SeqCst) {
                return Err(Error::Backend("injected fault".into()));
            }
            for e in extra {
                let aid = ActivityId::next_after(inner.last_activity);
                let act = Activity {
                    id: aid,
                    kind: e.kind,
                    object: id,
                    actor: actor.into(),
                    at: now(),
                    target: None,
                    payload: e.payload,
                    synced: false,
                };
                insert_activity(&inner.conn, &act).await?;
                inner.last_activity = Some(aid);
                acts.push(act);
            }
            Ok(acts)
        })
        .await;
        match acts {
            Ok(acts) => {
                self.publish(&acts);
                Ok(acts)
            }
            Err(e) => {
                // last_activity may have advanced inside the rolled-back transaction: re-read it.
                if let Ok(rows) =
                    collect(&inner.conn, "SELECT MAX(id) FROM activities", vec![]).await
                {
                    inner.last_activity = match rows.first().and_then(|r| r.first()) {
                        Some(Value::Text(s)) => s.parse().ok(),
                        _ => None,
                    };
                }
                Err(e)
            }
        }
    }

    async fn get(&self, id: ObjectId) -> Result<Option<StoredObject>> {
        let inner = self.inner.lock().await;
        load_object(&inner.conn, id).await
    }

    async fn delete(&self, id: ObjectId, actor: &str) -> Result<Activity> {
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let act = in_tx(&conn, async {
            let before = load_object(&inner.conn, id)
                .await?
                .ok_or(Error::NotFound(id))?;
            self.apply(
                inner,
                ActivityKind::Delete,
                actor,
                id,
                None,
                Some(before),
                None,
            )
            .await
        })
        .await?;
        self.publish(std::slice::from_ref(&act));
        Ok(act)
    }

    async fn query(&self, q: &Query) -> Result<Vec<StoredObject>> {
        let inner = self.inner.lock().await;
        let (w, mut p) = query_where(q)?;
        let order = match q.order {
            Order::UpdatedDesc => "o.updated DESC, o.id DESC",
            Order::UpdatedAsc => "o.updated ASC, o.id ASC",
            Order::PublishedAsc => "o.published ASC, o.id ASC",
            Order::PublishedDesc => "o.published DESC, o.id DESC",
            Order::NameAsc => "o.name_sort ASC, o.name ASC, o.id ASC",
            Order::NameDesc => "o.name_sort DESC, o.name DESC, o.id DESC",
            Order::StartAsc => "COALESCE(o.start_time, 9223372036854775807) ASC, o.id ASC",
            Order::StartDesc => "COALESCE(o.start_time, -9223372036854775807) DESC, o.id DESC",
        };
        let order = match &q.order_by_index {
            Some(k) => {
                let sub = "(SELECT MIN(x.value_norm) FROM index_fields x WHERE x.object_id = o.id AND x.key = ?)";
                p.push(Value::Text(k.clone()));
                p.push(Value::Text(k.clone()));
                format!("{sub} IS NULL, {sub}, o.name_sort ASC, o.id ASC")
            }
            None => order.to_string(),
        };
        let cols = OBJECT_COLS
            .split(", ")
            .map(|c| format!("o.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        p.push(Value::Integer(
            q.limit.map(|l| l as i64).unwrap_or(i64::MAX),
        ));
        p.push(Value::Integer(q.offset as i64));
        let rows = collect(
            &inner.conn,
            &format!("SELECT {cols} FROM objects o{w} ORDER BY {order} LIMIT ? OFFSET ?"),
            p,
        )
        .await?;
        let mut objs = rows
            .iter()
            .map(|r| decode_object(r))
            .collect::<Result<Vec<_>>>()?;
        attach_index(&inner.conn, &mut objs).await?;
        Ok(objs)
    }

    async fn count(&self, q: &Query) -> Result<usize> {
        let inner = self.inner.lock().await;
        let (w, p) = query_where(q)?;
        let rows = collect(
            &inner.conn,
            &format!("SELECT COUNT(*) FROM objects o{w}"),
            p,
        )
        .await?;
        Ok(int(&rows[0][0])? as usize)
    }

    async fn log(
        &self,
        kind: ActivityKind,
        object: ObjectId,
        actor: &str,
        payload: serde_json::Value,
    ) -> Result<Activity> {
        if kind.changes_state() {
            return Err(Error::Invalid(format!(
                "{} is written by put/delete/undo/redo only",
                kind.as_str()
            )));
        }
        let mut inner = self.inner.lock().await;
        let inner = &mut *inner;
        let conn = inner.conn.clone();
        let act = in_tx(&conn, async {
            let id = ActivityId::next_after(inner.last_activity);
            let act = Activity {
                id,
                kind,
                object,
                actor: actor.into(),
                at: now(),
                target: None,
                payload,
                synced: false,
            };
            insert_activity(&inner.conn, &act).await?;
            inner.last_activity = Some(id);
            Ok(act)
        })
        .await?;
        self.publish(std::slice::from_ref(&act));
        Ok(act)
    }

    fn subscribe(&self) -> broadcast::Receiver<Activity> {
        self.tx.subscribe()
    }

    async fn activities(&self, f: &ActivityFilter) -> Result<Vec<Activity>> {
        let inner = self.inner.lock().await;
        let mut clauses: Vec<&str> = Vec::new();
        let mut p: Vec<Value> = Vec::new();
        if let Some(a) = f.after {
            clauses.push("id > ?");
            p.push(Value::Text(a.to_string()));
        }
        if let Some(o) = f.object {
            clauses.push("object_id = ?");
            p.push(Value::Text(o.to_string()));
        }
        if let Some(a) = &f.actor {
            clauses.push("actor = ?");
            p.push(Value::Text(a.clone()));
        }
        if f.unsynced_only {
            clauses.push("synced = 0");
        }
        let w = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        p.push(Value::Integer(
            f.limit.map(|l| l as i64).unwrap_or(i64::MAX),
        ));
        let rows = collect(
            &inner.conn,
            &format!("SELECT {ACTIVITY_COLS} FROM activities{w} ORDER BY id LIMIT ?"),
            p,
        )
        .await?;
        rows.iter().map(|r| decode_activity(r)).collect()
    }

    async fn undo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        self.step_back(actor, n, false).await
    }

    async fn redo(&self, actor: &str, n: usize) -> Result<Vec<Activity>> {
        self.step_back(actor, n, true).await
    }

    async fn undo_state(&self, actor: &str) -> Result<UndoState> {
        let inner = self.inner.lock().await;
        let s = history::compute(&load_steps(&inner.conn, actor).await?);
        Ok(UndoState {
            undoable: s.undo.len(),
            redoable: s.redo.len(),
        })
    }

    async fn retention(&self) -> Result<Retention> {
        let inner = self.inner.lock().await;
        let rows = collect(
            &inner.conn,
            "SELECT value FROM meta WHERE key = 'retention'",
            vec![],
        )
        .await?;
        match rows.first() {
            Some(r) => Ok(serde_json::from_str(&text(&r[0])?)?),
            None => Ok(Retention::UNLIMITED),
        }
    }

    async fn set_retention(&self, r: Retention) -> Result<()> {
        let inner = self.inner.lock().await;
        let c = &inner.conn;
        in_tx(c, async {
            exec(c, "DELETE FROM meta WHERE key = 'retention'", vec![]).await?;
            exec(
                c,
                "INSERT INTO meta (key, value) VALUES ('retention', ?)",
                vec![Value::Text(serde_json::to_string(&r)?)],
            )
            .await?;
            Ok(())
        })
        .await
    }

    async fn compact_at(&self, now: Timestamp) -> Result<CompactionReport> {
        let r = self.retention().await?;
        self.compact_with(r, now, false).await
    }

    async fn clear_history(&self) -> Result<CompactionReport> {
        self.compact_with(Retention::UNLIMITED, now(), true).await
    }

    async fn mark_synced(&self, up_to: ActivityId) -> Result<usize> {
        let inner = self.inner.lock().await;
        let n = exec(
            &inner.conn,
            "UPDATE activities SET synced = 1 WHERE synced = 0 AND id <= ?",
            vec![Value::Text(up_to.to_string())],
        )
        .await?;
        Ok(n as usize)
    }

    async fn log_stats(&self) -> Result<LogStats> {
        let inner = self.inner.lock().await;
        let rows = collect(
            &inner.conn,
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN synced = 0 THEN 1 ELSE 0 END), 0), COALESCE(SUM(LENGTH(payload)), 0) FROM activities",
            vec![],
        )
        .await?;
        let r = &rows[0];
        Ok(LogStats {
            entries: int(&r[0])? as usize,
            unsynced: int(&r[1])? as usize,
            payload_bytes: int(&r[2])? as usize,
        })
    }
}
