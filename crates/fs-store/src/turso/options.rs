//! Options for opening a [`TursoStore`](super::TursoStore).
//!
//! Pattern: Strategy (configuration) — the optional `KeyProvider` selects the at-rest
//! encryption strategy without the store knowing where keys come from.

use std::sync::Arc;

use crate::KeyProvider;

/// Options for opening a [`TursoStore`](super::TursoStore).
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
