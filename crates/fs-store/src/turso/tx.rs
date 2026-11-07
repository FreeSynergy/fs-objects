//! Transaction wrapper.
//!
//! Pattern: Template method — `in_tx` fixes the skeleton (`BEGIN IMMEDIATE`, run the step,
//! `COMMIT` or `ROLLBACK`) and the caller supplies the varying step as a future. A `Drop` guard
//! cannot await the rollback, so the closure form is the safe RAII equivalent here.

use std::future::Future;

use turso::Connection;

use crate::Result;

/// Run `fut` inside `BEGIN IMMEDIATE ... COMMIT`, rolling back on error.
pub(super) async fn in_tx<T>(c: &Connection, fut: impl Future<Output = Result<T>>) -> Result<T> {
    begin_immediate(c).await?;
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

/// `BEGIN IMMEDIATE`, waiting for a competing writer (another handle on the same file, or a first open
/// that is migrating): retry on `Busy` for about five seconds, then give up with the backend error.
async fn begin_immediate(c: &Connection) -> Result<()> {
    for _ in 0..100 {
        match c.execute("BEGIN IMMEDIATE", ()).await {
            Err(turso::Error::Busy(_) | turso::Error::BusySnapshot(_)) => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            other => return other.map(|_| ()).map_err(Into::into),
        }
    }
    c.execute("BEGIN IMMEDIATE", ()).await?;
    Ok(())
}
