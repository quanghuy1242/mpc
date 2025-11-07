//! # Sync Job Repository
//!
//! Provides database persistence for sync jobs.
//!
//! ## Overview
//!
//! This repository handles CRUD operations for sync jobs, including:
//! - Creating new sync jobs
//! - Updating job progress and status
//! - Querying jobs by provider or status
//! - Job history retrieval

use crate::{
    Result, SyncError, SyncJob, SyncJobId, SyncJobStats, SyncProgress, SyncStatus, SyncType,
};
use async_trait::async_trait;
use core_auth::ProviderKind;
use sqlx::{FromRow, SqlitePool};

// ============================================================================
// Repository Trait
// ============================================================================

/// Repository trait for sync job persistence
#[async_trait]
pub trait SyncJobRepository: Send + Sync {
    /// Create or insert a new sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn insert(&self, job: &SyncJob) -> Result<()>;

    /// Update an existing sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the job doesn't exist or the database operation fails
    async fn update(&self, job: &SyncJob) -> Result<()>;

    /// Find a sync job by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_id(&self, id: &SyncJobId) -> Result<Option<SyncJob>>;

    /// Get all sync jobs for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_provider(&self, provider: ProviderKind) -> Result<Vec<SyncJob>>;

    /// Get sync jobs by status
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_status(&self, status: SyncStatus) -> Result<Vec<SyncJob>>;

    /// Get the most recent sync job for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_latest_by_provider(&self, provider: ProviderKind) -> Result<Option<SyncJob>>;

    /// Get sync job history for a provider (most recent first)
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider to get history for
    /// * `limit` - Maximum number of jobs to return
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn get_history(&self, provider: ProviderKind, limit: u32) -> Result<Vec<SyncJob>>;

    /// Delete a sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn delete(&self, id: &SyncJobId) -> Result<()>;

    /// Check if there's an active (pending or running) sync for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn has_active_sync(&self, provider: ProviderKind) -> Result<bool>;
}

// ============================================================================
// SQLite Implementation
// ============================================================================

/// SQLite implementation of SyncJobRepository
pub struct SqliteSyncJobRepository {
    pool: SqlitePool,
}

impl SqliteSyncJobRepository {
    /// Create a new SQLite sync job repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Database row representation of a sync job
#[derive(Debug, FromRow)]
struct SyncJobRow {
    id: String,
    provider_id: String,
    status: String,
    sync_type: String,
    items_discovered: i64,
    items_processed: i64,
    items_failed: i64,
    items_added: i64,
    items_updated: i64,
    items_deleted: i64,
    error_message: Option<String>,
    error_details: Option<String>,
    cursor: Option<String>,
    started_at: Option<i64>,
    completed_at: Option<i64>,
    created_at: i64,
}

impl TryFrom<SyncJobRow> for SyncJob {
    type Error = SyncError;

    fn try_from(row: SyncJobRow) -> Result<Self> {
        let provider_id = ProviderKind::parse(&row.provider_id).ok_or_else(|| {
            SyncError::Database(format!("Invalid provider_id: {}", row.provider_id))
        })?;

        let status: SyncStatus = row.status.parse()?;
        let sync_type: SyncType = row.sync_type.parse()?;

        // Calculate progress percentage
        let percent = if row.items_discovered > 0 {
            ((row.items_processed as f64 / row.items_discovered as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };

        let progress = SyncProgress {
            items_discovered: row.items_discovered as u64,
            items_processed: row.items_processed as u64,
            items_failed: row.items_failed as u64,
            percent,
            phase: status.as_str().to_string(),
        };

        let stats = if status == SyncStatus::Completed {
            Some(SyncJobStats {
                items_added: row.items_added as u64,
                items_updated: row.items_updated as u64,
                items_deleted: row.items_deleted as u64,
                items_failed: row.items_failed as u64,
            })
        } else {
            None
        };

        Ok(SyncJob {
            id: SyncJobId::from_string(&row.id)?,
            provider_id,
            status,
            sync_type,
            progress,
            stats,
            cursor: row.cursor,
            error_message: row.error_message,
            error_details: row.error_details,
            created_at: row.created_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
        })
    }
}

#[async_trait]
impl SyncJobRepository for SqliteSyncJobRepository {
    async fn insert(&self, job: &SyncJob) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sync_jobs (
                id, provider_id, status, sync_type,
                items_discovered, items_processed, items_failed,
                items_added, items_updated, items_deleted,
                error_message, error_details, cursor,
                started_at, completed_at, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(job.id.as_str())
        .bind(job.provider_id.as_str())
        .bind(job.status.as_str())
        .bind(job.sync_type.as_str())
        .bind(job.progress.items_discovered as i64)
        .bind(job.progress.items_processed as i64)
        // Use stats.items_failed if available (for completed jobs), otherwise use progress.items_failed
        .bind(
            job.stats
                .as_ref()
                .map_or(job.progress.items_failed as i64, |s| s.items_failed as i64),
        )
        .bind(job.stats.as_ref().map_or(0, |s| s.items_added as i64))
        .bind(job.stats.as_ref().map_or(0, |s| s.items_updated as i64))
        .bind(job.stats.as_ref().map_or(0, |s| s.items_deleted as i64))
        .bind(&job.error_message)
        .bind(&job.error_details)
        .bind(&job.cursor)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(job.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, job: &SyncJob) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE sync_jobs SET
                status = ?,
                items_discovered = ?,
                items_processed = ?,
                items_failed = ?,
                items_added = ?,
                items_updated = ?,
                items_deleted = ?,
                error_message = ?,
                error_details = ?,
                cursor = ?,
                started_at = ?,
                completed_at = ?
            WHERE id = ?
            "#,
        )
        .bind(job.status.as_str())
        .bind(job.progress.items_discovered as i64)
        .bind(job.progress.items_processed as i64)
        // Use stats.items_failed if available (for completed jobs), otherwise use progress.items_failed
        .bind(
            job.stats
                .as_ref()
                .map_or(job.progress.items_failed as i64, |s| s.items_failed as i64),
        )
        .bind(job.stats.as_ref().map_or(0, |s| s.items_added as i64))
        .bind(job.stats.as_ref().map_or(0, |s| s.items_updated as i64))
        .bind(job.stats.as_ref().map_or(0, |s| s.items_deleted as i64))
        .bind(&job.error_message)
        .bind(&job.error_details)
        .bind(&job.cursor)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(job.id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(SyncError::JobNotFound {
                job_id: job.id.to_string(),
            });
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &SyncJobId) -> Result<Option<SyncJob>> {
        let row = sqlx::query_as::<_, SyncJobRow>(
            r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE id = ?
            "#,
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        row.map(SyncJob::try_from).transpose()
    }

    async fn find_by_provider(&self, provider: ProviderKind) -> Result<Vec<SyncJob>> {
        let rows = sqlx::query_as::<_, SyncJobRow>(
            r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE provider_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(provider.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.into_iter()
            .map(SyncJob::try_from)
            .collect::<Result<Vec<_>>>()
    }

    async fn find_by_status(&self, status: SyncStatus) -> Result<Vec<SyncJob>> {
        let rows = sqlx::query_as::<_, SyncJobRow>(
            r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE status = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(status.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.into_iter()
            .map(SyncJob::try_from)
            .collect::<Result<Vec<_>>>()
    }

    async fn find_latest_by_provider(&self, provider: ProviderKind) -> Result<Option<SyncJob>> {
        let row = sqlx::query_as::<_, SyncJobRow>(
            r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE provider_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(provider.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        row.map(SyncJob::try_from).transpose()
    }

    async fn get_history(&self, provider: ProviderKind, limit: u32) -> Result<Vec<SyncJob>> {
        let rows = sqlx::query_as::<_, SyncJobRow>(
            r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE provider_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(provider.as_str())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.into_iter()
            .map(SyncJob::try_from)
            .collect::<Result<Vec<_>>>()
    }

    async fn delete(&self, id: &SyncJobId) -> Result<()> {
        let result = sqlx::query("DELETE FROM sync_jobs WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(SyncError::JobNotFound {
                job_id: id.to_string(),
            });
        }

        Ok(())
    }

    async fn has_active_sync(&self, provider: ProviderKind) -> Result<bool> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM sync_jobs
            WHERE provider_id = ? AND status IN ('pending', 'running')
            "#,
        )
        .bind(provider.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(count > 0)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core_async::time::{sleep, Duration};
    use sqlx::SqlitePool;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Create sync_jobs table
        sqlx::query(
            r#"
            CREATE TABLE sync_jobs (
                id TEXT PRIMARY KEY NOT NULL,
                provider_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                sync_type TEXT NOT NULL,
                items_discovered INTEGER DEFAULT 0,
                items_processed INTEGER DEFAULT 0,
                items_failed INTEGER DEFAULT 0,
                items_added INTEGER DEFAULT 0,
                items_updated INTEGER DEFAULT 0,
                items_deleted INTEGER DEFAULT 0,
                error_message TEXT,
                error_details TEXT,
                cursor TEXT,
                started_at INTEGER,
                completed_at INTEGER,
                created_at INTEGER NOT NULL,
                CONSTRAINT sync_jobs_status_check CHECK (
                    status IN ('pending', 'running', 'completed', 'failed', 'cancelled')
                ),
                CONSTRAINT sync_jobs_type_check CHECK (
                    sync_type IN ('full', 'incremental')
                )
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[core_async::test]
    async fn test_insert_and_find_by_id() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;

        repo.insert(&job).await.unwrap();

        let found = repo.find_by_id(&job_id).await.unwrap();
        assert!(found.is_some());

        let found_job = found.unwrap();
        assert_eq!(found_job.id, job_id);
        assert_eq!(found_job.provider_id, ProviderKind::GoogleDrive);
        assert_eq!(found_job.status, SyncStatus::Pending);
    }

    #[core_async::test]
    async fn test_update_job() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;
        repo.insert(&job).await.unwrap();

        // Start the job
        let mut job = job.start().unwrap();
        job.update_progress(50, 100, "Processing").unwrap();
        repo.update(&job).await.unwrap();

        // Verify update
        let found = repo.find_by_id(&job_id).await.unwrap().unwrap();
        assert_eq!(found.status, SyncStatus::Running);
        assert_eq!(found.progress.items_processed, 50);
        assert_eq!(found.progress.items_discovered, 100);
    }

    #[core_async::test]
    async fn test_find_by_provider() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        // Insert multiple jobs
        let job1 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Incremental);
        let job3 = SyncJob::new(ProviderKind::OneDrive, SyncType::Full);

        repo.insert(&job1).await.unwrap();
        repo.insert(&job2).await.unwrap();
        repo.insert(&job3).await.unwrap();

        let google_jobs = repo
            .find_by_provider(ProviderKind::GoogleDrive)
            .await
            .unwrap();
        assert_eq!(google_jobs.len(), 2);

        let onedrive_jobs = repo.find_by_provider(ProviderKind::OneDrive).await.unwrap();
        assert_eq!(onedrive_jobs.len(), 1);
    }

    #[core_async::test]
    async fn test_find_by_status() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        let job1 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = job2.start().unwrap();

        repo.insert(&job1).await.unwrap();
        repo.insert(&job2).await.unwrap();

        let pending = repo.find_by_status(SyncStatus::Pending).await.unwrap();
        assert_eq!(pending.len(), 1);

        let running = repo.find_by_status(SyncStatus::Running).await.unwrap();
        assert_eq!(running.len(), 1);
    }

    #[core_async::test]
    async fn test_has_active_sync() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        // No active sync initially
        assert!(!repo
            .has_active_sync(ProviderKind::GoogleDrive)
            .await
            .unwrap());

        // Add pending job
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        repo.insert(&job).await.unwrap();

        assert!(repo
            .has_active_sync(ProviderKind::GoogleDrive)
            .await
            .unwrap());

        // Complete the job
        let job = job.start().unwrap();
        let job = job.complete(SyncJobStats::new()).unwrap();
        repo.update(&job).await.unwrap();

        // No longer active
        assert!(!repo
            .has_active_sync(ProviderKind::GoogleDrive)
            .await
            .unwrap());
    }

    #[core_async::test]
    async fn test_get_history() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        // Create multiple jobs
        for _i in 0..5 {
            let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
            repo.insert(&job).await.unwrap();
            // Small delay to ensure different created_at times
            sleep(Duration::from_millis(10)).await;
        }

        let history = repo
            .get_history(ProviderKind::GoogleDrive, 3)
            .await
            .unwrap();
        assert_eq!(history.len(), 3);

        // Should be in descending order (most recent first)
        for i in 0..history.len() - 1 {
            assert!(history[i].created_at >= history[i + 1].created_at);
        }
    }

    #[core_async::test]
    async fn test_delete_job() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;

        repo.insert(&job).await.unwrap();
        assert!(repo.find_by_id(&job_id).await.unwrap().is_some());

        repo.delete(&job_id).await.unwrap();
        assert!(repo.find_by_id(&job_id).await.unwrap().is_none());
    }

    #[core_async::test]
    async fn test_complete_job_with_stats() {
        let pool = create_test_pool().await;
        let repo = SqliteSyncJobRepository::new(pool);

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;
        let job = job.start().unwrap();

        let stats = SyncJobStats {
            items_added: 100,
            items_updated: 20,
            items_deleted: 5,
            items_failed: 2,
        };

        let job = job.complete(stats).unwrap();
        repo.insert(&job).await.unwrap();

        let found = repo.find_by_id(&job_id).await.unwrap().unwrap();
        assert_eq!(found.status, SyncStatus::Completed);
        assert!(found.stats.is_some());

        let found_stats = found.stats.unwrap();
        assert_eq!(found_stats.items_added, 100);
        assert_eq!(found_stats.items_updated, 20);
        assert_eq!(found_stats.items_deleted, 5);
        assert_eq!(found_stats.items_failed, 2);
    }
}
