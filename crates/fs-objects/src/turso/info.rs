//! [`StoreInfo`] for the Turso backend.
//!
//! Pattern: Adapter — answers from what the open database and its options already say.

use super::TursoStore;
use crate::{StoreId, StoreInfo};

impl StoreInfo for TursoStore {
    fn identity(&self) -> StoreId {
        self.identity
    }

    fn is_encrypted(&self) -> bool {
        TursoStore::is_encrypted(self)
    }
}
