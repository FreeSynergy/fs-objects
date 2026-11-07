//! A store written before `purge_object` existed still compiles and answers "not supported".

use fs_store::traits::HistoryRetention;
use fs_store::*;

struct Fake;
#[async_trait::async_trait]
impl HistoryRetention for Fake {
    async fn retention(&self) -> Result<Retention> {
        Ok(Retention::UNLIMITED)
    }
    async fn set_retention(&self, _: Retention) -> Result<()> {
        Ok(())
    }
    async fn compact_at(&self, _: Timestamp) -> Result<CompactionReport> {
        Ok(CompactionReport::default())
    }
    async fn clear_history(&self) -> Result<CompactionReport> {
        Ok(CompactionReport::default())
    }
    async fn mark_synced(&self, _: ActivityId) -> Result<usize> {
        Ok(0)
    }
    async fn log_stats(&self) -> Result<LogStats> {
        Ok(LogStats::default())
    }
}

#[tokio::test]
async fn an_older_implementation_compiles_and_answers_unsupported() {
    let r = Fake.purge_object(ObjectId::new()).await;
    assert!(matches!(r, Err(Error::Backend(m)) if m.contains("not supported")));
}
