//! # Sync Job State Machine
//!
//! Manages the lifecycle of sync jobs with validated state transitions.
//!
//! ## Overview
//!
//! This module provides a state machine for sync job lifecycle management, ensuring
//! that state transitions are valid and that progress is tracked throughout the sync
//! process. Jobs persist across restarts via database storage.
//!
//! ## State Machine
//!
//! ```text
//! Pending → Running → Completed
//!     ↓         ↓         ↑
//!     └──────→ Failed    │
//!     └──────→ Cancelled │
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_sync::{SyncJob, SyncJobId, SyncStatus, SyncType};
//! use core_auth::ProviderKind;
//!
//! // Create a new sync job
//! let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
//!
//! // Start the job
//! let mut job = job.start()?;
//!
//! // Update progress
//! job.update_progress(50, 100, "Processing files")?;
//!
//! // Complete the job
//! let job = job.complete(SyncJobStats {
//!     items_added: 45,
//!     items_updated: 5,
//!     items_deleted: 0,
//!     items_failed: 0,
//! })?;
//! ```

use crate::{Result, SyncError};
use core_auth::ProviderKind;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// ID Types
// ============================================================================

/// Unique identifier for a sync job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SyncJobId(Uuid);

impl SyncJobId {
    /// Create a new random sync job ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a sync job ID from a string
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID
    pub fn from_string(s: &str) -> Result<Self> {
        Ok(Self(
            Uuid::parse_str(s).map_err(|e| SyncError::InvalidJobId(e.to_string()))?,
        ))
    }

    /// Get the string representation of this ID
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SyncJobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SyncJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SyncJobId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<SyncJobId> for Uuid {
    fn from(id: SyncJobId) -> Self {
        id.0
    }
}

// ============================================================================
// Status Types
// ============================================================================

/// The current status of a sync job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    /// Job has been created but not yet started
    Pending,
    /// Job is currently running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed with an error
    Failed,
    /// Job was cancelled by the user
    Cancelled,
}

impl SyncStatus {
    /// Check if this status represents a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SyncStatus::Completed | SyncStatus::Failed | SyncStatus::Cancelled
        )
    }

    /// Check if this status represents an active state
    pub fn is_active(&self) -> bool {
        matches!(self, SyncStatus::Pending | SyncStatus::Running)
    }

    /// Get the string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatus::Pending => "pending",
            SyncStatus::Running => "running",
            SyncStatus::Completed => "completed",
            SyncStatus::Failed => "failed",
            SyncStatus::Cancelled => "cancelled",
        }
    }
}

impl FromStr for SyncStatus {
    type Err = SyncError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(SyncStatus::Pending),
            "running" => Ok(SyncStatus::Running),
            "completed" => Ok(SyncStatus::Completed),
            "failed" => Ok(SyncStatus::Failed),
            "cancelled" => Ok(SyncStatus::Cancelled),
            _ => Err(SyncError::InvalidStatus(s.to_string())),
        }
    }
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The type of sync being performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    /// Full synchronization of all files
    Full,
    /// Incremental synchronization using a cursor
    Incremental,
}

impl SyncType {
    /// Get the string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncType::Full => "full",
            SyncType::Incremental => "incremental",
        }
    }
}

impl FromStr for SyncType {
    type Err = SyncError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "full" => Ok(SyncType::Full),
            "incremental" => Ok(SyncType::Incremental),
            _ => Err(SyncError::InvalidSyncType(s.to_string())),
        }
    }
}

impl std::fmt::Display for SyncType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Progress Types
// ============================================================================

/// Progress information for a running sync job
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncProgress {
    /// Total number of items discovered
    pub items_discovered: u64,
    /// Number of items processed so far
    pub items_processed: u64,
    /// Number of items that failed processing
    pub items_failed: u64,
    /// Progress percentage (0-100)
    pub percent: u8,
    /// Current processing phase
    pub phase: String,
}

impl SyncProgress {
    /// Create a new progress tracker
    pub fn new() -> Self {
        Self {
            items_discovered: 0,
            items_processed: 0,
            items_failed: 0,
            percent: 0,
            phase: "Initializing".to_string(),
        }
    }

    /// Update progress with new values
    pub fn update(&mut self, items_processed: u64, items_discovered: u64, phase: &str) {
        self.items_processed = items_processed;
        self.items_discovered = items_discovered;
        self.phase = phase.to_string();

        // Calculate percentage (cap at 100)
        self.percent = if items_discovered > 0 {
            ((items_processed as f64 / items_discovered as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };
    }

    /// Increment failed items counter
    pub fn increment_failed(&mut self) {
        self.items_failed += 1;
    }
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics collected upon sync job completion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncJobStats {
    /// Number of new tracks added to library
    pub items_added: u64,
    /// Number of existing tracks updated
    pub items_updated: u64,
    /// Number of tracks deleted from library
    pub items_deleted: u64,
    /// Number of items that failed processing
    pub items_failed: u64,
}

impl SyncJobStats {
    /// Create empty stats
    pub fn new() -> Self {
        Self {
            items_added: 0,
            items_updated: 0,
            items_deleted: 0,
            items_failed: 0,
        }
    }

    /// Get total items processed (added + updated + deleted)
    pub fn total_processed(&self) -> u64 {
        self.items_added + self.items_updated + self.items_deleted
    }
}

impl Default for SyncJobStats {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Sync Job Entity
// ============================================================================

/// A sync job with state machine semantics
///
/// This type enforces valid state transitions at compile time through
/// the type system. Jobs can only be created in `Pending` state and must
/// transition through valid states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncJob {
    /// Unique identifier for this job
    pub id: SyncJobId,
    /// The provider being synced
    pub provider_id: ProviderKind,
    /// Current status
    pub status: SyncStatus,
    /// Type of sync
    pub sync_type: SyncType,
    /// Progress information
    pub progress: SyncProgress,
    /// Statistics (only available when completed)
    pub stats: Option<SyncJobStats>,
    /// Sync cursor for resumable sync
    pub cursor: Option<String>,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Additional error details (JSON)
    pub error_details: Option<String>,
    /// When the job was created
    pub created_at: i64,
    /// When the job started running
    pub started_at: Option<i64>,
    /// When the job completed
    pub completed_at: Option<i64>,
}

impl SyncJob {
    /// Create a new sync job in pending state
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use core_sync::{SyncJob, SyncType};
    /// use core_auth::ProviderKind;
    ///
    /// let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
    /// assert_eq!(job.status, SyncStatus::Pending);
    /// ```
    pub fn new(provider_id: ProviderKind, sync_type: SyncType) -> Self {
        Self {
            id: SyncJobId::new(),
            provider_id,
            status: SyncStatus::Pending,
            sync_type,
            progress: SyncProgress::new(),
            stats: None,
            cursor: None,
            error_message: None,
            error_details: None,
            created_at: current_timestamp(),
            started_at: None,
            completed_at: None,
        }
    }

    /// Create a new incremental sync job with a cursor
    pub fn new_incremental(provider_id: ProviderKind, cursor: String) -> Self {
        let mut job = Self::new(provider_id, SyncType::Incremental);
        job.cursor = Some(cursor);
        job
    }

    /// Start the sync job
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not in `Pending` state
    pub fn start(mut self) -> Result<Self> {
        self.validate_transition(SyncStatus::Running)?;
        self.status = SyncStatus::Running;
        self.started_at = Some(current_timestamp());
        self.progress.phase = "Starting sync".to_string();
        Ok(self)
    }

    /// Update progress information
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not in `Running` state
    pub fn update_progress(
        &mut self,
        items_processed: u64,
        items_discovered: u64,
        phase: &str,
    ) -> Result<()> {
        if self.status != SyncStatus::Running {
            return Err(SyncError::InvalidStateTransition {
                from: self.status.as_str().to_string(),
                to: "update_progress".to_string(),
                reason: "Job must be running to update progress".to_string(),
            });
        }

        self.progress
            .update(items_processed, items_discovered, phase);
        Ok(())
    }

    /// Update the sync cursor
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not in `Running` state
    pub fn update_cursor(&mut self, cursor: String) -> Result<()> {
        if self.status != SyncStatus::Running {
            return Err(SyncError::InvalidStateTransition {
                from: self.status.as_str().to_string(),
                to: "update_cursor".to_string(),
                reason: "Job must be running to update cursor".to_string(),
            });
        }

        self.cursor = Some(cursor);
        Ok(())
    }

    /// Mark the job as completed with statistics
    ///
    /// # Errors
    ///
    /// Returns an error if the job is not in `Running` state
    pub fn complete(mut self, stats: SyncJobStats) -> Result<Self> {
        self.validate_transition(SyncStatus::Completed)?;
        self.status = SyncStatus::Completed;
        self.completed_at = Some(current_timestamp());
        self.stats = Some(stats);
        self.progress.percent = 100;
        self.progress.phase = "Completed".to_string();
        Ok(self)
    }

    /// Mark the job as failed with an error message
    ///
    /// # Errors
    ///
    /// Returns an error if the job is already in a terminal state other than Running/Pending
    pub fn fail(mut self, error_message: String, error_details: Option<String>) -> Result<Self> {
        self.validate_transition(SyncStatus::Failed)?;
        self.status = SyncStatus::Failed;
        self.completed_at = Some(current_timestamp());
        self.error_message = Some(error_message);
        self.error_details = error_details;
        self.progress.phase = "Failed".to_string();
        Ok(self)
    }

    /// Cancel the job
    ///
    /// # Errors
    ///
    /// Returns an error if the job is already in a terminal state
    pub fn cancel(mut self) -> Result<Self> {
        self.validate_transition(SyncStatus::Cancelled)?;
        self.status = SyncStatus::Cancelled;
        self.completed_at = Some(current_timestamp());
        self.progress.phase = "Cancelled".to_string();
        Ok(self)
    }

    /// Get the duration of the job in seconds
    ///
    /// Returns None if the job hasn't started or completed yet
    pub fn duration_secs(&self) -> Option<u64> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some((end - start) as u64),
            _ => None,
        }
    }

    /// Validate a state transition
    fn validate_transition(&self, to: SyncStatus) -> Result<()> {
        let valid = match (self.status, to) {
            // From Pending
            (SyncStatus::Pending, SyncStatus::Running) => true,
            (SyncStatus::Pending, SyncStatus::Cancelled) => true,
            (SyncStatus::Pending, SyncStatus::Failed) => true,

            // From Running
            (SyncStatus::Running, SyncStatus::Completed) => true,
            (SyncStatus::Running, SyncStatus::Failed) => true,
            (SyncStatus::Running, SyncStatus::Cancelled) => true,

            // Terminal states cannot transition
            (SyncStatus::Completed, _) => false,
            (SyncStatus::Failed, _) => false,
            (SyncStatus::Cancelled, _) => false,

            // All other transitions are invalid
            _ => false,
        };

        if !valid {
            return Err(SyncError::InvalidStateTransition {
                from: self.status.as_str().to_string(),
                to: to.as_str().to_string(),
                reason: format!(
                    "Cannot transition from {} to {}",
                    self.status.as_str(),
                    to.as_str()
                ),
            });
        }

        Ok(())
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs() as i64
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_job_id_new() {
        let id1 = SyncJobId::new();
        let id2 = SyncJobId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_sync_job_id_from_string() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = SyncJobId::from_string(uuid_str).unwrap();
        assert_eq!(id.as_str(), uuid_str);
    }

    #[test]
    fn test_sync_status_is_terminal() {
        assert!(!SyncStatus::Pending.is_terminal());
        assert!(!SyncStatus::Running.is_terminal());
        assert!(SyncStatus::Completed.is_terminal());
        assert!(SyncStatus::Failed.is_terminal());
        assert!(SyncStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_sync_status_is_active() {
        assert!(SyncStatus::Pending.is_active());
        assert!(SyncStatus::Running.is_active());
        assert!(!SyncStatus::Completed.is_active());
        assert!(!SyncStatus::Failed.is_active());
        assert!(!SyncStatus::Cancelled.is_active());
    }

    #[test]
    fn test_sync_status_from_str() {
        assert_eq!(
            SyncStatus::from_str("pending").unwrap(),
            SyncStatus::Pending
        );
        assert_eq!(
            SyncStatus::from_str("RUNNING").unwrap(),
            SyncStatus::Running
        );
        assert_eq!(
            SyncStatus::from_str("completed").unwrap(),
            SyncStatus::Completed
        );
        assert!(SyncStatus::from_str("invalid").is_err());
    }

    #[test]
    fn test_sync_type_parsing() {
        assert_eq!("full".parse::<SyncType>().unwrap(), SyncType::Full);
        assert_eq!(
            "INCREMENTAL".parse::<SyncType>().unwrap(),
            SyncType::Incremental
        );
        assert!("invalid".parse::<SyncType>().is_err());
    }

    #[test]
    fn test_sync_progress_update() {
        let mut progress = SyncProgress::new();
        progress.update(50, 100, "Processing");

        assert_eq!(progress.items_processed, 50);
        assert_eq!(progress.items_discovered, 100);
        assert_eq!(progress.percent, 50);
        assert_eq!(progress.phase, "Processing");
    }

    #[test]
    fn test_sync_progress_percent_calculation() {
        let mut progress = SyncProgress::new();

        // 0% when no items discovered
        progress.update(0, 0, "Test");
        assert_eq!(progress.percent, 0);

        // 50% when half processed
        progress.update(50, 100, "Test");
        assert_eq!(progress.percent, 50);

        // 100% when all processed
        progress.update(100, 100, "Test");
        assert_eq!(progress.percent, 100);

        // Cap at 100% even if processed exceeds discovered
        progress.update(150, 100, "Test");
        assert_eq!(progress.percent, 100);
    }

    #[test]
    fn test_sync_job_stats_total_processed() {
        let stats = SyncJobStats {
            items_added: 10,
            items_updated: 5,
            items_deleted: 2,
            items_failed: 3,
        };

        assert_eq!(stats.total_processed(), 17);
    }

    #[test]
    fn test_sync_job_new() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

        assert_eq!(job.status, SyncStatus::Pending);
        assert_eq!(job.provider_id, ProviderKind::GoogleDrive);
        assert_eq!(job.sync_type, SyncType::Full);
        assert!(job.cursor.is_none());
        assert!(job.started_at.is_none());
        assert!(job.completed_at.is_none());
    }

    #[test]
    fn test_sync_job_new_incremental() {
        let job = SyncJob::new_incremental(ProviderKind::OneDrive, "cursor-123".to_string());

        assert_eq!(job.sync_type, SyncType::Incremental);
        assert_eq!(job.cursor, Some("cursor-123".to_string()));
    }

    #[test]
    fn test_sync_job_start() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();

        assert_eq!(job.status, SyncStatus::Running);
        assert!(job.started_at.is_some());
        assert_eq!(job.progress.phase, "Starting sync");
    }

    #[test]
    fn test_sync_job_start_invalid_state() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();

        // Try to start again - should fail
        let result = job.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_job_update_progress() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let mut job = job.start().unwrap();

        job.update_progress(50, 100, "Processing files").unwrap();

        assert_eq!(job.progress.items_processed, 50);
        assert_eq!(job.progress.items_discovered, 100);
        assert_eq!(job.progress.percent, 50);
        assert_eq!(job.progress.phase, "Processing files");
    }

    #[test]
    fn test_sync_job_update_progress_invalid_state() {
        let mut job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

        // Try to update progress when not running
        let result = job.update_progress(50, 100, "Test");
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_job_update_cursor() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Incremental);
        let mut job = job.start().unwrap();

        job.update_cursor("new-cursor".to_string()).unwrap();
        assert_eq!(job.cursor, Some("new-cursor".to_string()));
    }

    #[test]
    fn test_sync_job_complete() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();

        let stats = SyncJobStats {
            items_added: 10,
            items_updated: 5,
            items_deleted: 2,
            items_failed: 0,
        };

        let job = job.complete(stats).unwrap();

        assert_eq!(job.status, SyncStatus::Completed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.stats, Some(stats));
        assert_eq!(job.progress.percent, 100);
        assert_eq!(job.progress.phase, "Completed");
    }

    #[test]
    fn test_sync_job_complete_invalid_state() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

        // Try to complete without starting
        let result = job.complete(SyncJobStats::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_job_fail() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();

        let job = job
            .fail("Connection timeout".to_string(), Some("{}".to_string()))
            .unwrap();

        assert_eq!(job.status, SyncStatus::Failed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.error_message, Some("Connection timeout".to_string()));
        assert_eq!(job.progress.phase, "Failed");
    }

    #[test]
    fn test_sync_job_cancel() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();

        let job = job.cancel().unwrap();

        assert_eq!(job.status, SyncStatus::Cancelled);
        assert!(job.completed_at.is_some());
        assert_eq!(job.progress.phase, "Cancelled");
    }

    #[test]
    fn test_sync_job_cancel_from_pending() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

        let job = job.cancel().unwrap();

        assert_eq!(job.status, SyncStatus::Cancelled);
    }

    #[test]
    fn test_sync_job_duration() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

        // No duration before start
        assert!(job.duration_secs().is_none());

        let job = job.start().unwrap();

        // No duration while running
        assert!(job.duration_secs().is_none());

        let job = job.complete(SyncJobStats::new()).unwrap();

        // Has duration after completion
        assert!(job.duration_secs().is_some());
    }

    #[test]
    fn test_sync_job_terminal_states_cannot_transition() {
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        let job = job.start().unwrap();
        let completed_job = job.complete(SyncJobStats::new()).unwrap();

        // Cannot start from completed
        assert!(completed_job.clone().start().is_err());

        // Cannot fail from completed
        assert!(completed_job
            .clone()
            .fail("Error".to_string(), None)
            .is_err());

        // Cannot cancel from completed
        assert!(completed_job.cancel().is_err());
    }

    #[test]
    fn test_state_machine_full_workflow() {
        // Create job
        let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);
        assert_eq!(job.status, SyncStatus::Pending);

        // Start job
        let mut job = job.start().unwrap();
        assert_eq!(job.status, SyncStatus::Running);

        // Update progress
        job.update_progress(25, 100, "Listing files").unwrap();
        assert_eq!(job.progress.percent, 25);

        job.update_progress(50, 100, "Processing files").unwrap();
        assert_eq!(job.progress.percent, 50);

        job.update_progress(75, 100, "Extracting metadata").unwrap();
        assert_eq!(job.progress.percent, 75);

        // Complete job
        let stats = SyncJobStats {
            items_added: 80,
            items_updated: 15,
            items_deleted: 5,
            items_failed: 0,
        };
        let job = job.complete(stats).unwrap();
        assert_eq!(job.status, SyncStatus::Completed);
        assert_eq!(job.progress.percent, 100);
        assert!(job.duration_secs().is_some());
    }
}
