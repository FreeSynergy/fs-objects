//! The [`Query`] value with its fluent builder methods, and [`Order`].
//!
//! Pattern: Builder — `Query::new().of_type(..).name_contains(..).limit(..)` assembles a
//! backend-neutral query; backends translate it (Turso: SQL).

use crate::Timestamp;
use crate::store::cond::{Cond, Op, TimeField};

/// Sort order of [`Query`] results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// Most recently updated first (default).
    #[default]
    UpdatedDesc,
    /// Least recently updated first.
    UpdatedAsc,
    /// Oldest published first.
    PublishedAsc,
    /// Newest published first.
    PublishedDesc,
    /// Case-insensitive, Unicode-normalised name order (see [`crate::sort_key`]); ties by id.
    NameAsc,
    /// Reverse of `NameAsc`.
    NameDesc,
    /// By envelope `start_time`; objects without one come last.
    StartAsc,
    /// By envelope `start_time`, latest first; objects without one come last.
    StartDesc,
}

/// Object query. All set filters are AND-ed; `any` is an OR-group that is AND-ed with the rest
/// (empty = no constraint). Full-text search (tokenised) is not provided; use `Cond` with
/// `Op::Contains` / `Op::Prefix`.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// Only this AS2 type.
    pub object_type: Option<String>,
    /// Only this context.
    pub context: Option<String>,
    /// Only objects attributed to this actor.
    pub attributed_to: Option<String>,
    /// `(key, value)` pairs that must all be present in the object's index fields.
    pub index: Vec<(String, String)>,
    /// Further conditions, all of which must hold (0.2).
    pub conds: Vec<Cond>,
    /// At least one of these must hold, if non-empty (0.2); e.g. a search over name, e-mail, phone.
    pub any: Vec<Cond>,
    /// Result order (ignored if `order_by_index` is set).
    pub order: Order,
    /// If set, order by this index field instead of `order`: smallest normalised value of the key
    /// ascending (objects without the key last), ties by name key, then id (0.2).
    pub order_by_index: Option<String>,
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Number of results to skip.
    pub offset: usize,
}

impl Query {
    /// An empty query (matches everything).
    pub fn new() -> Self {
        Self::default()
    }
    /// Only objects of AS2 type `t`.
    pub fn of_type(mut self, t: impl Into<String>) -> Self {
        self.object_type = Some(t.into());
        self
    }
    /// Only objects in context `c`.
    pub fn in_context(mut self, c: impl Into<String>) -> Self {
        self.context = Some(c.into());
        self
    }
    /// Only objects attributed to `a`.
    pub fn attributed_to(mut self, a: impl Into<String>) -> Self {
        self.attributed_to = Some(a.into());
        self
    }
    /// Require the exact index pair `(key, value)`.
    pub fn with_index(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.index.push((key.into(), value.into()));
        self
    }
    /// Add a condition that must hold.
    pub fn and(mut self, c: Cond) -> Self {
        self.conds.push(c);
        self
    }
    /// Add a condition to the OR-group.
    pub fn or(mut self, c: Cond) -> Self {
        self.any.push(c);
        self
    }
    /// Index field `key` has a value satisfying `op value`.
    pub fn index_op(self, key: impl Into<String>, op: Op, value: impl Into<String>) -> Self {
        self.and(Cond::Index {
            key: key.into(),
            op,
            value: value.into(),
        })
    }
    /// Index field `key` has a value in `[from, to)` (binary text order).
    pub fn index_between(
        self,
        key: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        let key = key.into();
        self.index_op(key.clone(), Op::Ge, from)
            .index_op(key, Op::Lt, to)
    }
    /// Envelope time condition.
    pub fn time(self, field: TimeField, op: Op, value: Timestamp) -> Self {
        self.and(Cond::Time { field, op, value })
    }
    /// Case-insensitive substring in the name.
    pub fn name_contains(self, v: impl Into<String>) -> Self {
        self.and(Cond::Name {
            op: Op::Contains,
            value: v.into(),
        })
    }
    /// Case-insensitive name prefix.
    pub fn name_prefix(self, v: impl Into<String>) -> Self {
        self.and(Cond::Name {
            op: Op::Prefix,
            value: v.into(),
        })
    }
    /// Order by an index field (see [`Query::order_by_index`] field).
    pub fn order_by_index(mut self, key: impl Into<String>) -> Self {
        self.order_by_index = Some(key.into());
        self
    }
    /// Set the result order.
    pub fn ordered(mut self, o: Order) -> Self {
        self.order = o;
        self
    }
    /// Return at most `n` results.
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
    /// Skip the first `n` results.
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }
}
