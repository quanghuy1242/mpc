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
use bridge_traits::database::{DatabaseAdapter, QueryValue};
use core_library::models::TrackId;
use core_runtime::events::{CoreEvent, EventBus, SyncEvent};
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
    db: Arc<dyn DatabaseAdapter>,
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
    /// * `db` - Database adapter for persistence
    /// * `event_bus` - Event bus for progress notifications
    /// * `policy` - Conflict resolution policy (KeepNewest, KeepBoth, UserPrompt)
    /// * `hard_delete` - Whether to permanently delete tracks (true) or soft-delete (false)
    pub fn new(
        conflict_resolver: Arc<ConflictResolver>,
        db: Arc<dyn DatabaseAdapter>,
        event_bus: EventBus,
        policy: ConflictPolicy,
        hard_delete: bool,
    ) -> Self {
        Self {
            conflict_resolver,
            db,
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
                        if let ResolutionResult::Merged {
                            primary_id,
                            duplicate_id,
                        } = result
                        {
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
        let tracks_with_hashes = self.query_tracks_with_hashes(provider_id).await?;

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
                    Ok(ResolutionResult::Deleted {
                        track_id: deleted_track_id,
                    }) => {
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
        let query = r#"
            SELECT hash, provider_file_id, id
            FROM tracks
            WHERE provider_id = ? AND hash IS NOT NULL
            "#;

        let rows = self
            .db
            .query(query, &[QueryValue::Text(provider_id.to_string())])
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut result = Vec::new();

        for row in rows {
            let hash = row
                .get("hash")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing hash field".to_string()))?;

            let provider_file_id = row
                .get("provider_file_id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing provider_file_id field".to_string()))?;

            let track_id_str = row
                .get("id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;

            let track_id =
                TrackId::from_string(&track_id_str).map_err(|e| SyncError::InvalidInput {
                    field: "track_id".to_string(),
                    message: e.to_string(),
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
        let query = r#"
            SELECT provider_file_id, id
            FROM tracks
            WHERE provider_id = ?
            "#;

        let rows = self
            .db
            .query(query, &[QueryValue::Text(provider_id.to_string())])
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut result = Vec::new();

        for row in rows {
            let provider_file_id = row
                .get("provider_file_id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing provider_file_id field".to_string()))?;

            let track_id_str = row
                .get("id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;

            let track_id =
                TrackId::from_string(&track_id_str).map_err(|e| SyncError::InvalidInput {
                    field: "track_id".to_string(),
                    message: e.to_string(),
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
    use bridge_traits::database::DatabaseAdapter;
    use core_library::adapters::sqlite_native::SqliteAdapter;
    use core_library::db::DatabaseConfig;
    use core_runtime::events::EventBus;

    async fn create_test_db() -> Arc<dyn DatabaseAdapter> {
        let config = DatabaseConfig::in_memory();
        let pool = core_library::db::create_pool(config).await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(SqliteAdapter::from_pool(pool));

        // Create test provider
        db.execute(
            r#"
            INSERT INTO providers (id, type, display_name, profile_id, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            &[
                QueryValue::Text("test_provider".to_string()),
                QueryValue::Text("GoogleDrive".to_string()),
                QueryValue::Text("Test Provider".to_string()),
                QueryValue::Text("test_profile_id".to_string()),
                QueryValue::Integer(chrono::Utc::now().timestamp()),
            ],
        )
        .await
        .unwrap();

        db
    }

    async fn create_test_track(
        db: &Arc<dyn DatabaseAdapter>,
        provider_file_id: &str,
        hash: Option<&str>,
    ) -> TrackId {
        let track_id = TrackId::new();
        let now = chrono::Utc::now().timestamp();

        db.execute(
            r#"
            INSERT INTO tracks (
                id, provider_id, provider_file_id, title, normalized_title,
                duration_ms, format, file_size, created_at, updated_at, hash
            ) VALUES (?, ?, ?, ?, LOWER(TRIM(?)), ?, ?, ?, ?, ?, ?)
            "#,
            &[
                QueryValue::Text(track_id.to_string()),
                QueryValue::Text("test_provider".to_string()),
                QueryValue::Text(provider_file_id.to_string()),
                QueryValue::Text("Test Track".to_string()),
                QueryValue::Text("Test Track".to_string()),
                QueryValue::Integer(180000),
                QueryValue::Text("mp3".to_string()),
                QueryValue::Integer(5000000),
                QueryValue::Integer(now),
                QueryValue::Integer(now),
                hash.map(|h| QueryValue::Text(h.to_string()))
                    .unwrap_or(QueryValue::Null),
            ],
        )
        .await
        .unwrap();

        track_id
    }

    #[core_async::test]
    async fn test_deletion_tracking_soft() {
        let db = create_test_db().await;
        let resolver = Arc::new(ConflictResolver::new(
            db.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            db.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false, // soft delete
        );

        // Create tracks
        let _track1 = create_test_track(&db, "file_1", None).await;
        let _track2 = create_test_track(&db, "file_2", None).await;
        let _track3 = create_test_track(&db, "file_3", None).await;

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
        let rows = db
            .query(
                "SELECT COUNT(*) as count FROM tracks WHERE provider_file_id LIKE ?",
                &[QueryValue::Text("DELETED_%".to_string())],
            )
            .await
            .unwrap();

        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(count, 1);
    }

    #[core_async::test]
    async fn test_deletion_tracking_hard() {
        let db = create_test_db().await;
        let resolver = Arc::new(ConflictResolver::new(
            db.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            db.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            true, // hard delete
        );

        // Create tracks
        create_test_track(&db, "file_1", None).await;
        create_test_track(&db, "file_2", None).await;
        create_test_track(&db, "file_3", None).await;

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
        let rows = db
            .query(
                "SELECT COUNT(*) as count FROM tracks WHERE provider_id = ?",
                &[QueryValue::Text("test_provider".to_string())],
            )
            .await
            .unwrap();

        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(count, 1); // Only file_1 remains
    }

    #[core_async::test]
    async fn test_duplicate_resolution() {
        let db = create_test_db().await;
        let resolver = Arc::new(ConflictResolver::new(
            db.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            db.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false,
        );

        // Create duplicate tracks with same hash
        let hash = "duplicate_hash_123";
        create_test_track(&db, "file_1", Some(hash)).await;
        create_test_track(&db, "file_2", Some(hash)).await;
        create_test_track(&db, "file_3", Some(hash)).await;

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
        let rows = db
            .query(
                "SELECT COUNT(*) as count FROM tracks WHERE hash = ?",
                &[QueryValue::Text(hash.to_string())],
            )
            .await
            .unwrap();

        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(count, 1);
    }

    #[core_async::test]
    async fn test_no_conflicts() {
        let db = create_test_db().await;
        let resolver = Arc::new(ConflictResolver::new(
            db.clone(),
            ConflictPolicy::KeepNewest,
        ));
        let event_bus = EventBus::new(100);

        let orchestrator = ConflictResolutionOrchestrator::new(
            resolver,
            db.clone(),
            event_bus,
            ConflictPolicy::KeepNewest,
            false,
        );

        // Create tracks that all exist in provider
        create_test_track(&db, "file_1", None).await;
        create_test_track(&db, "file_2", None).await;

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
