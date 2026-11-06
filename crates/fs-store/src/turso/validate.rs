//! Input validation for objects handed to the store.
//!
//! Pattern: none (pure rules) — separate so the rules are easy to test and extend.

use crate::{Error, Result, StoredObject};

/// Reject objects that violate store rules (ADR 0032: non-empty name, etc.).
pub(super) fn validate(o: &StoredObject) -> Result<()> {
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
        if k.is_empty() || v.iter().any(std::string::String::is_empty) {
            return Err(Error::Invalid(
                "index keys and values must not be empty".into(),
            ));
        }
    }
    Ok(())
}
