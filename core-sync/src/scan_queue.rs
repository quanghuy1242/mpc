//! # Scan Queue System
//!
//! Work queue for processing discovered files during synchronization.
//!
//! ## Overview
//!
//! The scan queue manages work items representing files that need to be:
//! - Downloaded from cloud storage
//! - Metadata extracted
//! - Persisted to the library database
//!
//! ## Features
//!
//! - **Persistence**: Queue state persists to database for resumability
//! - **Prioritization**: New files processed before updates
//! - **Bounded Concurrency**: Process N files simultaneously
//! - **Retry Logic**: Failed items retry with exponential backoff
//! - **Progress Tracking**: Monitor queue size and completion status
//!
//! ## Usage
//!
//! ```ignore
//! use core_sync::{ScanQueue, WorkItem, Priority};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = create_database_pool().await?;
//! let queue = ScanQueue::new(pool.clone(), 5).await?; // Max 5 concurrent items
//!
//! // Enqueue a file for processing
//! let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
//! queue.enqueue(item).await?;
//!
//! // Dequeue and process
//! while let Some(item) = queue.dequeue().await? {
//!     match process_file(&item).await {
//!         Ok(_) => queue.mark_complete(item.id).await?,
//!         Err(e) => queue.mark_failed(item.id, Some(e.to_string())).await?,
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use core_async::sync::Semaphore;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{Result, SyncError};

/// Maximum number of retry attempts for failed items
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Initial backoff delay in milliseconds
const INITIAL_BACKOFF_MS: u64 = 100;

/// Type-safe work item identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkItemId(Uuid);

impl WorkItemId {
    /// Create a new random work item ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a work item ID from a string
    pub fn from_string(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| SyncError::InvalidJobId(e.to_string()))
    }

    /// Get the string representation
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for WorkItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Work item status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkItemStatus {
    /// Item is queued and waiting to be processed
    Pending,
    /// Item is currently being processed
    Processing,
    /// Item processing completed successfully
    Completed,
    /// Item processing failed
    Failed,
}

impl WorkItemStatus {
    /// Convert status to database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Check if status is terminal (completed or failed)
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Check if status is active (pending or processing)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Processing)
    }
}

impl std::str::FromStr for WorkItemStatus {
    type Err = SyncError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(SyncError::InvalidStatus(s.to_string())),
        }
    }
}

/// Priority level for work items
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    /// Low priority - existing file updates
    Low = 0,
    /// Normal priority - regular files
    #[default]
    Normal = 1,
    /// High priority - new files
    High = 2,
}

impl Priority {
    /// Convert priority to database integer
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }

    /// Parse priority from database integer
    pub fn from_i32(i: i32) -> Result<Self> {
        match i {
            0 => Ok(Self::Low),
            1 => Ok(Self::Normal),
            2 => Ok(Self::High),
            _ => Err(SyncError::InvalidStatus(format!("Invalid priority: {}", i))),
        }
    }
}

/// Work item representing a file to be processed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    /// Unique identifier
    pub id: WorkItemId,
    /// Remote file ID from storage provider
    pub remote_file_id: String,
    /// MIME type of the file
    pub mime_type: String,
    /// File size in bytes
    pub file_size: Option<i64>,
    /// Current status
    pub status: WorkItemStatus,
    /// Priority level
    pub priority: Priority,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
    /// Unix timestamp when processing started
    pub processing_started_at: Option<i64>,
}

impl WorkItem {
    /// Create a new work item
    pub fn new(remote_file_id: String, mime_type: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: WorkItemId::new(),
            remote_file_id,
            mime_type,
            file_size: None,
            status: WorkItemStatus::Pending,
            priority: Priority::Normal,
            retry_count: 0,
            error_message: None,
            created_at: now,
            updated_at: now,
            processing_started_at: None,
        }
    }

    /// Create a new work item with specified priority
    pub fn with_priority(remote_file_id: String, mime_type: String, priority: Priority) -> Self {
        let mut item = Self::new(remote_file_id, mime_type);
        item.priority = priority;
        item
    }

    /// Set file size
    pub fn with_file_size(mut self, size: i64) -> Self {
        self.file_size = Some(size);
        self
    }

    /// Calculate next retry delay in milliseconds using exponential backoff
    pub fn next_retry_delay_ms(&self) -> u64 {
        INITIAL_BACKOFF_MS * 2u64.pow(self.retry_count)
    }

    /// Check if item can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < MAX_RETRY_ATTEMPTS
    }

    /// Mark item as processing
    fn start_processing(&mut self) {
        self.status = WorkItemStatus::Processing;
        self.processing_started_at = Some(chrono::Utc::now().timestamp());
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Mark item as completed
    fn complete(&mut self) {
        self.status = WorkItemStatus::Completed;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Mark item as failed and increment retry count
    fn fail(&mut self, error_message: Option<String>) {
        self.retry_count += 1;
        self.error_message = error_message;
        self.updated_at = chrono::Utc::now().timestamp();

        if self.can_retry() {
            self.status = WorkItemStatus::Pending;
        } else {
            self.status = WorkItemStatus::Failed;
        }
    }
}

/// Repository trait for persisting scan queue to database
#[async_trait]
pub trait ScanQueueRepository: Send + Sync {
    /// Insert a work item
    async fn insert(&self, item: &WorkItem) -> Result<()>;

    /// Update a work item
    async fn update(&self, item: &WorkItem) -> Result<()>;

    /// Find work item by ID
    async fn find_by_id(&self, id: WorkItemId) -> Result<Option<WorkItem>>;

    /// Get next pending item by priority
    async fn get_next_pending(&self) -> Result<Option<WorkItem>>;

    /// Count items by status
    async fn count_by_status(&self, status: WorkItemStatus) -> Result<u64>;

    /// Delete completed items
    async fn delete_completed(&self) -> Result<u64>;

    /// Get all failed items
    async fn get_failed_items(&self) -> Result<Vec<WorkItem>>;
}

/// SQLite implementation of scan queue repository
pub struct SqliteScanQueueRepository {
    pool: SqlitePool,
}

impl SqliteScanQueueRepository {
    /// Create a new repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize database table if it doesn't exist
    pub async fn initialize(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS scan_queue (
                id TEXT PRIMARY KEY,
                remote_file_id TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                file_size INTEGER,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL,
                retry_count INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                processing_started_at INTEGER
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        // Create indexes for efficient queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_scan_queue_status_priority 
            ON scan_queue(status, priority DESC, created_at ASC)
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl ScanQueueRepository for SqliteScanQueueRepository {
    async fn insert(&self, item: &WorkItem) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO scan_queue (
                id, remote_file_id, mime_type, file_size, status, priority,
                retry_count, error_message, created_at, updated_at, processing_started_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(item.id.as_str())
        .bind(&item.remote_file_id)
        .bind(&item.mime_type)
        .bind(item.file_size)
        .bind(item.status.as_str())
        .bind(item.priority.as_i32())
        .bind(item.retry_count as i32)
        .bind(&item.error_message)
        .bind(item.created_at)
        .bind(item.updated_at)
        .bind(item.processing_started_at)
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(())
    }

    async fn update(&self, item: &WorkItem) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scan_queue SET
                status = ?,
                retry_count = ?,
                error_message = ?,
                updated_at = ?,
                processing_started_at = ?
            WHERE id = ?
            "#,
        )
        .bind(item.status.as_str())
        .bind(item.retry_count as i32)
        .bind(&item.error_message)
        .bind(item.updated_at)
        .bind(item.processing_started_at)
        .bind(item.id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: WorkItemId) -> Result<Option<WorkItem>> {
        let row = sqlx::query(
            r#"
            SELECT id, remote_file_id, mime_type, file_size, status, priority,
                   retry_count, error_message, created_at, updated_at, processing_started_at
            FROM scan_queue
            WHERE id = ?
            "#,
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        if let Some(row) = row {
            Ok(Some(WorkItem {
                id: WorkItemId::from_string(&row.get::<String, _>("id"))?,
                remote_file_id: row.get("remote_file_id"),
                mime_type: row.get("mime_type"),
                file_size: row.get("file_size"),
                status: row.get::<String, _>("status").parse()?,
                priority: Priority::from_i32(row.get("priority"))?,
                retry_count: row.get::<i32, _>("retry_count") as u32,
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                processing_started_at: row.get("processing_started_at"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_next_pending(&self) -> Result<Option<WorkItem>> {
        let row = sqlx::query(
            r#"
            SELECT id, remote_file_id, mime_type, file_size, status, priority,
                   retry_count, error_message, created_at, updated_at, processing_started_at
            FROM scan_queue
            WHERE status = 'pending'
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        if let Some(row) = row {
            Ok(Some(WorkItem {
                id: WorkItemId::from_string(&row.get::<String, _>("id"))?,
                remote_file_id: row.get("remote_file_id"),
                mime_type: row.get("mime_type"),
                file_size: row.get("file_size"),
                status: row.get::<String, _>("status").parse()?,
                priority: Priority::from_i32(row.get("priority"))?,
                retry_count: row.get::<i32, _>("retry_count") as u32,
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                processing_started_at: row.get("processing_started_at"),
            }))
        } else {
            Ok(None)
        }
    }

    async fn count_by_status(&self, status: WorkItemStatus) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scan_queue WHERE status = ?")
            .bind(status.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(count as u64)
    }

    async fn delete_completed(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM scan_queue WHERE status = 'completed'")
            .execute(&self.pool)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn get_failed_items(&self) -> Result<Vec<WorkItem>> {
        let rows = sqlx::query(
            r#"
            SELECT id, remote_file_id, mime_type, file_size, status, priority,
                   retry_count, error_message, created_at, updated_at, processing_started_at
            FROM scan_queue
            WHERE status = 'failed'
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(WorkItem {
                id: WorkItemId::from_string(&row.get::<String, _>("id"))?,
                remote_file_id: row.get("remote_file_id"),
                mime_type: row.get("mime_type"),
                file_size: row.get("file_size"),
                status: row.get::<String, _>("status").parse()?,
                priority: Priority::from_i32(row.get("priority"))?,
                retry_count: row.get::<i32, _>("retry_count") as u32,
                error_message: row.get("error_message"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                processing_started_at: row.get("processing_started_at"),
            });
        }

        Ok(items)
    }
}

/// Scan queue for managing file processing work items
pub struct ScanQueue {
    repository: Arc<dyn ScanQueueRepository>,
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl ScanQueue {
    /// Create a new scan queue with specified concurrency limit
    pub async fn new(pool: SqlitePool, max_concurrent: usize) -> Result<Self> {
        let repository = SqliteScanQueueRepository::new(pool);
        repository.initialize().await?;

        Ok(Self {
            repository: Arc::new(repository),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        })
    }

    /// Create a scan queue with custom repository
    pub fn with_repository(
        repository: Arc<dyn ScanQueueRepository>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            repository,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }

    /// Enqueue a work item for processing
    pub async fn enqueue(&self, item: WorkItem) -> Result<WorkItemId> {
        info!(
            work_item_id = %item.id,
            remote_file_id = %item.remote_file_id,
            priority = ?item.priority,
            "Enqueuing work item"
        );

        self.repository.insert(&item).await?;
        Ok(item.id)
    }

    /// Dequeue the next work item (blocks if at concurrency limit)
    pub async fn dequeue(&self) -> Result<Option<WorkItem>> {
        // Acquire permit from semaphore (blocks if at max concurrent)
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| SyncError::Provider("Semaphore closed".to_string()))?;

        // Get next pending item
        if let Some(mut item) = self.repository.get_next_pending().await? {
            item.start_processing();
            self.repository.update(&item).await?;

            debug!(
                work_item_id = %item.id,
                remote_file_id = %item.remote_file_id,
                retry_count = item.retry_count,
                "Dequeued work item"
            );

            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// Mark a work item as successfully completed
    pub async fn mark_complete(&self, item_id: WorkItemId) -> Result<()> {
        let mut item =
            self.repository
                .find_by_id(item_id)
                .await?
                .ok_or_else(|| SyncError::JobNotFound {
                    job_id: item_id.to_string(),
                })?;

        item.complete();
        self.repository.update(&item).await?;

        info!(
            work_item_id = %item_id,
            "Work item completed successfully"
        );

        Ok(())
    }

    /// Mark a work item as failed
    pub async fn mark_failed(
        &self,
        item_id: WorkItemId,
        error_message: Option<String>,
    ) -> Result<()> {
        let mut item =
            self.repository
                .find_by_id(item_id)
                .await?
                .ok_or_else(|| SyncError::JobNotFound {
                    job_id: item_id.to_string(),
                })?;

        let will_retry = item.can_retry();
        let retry_count = item.retry_count;
        item.fail(error_message.clone());
        self.repository.update(&item).await?;

        if will_retry {
            warn!(
                work_item_id = %item_id,
                retry_count = retry_count + 1,
                max_retries = MAX_RETRY_ATTEMPTS,
                backoff_ms = item.next_retry_delay_ms(),
                error = ?error_message,
                "Work item failed, will retry"
            );
        } else {
            warn!(
                work_item_id = %item_id,
                retry_count = retry_count + 1,
                error = ?error_message,
                "Work item failed permanently after max retries"
            );
        }

        Ok(())
    }

    /// Get status of a work item
    pub async fn get_status(&self, item_id: WorkItemId) -> Result<Option<WorkItem>> {
        self.repository.find_by_id(item_id).await
    }

    /// Get queue statistics
    pub async fn stats(&self) -> Result<QueueStats> {
        let pending = self
            .repository
            .count_by_status(WorkItemStatus::Pending)
            .await?;
        let processing = self
            .repository
            .count_by_status(WorkItemStatus::Processing)
            .await?;
        let completed = self
            .repository
            .count_by_status(WorkItemStatus::Completed)
            .await?;
        let failed = self
            .repository
            .count_by_status(WorkItemStatus::Failed)
            .await?;

        Ok(QueueStats {
            pending,
            processing,
            completed,
            failed,
            available_slots: self.semaphore.available_permits(),
            max_concurrent: self.max_concurrent,
        })
    }

    /// Clean up completed items from the queue
    pub async fn cleanup_completed(&self) -> Result<u64> {
        let deleted = self.repository.delete_completed().await?;
        info!(deleted_count = deleted, "Cleaned up completed work items");
        Ok(deleted)
    }

    /// Get all failed items for inspection
    pub async fn get_failed_items(&self) -> Result<Vec<WorkItem>> {
        self.repository.get_failed_items().await
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Number of pending items
    pub pending: u64,
    /// Number of items currently being processed
    pub processing: u64,
    /// Number of completed items
    pub completed: u64,
    /// Number of permanently failed items
    pub failed: u64,
    /// Number of available processing slots
    pub available_slots: usize,
    /// Maximum concurrent items
    pub max_concurrent: usize,
}

impl QueueStats {
    /// Calculate total items in queue
    pub fn total(&self) -> u64 {
        self.pending + self.processing + self.completed + self.failed
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.pending == 0 && self.processing == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_item_id() {
        let id = WorkItemId::new();
        let id_str = id.as_str();
        let parsed = WorkItemId::from_string(&id_str).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_work_item_status() {
        assert_eq!(WorkItemStatus::Pending.as_str(), "pending");
        assert_eq!(
            "pending".parse::<WorkItemStatus>().unwrap(),
            WorkItemStatus::Pending
        );
        assert!(WorkItemStatus::Completed.is_terminal());
        assert!(WorkItemStatus::Pending.is_active());
    }

    #[test]
    fn test_priority() {
        assert_eq!(Priority::High.as_i32(), 2);
        assert_eq!(Priority::from_i32(2).unwrap(), Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn test_work_item_creation() {
        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        assert_eq!(item.status, WorkItemStatus::Pending);
        assert_eq!(item.priority, Priority::Normal);
        assert_eq!(item.retry_count, 0);
    }

    #[test]
    fn test_work_item_with_priority() {
        let item = WorkItem::with_priority(
            "file123".to_string(),
            "audio/mpeg".to_string(),
            Priority::High,
        );
        assert_eq!(item.priority, Priority::High);
    }

    #[test]
    fn test_retry_backoff() {
        let mut item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());

        // First attempt
        assert_eq!(item.next_retry_delay_ms(), 100);
        assert!(item.can_retry());

        // After first failure
        item.retry_count = 1;
        assert_eq!(item.next_retry_delay_ms(), 200);
        assert!(item.can_retry());

        // After second failure
        item.retry_count = 2;
        assert_eq!(item.next_retry_delay_ms(), 400);
        assert!(item.can_retry());

        // After third failure (max retries)
        item.retry_count = 3;
        assert_eq!(item.next_retry_delay_ms(), 800);
        assert!(!item.can_retry());
    }

    #[test]
    fn test_work_item_state_transitions() {
        let mut item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());

        // Pending -> Processing
        item.start_processing();
        assert_eq!(item.status, WorkItemStatus::Processing);
        assert!(item.processing_started_at.is_some());

        // Processing -> Completed
        item.complete();
        assert_eq!(item.status, WorkItemStatus::Completed);

        // Test failure with retry
        let mut item2 = WorkItem::new("file456".to_string(), "audio/flac".to_string());
        item2.start_processing();
        item2.fail(Some("Test error".to_string()));
        assert_eq!(item2.status, WorkItemStatus::Pending); // Back to pending for retry
        assert_eq!(item2.retry_count, 1);
        assert_eq!(item2.error_message, Some("Test error".to_string()));

        // Test failure after max retries
        item2.retry_count = MAX_RETRY_ATTEMPTS;
        item2.fail(Some("Final error".to_string()));
        assert_eq!(item2.status, WorkItemStatus::Failed); // Permanently failed
    }

    #[core_async::test]
    async fn test_scan_queue_repository_init() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let repo = SqliteScanQueueRepository::new(pool);
        repo.initialize().await.unwrap();
    }

    #[core_async::test]
    async fn test_scan_queue_insert_and_find() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let repo = SqliteScanQueueRepository::new(pool);
        repo.initialize().await.unwrap();

        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        let item_id = item.id;

        repo.insert(&item).await.unwrap();

        let found = repo.find_by_id(item_id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, item_id);
        assert_eq!(found.remote_file_id, "file123");
    }

    #[core_async::test]
    async fn test_scan_queue_priority_ordering() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let repo = SqliteScanQueueRepository::new(pool);
        repo.initialize().await.unwrap();

        // Insert items with different priorities
        let low =
            WorkItem::with_priority("low".to_string(), "audio/mpeg".to_string(), Priority::Low);
        let normal = WorkItem::with_priority(
            "normal".to_string(),
            "audio/mpeg".to_string(),
            Priority::Normal,
        );
        let high =
            WorkItem::with_priority("high".to_string(), "audio/mpeg".to_string(), Priority::High);

        repo.insert(&low).await.unwrap();
        repo.insert(&normal).await.unwrap();
        repo.insert(&high).await.unwrap();

        // Should get high priority first
        let next = repo.get_next_pending().await.unwrap().unwrap();
        assert_eq!(next.priority, Priority::High);
    }

    #[core_async::test]
    async fn test_scan_queue_enqueue_dequeue() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let queue = ScanQueue::new(pool, 2).await.unwrap();

        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        let item_id = queue.enqueue(item).await.unwrap();

        let dequeued = queue.dequeue().await.unwrap();
        assert!(dequeued.is_some());
        let dequeued = dequeued.unwrap();
        assert_eq!(dequeued.id, item_id);
        assert_eq!(dequeued.status, WorkItemStatus::Processing);
    }

    #[core_async::test]
    async fn test_scan_queue_mark_complete() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let queue = ScanQueue::new(pool, 2).await.unwrap();

        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        let item_id = queue.enqueue(item).await.unwrap();

        queue.dequeue().await.unwrap();
        queue.mark_complete(item_id).await.unwrap();

        let status = queue.get_status(item_id).await.unwrap().unwrap();
        assert_eq!(status.status, WorkItemStatus::Completed);
    }

    #[core_async::test]
    async fn test_scan_queue_mark_failed_with_retry() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let queue = ScanQueue::new(pool, 2).await.unwrap();

        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        let item_id = queue.enqueue(item).await.unwrap();

        queue.dequeue().await.unwrap();
        queue
            .mark_failed(item_id, Some("Test error".to_string()))
            .await
            .unwrap();

        let status = queue.get_status(item_id).await.unwrap().unwrap();
        assert_eq!(status.status, WorkItemStatus::Pending); // Back to pending for retry
        assert_eq!(status.retry_count, 1);
    }

    #[core_async::test]
    async fn test_scan_queue_stats() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let queue = ScanQueue::new(pool, 2).await.unwrap();

        let item1 = WorkItem::new("file1".to_string(), "audio/mpeg".to_string());
        let item2 = WorkItem::new("file2".to_string(), "audio/mpeg".to_string());

        queue.enqueue(item1).await.unwrap();
        queue.enqueue(item2).await.unwrap();

        let stats = queue.stats().await.unwrap();
        assert_eq!(stats.pending, 2);
        assert_eq!(stats.processing, 0);
        assert_eq!(stats.max_concurrent, 2);
    }

    #[core_async::test]
    async fn test_scan_queue_cleanup() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let queue = ScanQueue::new(pool, 2).await.unwrap();

        let item = WorkItem::new("file123".to_string(), "audio/mpeg".to_string());
        let item_id = queue.enqueue(item).await.unwrap();

        queue.dequeue().await.unwrap();
        queue.mark_complete(item_id).await.unwrap();

        let deleted = queue.cleanup_completed().await.unwrap();
        assert_eq!(deleted, 1);

        let stats = queue.stats().await.unwrap();
        assert_eq!(stats.completed, 0);
    }
}
