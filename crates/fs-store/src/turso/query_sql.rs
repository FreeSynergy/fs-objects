//! Translation of a [`Query`] into SQL (`WHERE`, `ORDER BY`).
//!
//! Pattern: Builder (internal) — assembles clause strings and bound parameters from the
//! declarative `Query`/`Cond` values; no SQL leaks out of the backend.

use turso::Value;

use crate::text::like_escape;
use crate::{Cond, Error, Op, Order, Query, Result, TimeField, sort_key};

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

/// `WHERE` clause (with leading space, or empty) and its parameters.
pub(super) fn query_where(q: &Query) -> Result<(String, Vec<Value>)> {
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

/// `ORDER BY` expression; may push parameters (for `order_by_index`).
pub(super) fn order_by(q: &Query, p: &mut Vec<Value>) -> String {
    if let Some(k) = &q.order_by_index {
        let sub =
            "(SELECT MIN(x.value_norm) FROM index_fields x WHERE x.object_id = o.id AND x.key = ?)";
        p.push(Value::Text(k.clone()));
        p.push(Value::Text(k.clone()));
        return format!("{sub} IS NULL, {sub}, o.name_sort ASC, o.id ASC");
    }
    match q.order {
        Order::UpdatedDesc => "o.updated DESC, o.id DESC",
        Order::UpdatedAsc => "o.updated ASC, o.id ASC",
        Order::PublishedAsc => "o.published ASC, o.id ASC",
        Order::PublishedDesc => "o.published DESC, o.id DESC",
        Order::NameAsc => "o.name_sort ASC, o.name ASC, o.id ASC",
        Order::NameDesc => "o.name_sort DESC, o.name DESC, o.id DESC",
        Order::StartAsc => "COALESCE(o.start_time, 9223372036854775807) ASC, o.id ASC",
        Order::StartDesc => "COALESCE(o.start_time, -9223372036854775807) DESC, o.id DESC",
    }
    .to_string()
}
