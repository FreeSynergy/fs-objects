//! Data model: ids, envelope, stored object, activity (ADR 0020, 0027, 0030, 0032).
//!
//! Pattern: Newtype — typed ids and values; this module only declares and re-exports.

mod activity;
mod envelope;
mod ids;
mod object;
mod store_id;

pub use activity::{Activity, ActivityKind, ChangeSet};
pub use envelope::{DpvCategory, Envelope, Timestamp};
pub use ids::{ActivityId, ObjectId};
pub use object::{Blob, IndexFields, StoredObject};
pub use store_id::StoreId;
