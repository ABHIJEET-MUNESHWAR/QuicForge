//! In-memory [`RunRepository`] adapter.

use async_trait::async_trait;
use parking_lot::Mutex;

use quicforge_core::error::PortError;
use quicforge_core::ports::RunRepository;
use quicforge_types::prelude::*;

/// Thread-safe in-memory store of run summaries.
///
/// Suitable for the CLI/lab harness and tests; a persistent adapter (sqlx) can
/// be slotted in behind the same port without touching the engine.
#[derive(Debug, Default)]
pub struct MemoryRunRepository {
    rows: Mutex<Vec<RunSummary>>,
}

impl MemoryRunRepository {
    /// Create an empty repository.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored summaries.
    pub fn len(&self) -> usize {
        self.rows.lock().len()
    }

    /// Whether the repository is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.lock().is_empty()
    }
}

#[async_trait]
impl RunRepository for MemoryRunRepository {
    async fn save(&self, summary: &RunSummary) -> Result<(), PortError> {
        self.rows.lock().push(summary.clone());
        Ok(())
    }

    async fn update(&self, summary: &RunSummary) -> Result<(), PortError> {
        let mut rows = self.rows.lock();
        if let Some(slot) = rows.iter_mut().find(|s| s.id == summary.id) {
            *slot = summary.clone();
        } else {
            rows.push(summary.clone());
        }
        Ok(())
    }

    async fn get(&self, id: RunId) -> Result<Option<RunSummary>, PortError> {
        Ok(self.rows.lock().iter().find(|s| s.id == id).cloned())
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<RunSummary>, PortError> {
        let mut rows = self.rows.lock().clone();
        rows.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn summary() -> RunSummary {
        RunSummary {
            id: RunId::generate(),
            params: BenchParams::new(
                "127.0.0.1:9000".parse().unwrap(),
                ConnectionCount::new(1).unwrap(),
                RequestCount::new(10).unwrap(),
                PayloadSize::default(),
            ),
            status: RunStatus::Running { completed: 0 },
            stats: None,
            throughput: None,
            started_at: Utc::now(),
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn save_get_update_replaces_in_place() {
        let repo = MemoryRunRepository::new();
        let mut s = summary();
        repo.save(&s).await.unwrap();
        assert_eq!(repo.len(), 1);

        s.status = RunStatus::Completed;
        repo.update(&s).await.unwrap();
        assert_eq!(repo.len(), 1, "update must not append a new row");

        let fetched = repo.get(s.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, RunStatus::Completed);
    }

    #[tokio::test]
    async fn list_recent_orders_by_start_desc() {
        let repo = MemoryRunRepository::new();
        let mut older = summary();
        older.started_at = Utc::now() - chrono::Duration::seconds(10);
        let newer = summary();
        repo.save(&older).await.unwrap();
        repo.save(&newer).await.unwrap();

        let recent = repo.list_recent(10).await.unwrap();
        assert_eq!(recent[0].id, newer.id);
        assert_eq!(recent.len(), 2);
    }
}
