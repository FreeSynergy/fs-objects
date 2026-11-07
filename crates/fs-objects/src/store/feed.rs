//! [`ChangeFeed`]: subscribe to committed activities.
//!
//! Pattern: Observer — subscribers receive every committed activity on a broadcast channel.

use tokio::sync::broadcast;

use crate::Activity;

/// Change stream of a store.
pub trait ChangeFeed: Send + Sync {
    /// Change stream: every committed activity is sent after its commit.
    /// Slow receivers get `RecvError::Lagged` and should re-read the log.
    fn subscribe(&self) -> broadcast::Receiver<Activity>;
}
