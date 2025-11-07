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
use bridge_traits::{
    database::{DatabaseAdapter, QueryRow, QueryValue},
    platform::PlatformSendSync,
};
use core_auth::ProviderKind;

// ============================================================================
// Repository Trait
// ============================================================================

/// Repository trait for sync job persistence
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait SyncJobRepository: PlatformSendSync {
    /// Create or insert a new sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn insert(&self, db: &dyn DatabaseAdapter, job: &SyncJob) -> Result<()>;

    /// Update an existing sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the job doesn't exist or the database operation fails
    async fn update(&self, db: &dyn DatabaseAdapter, job: &SyncJob) -> Result<()>;

    /// Find a sync job by ID
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_id(&self, db: &dyn DatabaseAdapter, id: &SyncJobId)
        -> Result<Option<SyncJob>>;

    /// Get all sync jobs for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_provider(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<Vec<SyncJob>>;

    /// Get sync jobs by status
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_by_status(
        &self,
        db: &dyn DatabaseAdapter,
        status: SyncStatus,
    ) -> Result<Vec<SyncJob>>;

    /// Get the most recent sync job for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn find_latest_by_provider(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<Option<SyncJob>>;

    /// Get sync job history for a provider (most recent first)
    ///
    /// # Arguments
    ///
    /// * `db` - Database adapter
    /// * `provider` - The provider to get history for
    /// * `limit` - Maximum number of jobs to return
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn get_history(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
        limit: u32,
    ) -> Result<Vec<SyncJob>>;

    /// Delete a sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn delete(&self, db: &dyn DatabaseAdapter, id: &SyncJobId) -> Result<()>;

    /// Check if there's an active (pending or running) sync for a provider
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails
    async fn has_active_sync(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<bool>;
}

// ============================================================================
// SQLite Implementation
// ============================================================================

/// SQLite implementation of SyncJobRepository
///
/// Uses DatabaseAdapter for platform-agnostic database operations
pub struct SqliteSyncJobRepository;

impl SqliteSyncJobRepository {
    /// Create a new SQLite sync job repository
    pub fn new() -> Self {
        Self
    }
}

impl Default for SqliteSyncJobRepository {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper Functions for Row Parsing
// ============================================================================

/// Parse a QueryRow into a SyncJob
fn row_to_sync_job(row: &QueryRow) -> Result<SyncJob> {
    let provider_id_str = get_string(row, "provider_id")?;
    let provider_id = ProviderKind::parse(&provider_id_str)
        .ok_or_else(|| SyncError::Database(format!("Invalid provider_id: {}", provider_id_str)))?;

    let status_str = get_string(row, "status")?;
    let status: SyncStatus = status_str.parse()?;

    let sync_type_str = get_string(row, "sync_type")?;
    let sync_type: SyncType = sync_type_str.parse()?;

    let items_discovered = get_i64(row, "items_discovered")? as u64;
    let items_processed = get_i64(row, "items_processed")? as u64;
    let items_failed = get_i64(row, "items_failed")? as u64;

    // Calculate progress percentage
    let percent = if items_discovered > 0 {
        ((items_processed as f64 / items_discovered as f64) * 100.0).min(100.0) as u8
    } else {
        0
    };

    let progress = SyncProgress {
        items_discovered,
        items_processed,
        items_failed,
        percent,
        phase: status.as_str().to_string(),
    };

    let stats = if status == SyncStatus::Completed {
        Some(SyncJobStats {
            items_added: get_i64(row, "items_added")? as u64,
            items_updated: get_i64(row, "items_updated")? as u64,
            items_deleted: get_i64(row, "items_deleted")? as u64,
            items_failed,
        })
    } else {
        None
    };

    Ok(SyncJob {
        id: SyncJobId::from_string(&get_string(row, "id")?)?,
        provider_id,
        status,
        sync_type,
        progress,
        stats,
        cursor: get_optional_string(row, "cursor")?,
        error_message: get_optional_string(row, "error_message")?,
        error_details: get_optional_string(row, "error_details")?,
        created_at: get_i64(row, "created_at")?,
        started_at: get_optional_i64(row, "started_at")?,
        completed_at: get_optional_i64(row, "completed_at")?,
    })
}

fn get_string(row: &QueryRow, key: &str) -> Result<String> {
    row.get(key)
        .and_then(|value| value.as_string())
        .ok_or_else(|| SyncError::Database(format!("Missing column: {}", key)))
}

fn get_optional_string(row: &QueryRow, key: &str) -> Result<Option<String>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(
            value
                .as_string()
                .ok_or_else(|| SyncError::Database(format!("Invalid type for column: {}", key)))?,
        ),
    })
}

fn get_i64(row: &QueryRow, key: &str) -> Result<i64> {
    row.get(key)
        .and_then(|value| value.as_i64())
        .ok_or_else(|| SyncError::Database(format!("Missing column: {}", key)))
}

fn get_optional_i64(row: &QueryRow, key: &str) -> Result<Option<i64>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(
            value
                .as_i64()
                .ok_or_else(|| SyncError::Database(format!("Invalid type for column: {}", key)))?,
        ),
    })
}

fn opt_text(value: &Option<String>) -> QueryValue {
    value
        .as_ref()
        .map(|v| QueryValue::Text(v.clone()))
        .unwrap_or(QueryValue::Null)
}

fn opt_i64(value: Option<i64>) -> QueryValue {
    value.map(QueryValue::Integer).unwrap_or(QueryValue::Null)
}

// ============================================================================
// Repository Implementation
// ============================================================================

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl SyncJobRepository for SqliteSyncJobRepository {
    async fn insert(&self, db: &dyn DatabaseAdapter, job: &SyncJob) -> Result<()> {
        let items_failed = job
            .stats
            .as_ref()
            .map_or(job.progress.items_failed, |s| s.items_failed);
        let items_added = job.stats.as_ref().map_or(0, |s| s.items_added);
        let items_updated = job.stats.as_ref().map_or(0, |s| s.items_updated);
        let items_deleted = job.stats.as_ref().map_or(0, |s| s.items_deleted);

        db.execute(
            r#"
            INSERT INTO sync_jobs (
                id, provider_id, status, sync_type,
                items_discovered, items_processed, items_failed,
                items_added, items_updated, items_deleted,
                error_message, error_details, cursor,
                started_at, completed_at, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            &[
                QueryValue::Text(job.id.as_str().to_string()),
                QueryValue::Text(job.provider_id.as_str().to_string()),
                QueryValue::Text(job.status.as_str().to_string()),
                QueryValue::Text(job.sync_type.as_str().to_string()),
                QueryValue::Integer(job.progress.items_discovered as i64),
                QueryValue::Integer(job.progress.items_processed as i64),
                QueryValue::Integer(items_failed as i64),
                QueryValue::Integer(items_added as i64),
                QueryValue::Integer(items_updated as i64),
                QueryValue::Integer(items_deleted as i64),
                opt_text(&job.error_message),
                opt_text(&job.error_details),
                opt_text(&job.cursor),
                opt_i64(job.started_at),
                opt_i64(job.completed_at),
                QueryValue::Integer(job.created_at),
            ],
        )
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, db: &dyn DatabaseAdapter, job: &SyncJob) -> Result<()> {
        let items_failed = job
            .stats
            .as_ref()
            .map_or(job.progress.items_failed, |s| s.items_failed);
        let items_added = job.stats.as_ref().map_or(0, |s| s.items_added);
        let items_updated = job.stats.as_ref().map_or(0, |s| s.items_updated);
        let items_deleted = job.stats.as_ref().map_or(0, |s| s.items_deleted);

        let affected = db
            .execute(
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
                &[
                    QueryValue::Text(job.status.as_str().to_string()),
                    QueryValue::Integer(job.progress.items_discovered as i64),
                    QueryValue::Integer(job.progress.items_processed as i64),
                    QueryValue::Integer(items_failed as i64),
                    QueryValue::Integer(items_added as i64),
                    QueryValue::Integer(items_updated as i64),
                    QueryValue::Integer(items_deleted as i64),
                    opt_text(&job.error_message),
                    opt_text(&job.error_details),
                    opt_text(&job.cursor),
                    opt_i64(job.started_at),
                    opt_i64(job.completed_at),
                    QueryValue::Text(job.id.as_str().to_string()),
                ],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if affected == 0 {
            return Err(SyncError::JobNotFound {
                job_id: job.id.to_string(),
            });
        }

        Ok(())
    }

    async fn find_by_id(
        &self,
        db: &dyn DatabaseAdapter,
        id: &SyncJobId,
    ) -> Result<Option<SyncJob>> {
        let row = db
            .query_one_optional(
                r#"
            SELECT id, provider_id, status, sync_type,
                   items_discovered, items_processed, items_failed,
                   items_added, items_updated, items_deleted,
                   error_message, error_details, cursor,
                   started_at, completed_at, created_at
            FROM sync_jobs
            WHERE id = ?
            "#,
                &[QueryValue::Text(id.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        row.map(|r| row_to_sync_job(&r)).transpose()
    }

    async fn find_by_provider(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<Vec<SyncJob>> {
        let rows = db
            .query(
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
                &[QueryValue::Text(provider.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.iter().map(row_to_sync_job).collect()
    }

    async fn find_by_status(
        &self,
        db: &dyn DatabaseAdapter,
        status: SyncStatus,
    ) -> Result<Vec<SyncJob>> {
        let rows = db
            .query(
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
                &[QueryValue::Text(status.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.iter().map(row_to_sync_job).collect()
    }

    async fn find_latest_by_provider(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<Option<SyncJob>> {
        let row = db
            .query_one_optional(
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
                &[QueryValue::Text(provider.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        row.map(|r| row_to_sync_job(&r)).transpose()
    }

    async fn get_history(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
        limit: u32,
    ) -> Result<Vec<SyncJob>> {
        let rows = db
            .query(
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
                &[
                    QueryValue::Text(provider.as_str().to_string()),
                    QueryValue::Integer(limit as i64),
                ],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        rows.iter().map(row_to_sync_job).collect()
    }

    async fn delete(&self, db: &dyn DatabaseAdapter, id: &SyncJobId) -> Result<()> {
        let affected = db
            .execute(
                "DELETE FROM sync_jobs WHERE id = ?",
                &[QueryValue::Text(id.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if affected == 0 {
            return Err(SyncError::JobNotFound {
                job_id: id.to_string(),
            });
        }

        Ok(())
    }

    async fn has_active_sync(
        &self,
        db: &dyn DatabaseAdapter,
        provider: ProviderKind,
    ) -> Result<bool> {
        let row = db
            .query_one(
                r#"
            SELECT COUNT(*) as count
            FROM sync_jobs
            WHERE provider_id = ? AND status IN ('pending', 'running')
            "#,
                &[QueryValue::Text(provider.as_str().to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let count = get_i64(&row, "count")?;
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
    use core_library::adapters::sqlite_native::SqliteAdapter;
    use std::sync::Arc;

    async fn create_test_adapter() -> Arc<dyn DatabaseAdapter> {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

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

        Arc::new(SqliteAdapter::from_pool(pool))
    }

    #[core_async::test]
    async fn test_insert_and_find_by_id() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;

        repo.insert(db.as_ref(), &job).await.unwrap();

        let found = repo.find_by_id(db.as_ref(), &job_id).await.unwrap();
        assert!(found.is_some());

        let found_job = found.unwrap();
        assert_eq!(found_job.id, job_id);
        assert_eq!(found_job.provider_id, ProviderKind::GoogleDrive);
        assert_eq!(found_job.status, SyncStatus::Pending);
    }

    #[core_async::test]
    async fn test_update_job() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;
        repo.insert(db.as_ref(), &job).await.unwrap();

        // Start the job
        let mut job = job.start().unwrap();
        job.update_progress(50, 100, "Processing").unwrap();
        repo.update(db.as_ref(), &job).await.unwrap();

        // Verify update
        let found = repo
            .find_by_id(db.as_ref(), &job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.status, SyncStatus::Running);
        assert_eq!(found.progress.items_processed, 50);
        assert_eq!(found.progress.items_discovered, 100);
    }

    #[core_async::test]
    async fn test_find_by_provider() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        // Insert multiple jobs
        let job1 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Incremental);
        let job3 = SyncJob::new(ProviderKind::OneDrive, SyncType::Full);

        repo.insert(db.as_ref(), &job1).await.unwrap();
        repo.insert(db.as_ref(), &job2).await.unwrap();
        repo.insert(db.as_ref(), &job3).await.unwrap();

        let google_jobs = repo
            .find_by_provider(db.as_ref(), ProviderKind::GoogleDrive)
            .await
            .unwrap();
        assert_eq!(google_jobs.len(), 2);

        let onedrive_jobs = repo
            .find_by_provider(db.as_ref(), ProviderKind::OneDrive)
            .await
            .unwrap();
        assert_eq!(onedrive_jobs.len(), 1);
    }

    #[core_async::test]
    async fn test_find_by_status() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        let job1 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job2 = job2.start().unwrap();

        repo.insert(db.as_ref(), &job1).await.unwrap();
        repo.insert(db.as_ref(), &job2).await.unwrap();

        let pending = repo
            .find_by_status(db.as_ref(), SyncStatus::Pending)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);

        let running = repo
            .find_by_status(db.as_ref(), SyncStatus::Running)
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
    }

    #[core_async::test]
    async fn test_has_active_sync() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        // No active sync initially
        assert!(!repo
            .has_active_sync(db.as_ref(), ProviderKind::GoogleDrive)
            .await
            .unwrap());

        // Add pending job
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        repo.insert(db.as_ref(), &job).await.unwrap();

        assert!(repo
            .has_active_sync(db.as_ref(), ProviderKind::GoogleDrive)
            .await
            .unwrap());

        // Complete the job
        let job = job.start().unwrap();
        let job = job.complete(SyncJobStats::new()).unwrap();
        repo.update(db.as_ref(), &job).await.unwrap();

        // No longer active
        assert!(!repo
            .has_active_sync(db.as_ref(), ProviderKind::GoogleDrive)
            .await
            .unwrap());
    }

    #[core_async::test]
    async fn test_get_history() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        // Create multiple jobs
        for _i in 0..5 {
            let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
            repo.insert(db.as_ref(), &job).await.unwrap();
            // Small delay to ensure different created_at times
            sleep(Duration::from_millis(10)).await;
        }

        let history = repo
            .get_history(db.as_ref(), ProviderKind::GoogleDrive, 3)
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
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job_id = job.id;

        repo.insert(db.as_ref(), &job).await.unwrap();
        assert!(repo
            .find_by_id(db.as_ref(), &job_id)
            .await
            .unwrap()
            .is_some());

        repo.delete(db.as_ref(), &job_id).await.unwrap();
        assert!(repo
            .find_by_id(db.as_ref(), &job_id)
            .await
            .unwrap()
            .is_none());
    }

    #[core_async::test]
    async fn test_complete_job_with_stats() {
        let db = create_test_adapter().await;
        let repo = SqliteSyncJobRepository::new();

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
        repo.insert(db.as_ref(), &job).await.unwrap();

        let found = repo
            .find_by_id(db.as_ref(), &job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.status, SyncStatus::Completed);
        assert!(found.stats.is_some());

        let found_stats = found.stats.unwrap();
        assert_eq!(found_stats.items_added, 100);
        assert_eq!(found_stats.items_updated, 20);
        assert_eq!(found_stats.items_deleted, 5);
        assert_eq!(found_stats.items_failed, 2);
    }
}
