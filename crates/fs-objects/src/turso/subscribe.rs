//! [`ChangeFeed`] for the Turso backend.
//!
//! Pattern: Observer — a `tokio::sync::broadcast` channel; activities are sent after commit.

use tokio::sync::broadcast;

use super::TursoStore;
use crate::{Activity, ChangeFeed};

impl ChangeFeed for TursoStore {
    fn subscribe(&self) -> broadcast::Receiver<Activity> {
        self.tx.subscribe()
    }
}
