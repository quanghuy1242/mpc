//! Conflict Resolution Orchestration
//!
//! This module orchestrates the conflict resolution workflow during sync operations.
//! It coordinates between the SyncCoordinator and ConflictResolver to handle:
//! - Duplicate detection and resolution
//! - Rename detection (files moved/renamed in provider)
//! - Deletion tracking (files removed from provider)
//! - Metadata conflict resolution
//!
//! ## Design Principles
//!
//! Following the architecture patterns from `docs/core_architecture.md`:
//! - **Separation of Concerns**: Isolated conflict resolution logic from sync orchestration
//! - **Fail-Fast with Descriptive Errors**: Clear error messages for all failure scenarios
//! - **Graceful Degradation**: Conflicts don't block entire sync operation
//! - **Event-Driven**: Emits progress events for conflict resolution phases
//!
//! ## Workflow
//!
//! 1. **Duplicate Detection**: Find tracks with identical content hashes
//! 2. **Duplicate Resolution**: Based on policy, keep highest quality or both
//! 3. **Rename Detection**: Match tracks by hash with different provider file IDs
//! 4. **Rename Resolution**: Update provider file ID without re-downloading
//! 5. **Deletion Detection**: Find tracks in DB but not in provider file list
//! 6. **Deletion Resolution**: Soft-delete or hard-delete based on policy
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_sync::conflict_resolution_orchestrator::ConflictResolutionOrchestrator;
//!
//! let orchestrator = ConflictResolutionOrchestrator::new(
//!     conflict_resolver,
//!     db_pool,
//!     event_bus,
//! );
//!
//! let stats = orchestrator.resolve_conflicts(
//!     job_id,
//!     provider_id,
//!     &provider_file_ids,
//! ).await?;
//! ```

use crate::{
    conflict_resolver::{ConflictPolicy, ConflictResolver, ResolutionResult},
    job::SyncJobId,
    Result, SyncError,
};
use core_library::models::TrackId;
use core_runtime::events::{CoreEvent, EventBus, SyncEvent};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

/// Statistics from conflict resolution phase
#[derive(Debug, Clone, Default)]
pub struct ConflictResolutionStats {
    /// Number of duplicate tracks detected
    pub duplicates_detected: u64,
    
    /// Number of duplicate tracks merged/removed
    pub duplicates_resolved: u64,
    
    /// Number of renames detected and resolved
    pub renames_resolved: u64,
    
    /// Number of tracks marked as deleted
    pub deletions_soft: u64,
    
    /// Number of tracks permanently deleted
    pub deletions_hard: u64,
    
    /// Total space reclaimed from deduplication (bytes)
    pub space_reclaimed: u64,
}

impl ConflictResolutionStats {
    /// Total number of items deleted (soft + hard)
    pub fn total_deleted(&self) -> u64 {
        self.deletions_soft + self.deletions_hard
    }
}

/// Orchestrates conflict resolution workflow during sync operations
pub struct ConflictResolutionOrchestrator {
    conflict_resolver: Arc<ConflictResolver>,
    db_pool: SqlitePool,
    event_bus: EventBus,
    policy: ConflictPolicy,
    hard_delete: bool,
}

impl ConflictResolutionOrchestrator {
    /// Create a new conflict resolution orchestrator
    ///
    /// # Arguments
    ///
    /// * `conflict_resolver` - The conflict resolver implementation
    /// * `db_pool` - Database connection pool
    /// * `event_bus` - Event bus for progress notifications
    /// * `policy` - Conflict resolution policy (KeepNewest, KeepBoth, UserPrompt)
    /// * `hard_delete` - Whether to permanently delete tracks (true) or soft-delete (false)
    pub fn new(
        conflict_resolver: Arc<ConflictResolver>,
        db_pool: SqlitePool,
        event_bus: EventBus,
        policy: ConflictPolicy,
        hard_delete: bool,
    ) -> Self {
        Self {
            conflict_resolver,
            db_pool,
            event_bus,
            policy,
            hard_delete,
        }
    }

    /// Execute full conflict resolution workflow
    ///
    /// This is the main entry point for conflict resolution. It performs all phases:
    /// duplicate detection/resolution, rename detection, and deletion tracking.
    ///
    /// # Arguments
    ///
    /// * `job_id` - Sync job ID for event emission
    /// * `provider_id` - Provider ID to scope the conflict resolution
    /// * `provider_file_ids` - Set of all file IDs currently in the provider
    ///
    /// # Returns
    ///
    /// Returns statistics about the conflict resolution operations performed.
    #[instrument(skip(self, provider_file_ids))]
    pub async fn resolve_conflicts(
        &self,
        job_id: &SyncJobId,
        provider_id: &str,
        provider_file_ids: &HashSet<String>,
    ) -> Result<ConflictResolutionStats> {
        info!(
            "Starting conflict resolution for provider {} (job {})",
            provider_id, job_id
        );

        let mut stats = ConflictResolutionStats::default();

        // Phase 1: Detect and resolve duplicates
        self.emit_progress(job_id, "duplicate_detection").await;
        self.resolve_duplicates(&mut stats).await?;

        // Phase 2: Detect and resolve renames
        self.emit_progress(job_id, "rename_detection").await;
        self.resolve_renames(provider_id, provider_file_ids, &mut stats)
            .await?;

        // Phase 3: Detect and handle deletions
        self.emit_progress(job_id, "deletion_tracking").await;
        self.handle_deletions(provider_id, provider_file_ids, &mut stats)
            .await?;

        info!(
            "Conflict resolution complete: {} duplicates resolved, {} renames, {} deletions",
            stats.duplicates_resolved,
            stats.renames_resolved,
            stats.total_deleted()
        );

        Ok(stats)
    }

    /// Detect and resolve duplicate tracks
    ///
    /// Finds tracks with identical content hashes and resolves them based on the
    /// configured policy. This helps reclaim storage space and clean up the library.
    #[instrument(skip(self, stats))]
    async fn resolve_duplicates(&self, stats: &mut ConflictResolutionStats) -> Result<()> {
        debug!("Detecting duplicate tracks by content hash");

        // Detect duplicates using the conflict resolver
        let duplicate_sets = self.conflict_resolver.detect_duplicates().await?;

        stats.duplicates_detected = duplicate_sets
            .iter()
            .map(|set| set.track_ids.len() as u64)
            .sum();

        info!(
            "Found {} duplicate sets containing {} total duplicates",
            duplicate_sets.len(),
            stats.duplicates_detected
        );

        // Resolve each duplicate set based on policy
        for dup_set in duplicate_sets {
            match self.policy {
                ConflictPolicy::KeepNewest | ConflictPolicy::KeepBoth => {
                    // Deduplicate by keeping best quality track
                    let results = self.conflict_resolver.deduplicate(&dup_set).await?;
                    
                    stats.duplicates_resolved += results.len() as u64;
                    stats.space_reclaimed += dup_set.wasted_space;

                    for result in results {
                        if let ResolutionResult::Merged { primary_id, duplicate_id } = result {
                            debug!(
                                "Merged duplicate {} into primary {}",
                                duplicate_id, primary_id
                            );
                        }
                    }
                }
                ConflictPolicy::UserPrompt => {
                    // Future: Surface duplicates to UI for manual resolution
                    warn!("UserPrompt policy not yet implemented for duplicates");
                }
            }
        }

        Ok(())
    }

    /// Detect and resolve file renames
    ///
    /// When a file is moved or renamed in the cloud provider, we want to update
    /// our database records rather than treating it as a deletion + new file.
    /// We detect renames by matching content hashes with different provider file IDs.
    #[instrument(skip(self, provider_file_ids, stats))]
    async fn resolve_renames(
        &self,
        provider_id: &str,
        provider_file_ids: &HashSet<String>,
        stats: &mut ConflictResolutionStats,
    ) -> Result<()> {
        debug!("Detecting renamed files by content hash matching");

        // Query tracks from this provider with hashes
        let tracks_with_hashes = self
            .query_tracks_with_hashes(provider_id)
            .await?;

        debug!(
            "Checking {} tracks with hashes for potential renames",
            tracks_with_hashes.len()
        );

        // Build a map of hash -> provider_file_id from the database
        let mut hash_to_db_file_id: HashMap<String, String> = HashMap::new();
        for (_hash, provider_file_id, _track_id) in &tracks_with_hashes {
            hash_to_db_file_id.insert(_hash.clone(), provider_file_id.clone());
        }

        // Query current provider files with hashes (if available from metadata)
        // For now, we detect renames by finding tracks whose provider_file_id
        // is no longer in the provider but whose hash matches another file
        let renames_detected = 0;

        for (_hash, old_provider_file_id, track_id) in tracks_with_hashes {
            // Check if the old provider file ID is still in the provider
            if !provider_file_ids.contains(&old_provider_file_id) {
                // File ID no longer exists - could be a rename or deletion
                // Try to find a new file with the same hash
                // This would require querying the provider for file hashes, which
                // is expensive. Instead, we'll handle this in the deletion phase
                // and rely on re-sync to detect renames organically.
                
                debug!(
                    "Track {} with provider_file_id {} not in current provider list",
                    track_id, old_provider_file_id
                );
            }
        }

        // Note: Full rename detection requires comparing hashes from the provider
        // response, which is not available in the current StorageProvider API.
        // This is a future enhancement (TASK-304.4).
        
        stats.renames_resolved = renames_detected;

        if renames_detected > 0 {
            info!("Resolved {} file renames", renames_detected);
        }

        Ok(())
    }

    /// Handle deleted files
    ///
    /// Detects tracks that exist in the database but are no longer in the provider.
    /// These tracks are either soft-deleted (marked but metadata kept) or hard-deleted
    /// (removed from database entirely) based on configuration.
    #[instrument(skip(self, provider_file_ids, stats))]
    async fn handle_deletions(
        &self,
        provider_id: &str,
        provider_file_ids: &HashSet<String>,
        stats: &mut ConflictResolutionStats,
    ) -> Result<()> {
        debug!("Detecting deleted files by comparing database with provider");

        // Query all tracks for this provider
        let db_tracks = self.query_all_tracks_for_provider(provider_id).await?;

        info!(
            "Comparing {} database tracks against {} provider files",
            db_tracks.len(),
            provider_file_ids.len()
        );

        let mut deleted_count = 0;

        for (provider_file_id, track_id) in db_tracks {
            // Skip already-deleted tracks (marked with DELETED_ prefix)
            if provider_file_id.starts_with("DELETED_") {
                continue;
            }

            // Check if this file still exists in the provider
            if !provider_file_ids.contains(&provider_file_id) {
                debug!(
                    "Track {} with provider_file_id {} not in provider - marking as deleted",
                    track_id, provider_file_id
                );

                // Handle deletion via conflict resolver
                match self
                    .conflict_resolver
                    .handle_deletion(&provider_file_id, self.hard_delete)
                    .await
                {
                    Ok(ResolutionResult::Deleted { track_id: deleted_track_id }) => {
                        deleted_count += 1;
                        
                        if self.hard_delete {
                            stats.deletions_hard += 1;
                            debug!("Hard deleted track {}", deleted_track_id);
                        } else {
                            stats.deletions_soft += 1;
                            debug!("Soft deleted track {}", deleted_track_id);
                        }
                    }
                    Ok(ResolutionResult::NoAction) => {
                        warn!(
                            "Track {} not found for deletion (possibly already deleted)",
                            track_id
                        );
                    }
                    Ok(_) => {
                        error!(
                            "Unexpected resolution result for deletion of track {}",
                            track_id
                        );
                    }
                    Err(e) => {
                        error!("Error deleting track {}: {}", track_id, e);
                        // Continue processing other deletions
                    }
                }
            }
        }

        if deleted_count > 0 {
            info!(
                "Processed {} deletions ({} soft, {} hard)",
                deleted_count, stats.deletions_soft, stats.deletions_hard
            );
        }

        Ok(())
    }

    /// Query tracks with hashes for rename detection
    async fn query_tracks_with_hashes(
        &self,
        provider_id: &str,
    ) -> Result<Vec<(String, String, TrackId)>> {
        let rows = sqlx::query(
            r#"
            SELECT hash, provider_file_id, id
            FROM tracks
            WHERE provider_id = ? AND hash IS NOT NULL
            "#,
        )
        .bind(provider_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut result = Vec::new();

        for row in rows {
            let hash: String = row
                .try_get("hash")
                .map_err(|e| SyncError::Database(e.to_string()))?;
            let provider_file_id: String = row
                .try_get("provider_file_id")
                .map_err(|e| SyncError::Database(e.to_string()))?;
            let track_id_str: String = row
                .try_get("id")
                .map_err(|e| SyncError::Database(e.to_string()))?;

            let track_id = TrackId::from_string(&track_id_str).map_err(|e| {
                SyncError::InvalidInput {
                    field: "track_id".to_string(),
                    message: e.to_string(),
                }
            })?;

            result.push((hash, provider_file_id, track_id));
        }

        Ok(result)
    }

    /// Query all tracks for a provider
    async fn query_all_tracks_for_provider(
        &self,
        provider_id: &str,
    ) -> Result<Vec<(String, TrackId)>> {
        let rows = sqlx::query(
            r#"
            SELECT provider_file_id, id
            FROM tracks
            WHERE provider_id = ?
            "#,
        )
        .bind(provider_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut result = Vec::new();

        for row in rows {
            let provider_file_id: String = row
                .try_get("provider_file_id")
                .map_err(|e| SyncError::Database(e.to_string()))?;
            let track_id_str: String = row
                .try_get("id")
                .map_err(|e| SyncError::Database(e.to_string()))?;

            let track_id = TrackId::from_string(&track_id_str).map_err(|e| {
                SyncError::InvalidInput {
                    field: "track_id".to_string(),
                    message: e.to_string(),
                }
            })?;

            result.push((provider_file_id, track_id));
        }

        Ok(result)
    }

    /// Emit progress event for conflict resolution phase
    async fn emit_progress(&self, job_id: &SyncJobId, phase: &str) {
        self.event_bus
            .emit(CoreEvent::Sync(SyncEvent::Progress {
                job_id: job_id.to_string(),
                items_processed: 0,
                total_items: None,
                percent: 0,
                phase: format!("conflict_resolution.{}", phase),
            }))
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict_resolver::ConflictResolver;
    use core_library::db::DatabaseConfig;
    use core_runtime::events::EventBus;

    async fn create_test_pool() -> SqlitePool {
        let config = DatabaseConfig::in_memory();
        let pool = core_library::db::create_pool(config).await.unwrap();

        // Create test provider
        sqlx::query(
            r#"
            INSERT INTO providers (id, type, display_name, profile_id, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind("test_provider")
        .bind("GoogleDrive")
        .bind("Test Provider")
        .bind("test_profile_id")
        .bind(chrono::Utc::now().timestamp())
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    async fn create_test_track(
        pool: &SqlitePool,
        provider_file_id: &str,
        hash: Option<&str>,
    ) -> TrackId {
        let track_id = TrackId::new();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT INTO tracks (
                id, provider_id, provider_file_id, title, normalized_title,
                duration_ms, format, file_size, created_at, updated_at, hash
            ) VALUES (?, ?, ?, ?, LOWER(TRIM(?)), ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(track_id.to_string())
        .bind("test_provider")
        .bind(provider_file_id)
        .bind("Test Track")
        .bind("Test Track")
        .bind(180000)
        .bind("mp3")
        .bind(5000000)
        .bind(now)
        .bind(now)
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();

        track_id
    }

    #[tokio::test]
    async fn test_deletion_tracking_soft() {
        let pool = create_test_pool().await;
        let resolver = Arc::new(ConflictResolver::new(
            pool.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            pool.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false, // soft delete
        );

        // Create tracks
        let _track1 = create_test_track(&pool, "file_1", None).await;
        let _track2 = create_test_track(&pool, "file_2", None).await;
        let _track3 = create_test_track(&pool, "file_3", None).await;

        // Provider only has file_1 and file_2 (file_3 was deleted)
        let mut provider_files = HashSet::new();
        provider_files.insert("file_1".to_string());
        provider_files.insert("file_2".to_string());

        let job_id = SyncJobId::new();
        let stats = orchestrator
            .resolve_conflicts(&job_id, "test_provider", &provider_files)
            .await
            .unwrap();

        // Should detect 1 deletion (file_3)
        assert_eq!(stats.deletions_soft, 1);
        assert_eq!(stats.deletions_hard, 0);
        assert_eq!(stats.total_deleted(), 1);

        // Verify track is soft-deleted (marked with DELETED_ prefix)
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tracks WHERE provider_file_id LIKE 'DELETED_%'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_deletion_tracking_hard() {
        let pool = create_test_pool().await;
        let resolver = Arc::new(ConflictResolver::new(
            pool.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            pool.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            true, // hard delete
        );

        // Create tracks
        create_test_track(&pool, "file_1", None).await;
        create_test_track(&pool, "file_2", None).await;
        create_test_track(&pool, "file_3", None).await;

        // Provider only has file_1 (file_2 and file_3 were deleted)
        let mut provider_files = HashSet::new();
        provider_files.insert("file_1".to_string());

        let job_id = SyncJobId::new();
        let stats = orchestrator
            .resolve_conflicts(&job_id, "test_provider", &provider_files)
            .await
            .unwrap();

        // Should detect 2 deletions
        assert_eq!(stats.deletions_soft, 0);
        assert_eq!(stats.deletions_hard, 2);
        assert_eq!(stats.total_deleted(), 2);

        // Verify tracks are hard-deleted (removed from database)
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks WHERE provider_id = ?")
            .bind("test_provider")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 1); // Only file_1 remains
    }

    #[tokio::test]
    async fn test_duplicate_resolution() {
        let pool = create_test_pool().await;
        let resolver = Arc::new(ConflictResolver::new(
            pool.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            pool.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false,
        );

        // Create duplicate tracks with same hash
        let hash = "duplicate_hash_123";
        create_test_track(&pool, "file_1", Some(hash)).await;
        create_test_track(&pool, "file_2", Some(hash)).await;
        create_test_track(&pool, "file_3", Some(hash)).await;

        let provider_files = HashSet::new();
        let job_id = SyncJobId::new();
        
        let stats = orchestrator
            .resolve_conflicts(&job_id, "test_provider", &provider_files)
            .await
            .unwrap();

        // Should detect 3 duplicates and resolve by keeping 1
        assert_eq!(stats.duplicates_detected, 3);
        assert_eq!(stats.duplicates_resolved, 2); // 2 merged into primary

        // Verify only 1 track with this hash remains
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks WHERE hash = ?")
            .bind(hash)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_no_conflicts() {
        let pool = create_test_pool().await;
        let resolver = Arc::new(ConflictResolver::new(
            pool.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            pool.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false,
        );

        // Create tracks that all exist in provider
        create_test_track(&pool, "file_1", None).await;
        create_test_track(&pool, "file_2", None).await;

        let mut provider_files = HashSet::new();
        provider_files.insert("file_1".to_string());
        provider_files.insert("file_2".to_string());

        let job_id = SyncJobId::new();
        let stats = orchestrator
            .resolve_conflicts(&job_id, "test_provider", &provider_files)
            .await
            .unwrap();

        // No conflicts should be detected
        assert_eq!(stats.duplicates_detected, 0);
        assert_eq!(stats.duplicates_resolved, 0);
        assert_eq!(stats.renames_resolved, 0);
        assert_eq!(stats.total_deleted(), 0);
    }
}
