//! Conflict Resolution for Sync Operations
//!
//! Handles file renames, duplicates, and deletions during synchronization.
//!
//! ## Overview
//!
//! When synchronizing music files from cloud providers, various conflicts can arise:
//! - **Duplicates**: Multiple files with the same content (detected by hash)
//! - **Renames**: Files moved or renamed (detected by tracking provider file IDs)
//! - **Deletions**: Files removed from the provider
//! - **Metadata Conflicts**: Same file with different metadata between local and remote
//!
//! ## Conflict Policies
//!
//! The resolver supports different strategies:
//! - **KeepNewest**: Keep the most recently modified version
//! - **KeepBoth**: Keep both versions (rename one to avoid collision)
//! - **UserPrompt**: Surface conflict to user for manual resolution (future)
//!
//! ## Usage
//!
//! ```no_run
//! use core_sync::conflict_resolver::{ConflictResolver, ConflictPolicy};
//! use bridge_traits::database::DatabaseAdapter;
//! use std::sync::Arc;
//!
//! # async fn example(db: Arc<dyn DatabaseAdapter>) -> Result<(), Box<dyn std::error::Error>> {
//! let resolver = ConflictResolver::new(db, ConflictPolicy::KeepNewest);
//!
//! // Detect duplicates by content hash
//! let duplicates = resolver.detect_duplicates().await?;
//! println!("Found {} duplicate files", duplicates.len());
//!
//! // Resolve a rename operation
//! resolver.resolve_rename(
//!     "old_provider_file_id",
//!     "new_provider_file_id",
//!     "new_file_name.mp3"
//! ).await?;
//!
//! // Handle a deletion
//! resolver.handle_deletion("provider_file_id", false).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, SyncError};
use bridge_traits::database::{DatabaseAdapter, QueryValue};
use core_library::models::{Track, TrackId};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

/// Conflict resolution policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Keep the most recently modified version
    #[default]
    KeepNewest,

    /// Keep both versions (rename one to avoid collision)
    KeepBoth,

    /// Prompt user for manual resolution (not yet implemented)
    UserPrompt,
}

/// Represents a set of duplicate tracks
#[derive(Debug, Clone)]
pub struct DuplicateSet {
    /// Content hash that identifies this duplicate set
    pub hash: String,

    /// List of track IDs that share this hash
    pub track_ids: Vec<TrackId>,

    /// Total size of duplicated data in bytes
    pub wasted_space: u64,
}

/// Metadata conflict between local and remote versions
#[derive(Debug, Clone)]
pub struct MetadataConflict {
    /// Track ID in local database
    pub track_id: TrackId,

    /// Local track metadata
    pub local: Track,

    /// Remote file modified timestamp
    pub remote_modified_at: i64,

    /// Fields that differ between local and remote
    pub conflicting_fields: Vec<String>,
}

/// Conflict resolution result
#[derive(Debug, Clone)]
pub enum ResolutionResult {
    /// Track was updated with new metadata
    Updated { track_id: TrackId },

    /// Track was marked as deleted (soft delete)
    Deleted { track_id: TrackId },

    /// Duplicate track was merged into primary
    Merged {
        primary_id: TrackId,
        duplicate_id: TrackId,
    },

    /// Track was renamed (provider file ID updated)
    Renamed {
        track_id: TrackId,
        old_provider_file_id: String,
        new_provider_file_id: String,
    },

    /// No action taken
    NoAction,
}

/// Conflict resolver for sync operations
pub struct ConflictResolver {
    db: Arc<dyn DatabaseAdapter>,
    policy: ConflictPolicy,
}

impl ConflictResolver {
    /// Create a new conflict resolver
    ///
    /// # Arguments
    ///
    /// * `db` - Database adapter
    /// * `policy` - Conflict resolution policy to use
    pub fn new(db: Arc<dyn DatabaseAdapter>, policy: ConflictPolicy) -> Self {
        Self { db, policy }
    }

    /// Detect duplicate files by content hash
    ///
    /// Finds all tracks that share the same content hash, indicating identical file content.
    /// This is useful for deduplication and space optimization.
    ///
    /// # Returns
    ///
    /// Returns a vector of `DuplicateSet` where each set contains tracks with identical content.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_sync::conflict_resolver::ConflictResolver;
    /// # async fn example(resolver: ConflictResolver) -> Result<(), Box<dyn std::error::Error>> {
    /// let duplicates = resolver.detect_duplicates().await?;
    /// for dup_set in duplicates {
    ///     println!("Hash: {}, {} duplicates, {} bytes wasted",
    ///              dup_set.hash, dup_set.track_ids.len(), dup_set.wasted_space);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn detect_duplicates(&self) -> Result<Vec<DuplicateSet>> {
        debug!("Detecting duplicate tracks by content hash");

        // Query for tracks grouped by hash with count > 1
        let rows = self
            .db
            .query(
                r#"
            SELECT hash, GROUP_CONCAT(id) as track_ids, file_size, COUNT(*) as count
            FROM tracks
            WHERE hash IS NOT NULL
            GROUP BY hash
            HAVING count > 1
            ORDER BY count DESC, file_size DESC
            "#,
                &[],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut duplicate_sets = Vec::new();

        for row in rows {
            let hash = row
                .get("hash")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing hash field".to_string()))?;
            let track_ids_str = row
                .get("track_ids")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing track_ids field".to_string()))?;
            let file_size = row
                .get("file_size")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| SyncError::Database("Missing file_size field".to_string()))?;
            let count = row
                .get("count")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| SyncError::Database("Missing count field".to_string()))?;

            // Parse track IDs
            let track_ids: Result<Vec<TrackId>> = track_ids_str
                .split(',')
                .map(|id| {
                    TrackId::from_string(id.trim()).map_err(|e| SyncError::InvalidInput {
                        field: "track_id".to_string(),
                        message: e.to_string(),
                    })
                })
                .collect();

            let track_ids = track_ids?;

            // Calculate wasted space (all duplicates except one)
            let wasted_space = if file_size > 0 {
                (file_size as u64) * ((count - 1) as u64)
            } else {
                0
            };

            duplicate_sets.push(DuplicateSet {
                hash,
                track_ids,
                wasted_space,
            });
        }

        info!("Found {} duplicate sets", duplicate_sets.len());
        Ok(duplicate_sets)
    }

    /// Resolve a file rename operation
    ///
    /// When a file is moved or renamed in the cloud provider, we need to update
    /// our local database to reflect the new provider file ID and name. This avoids
    /// treating the renamed file as a deletion + new file.
    ///
    /// # Arguments
    ///
    /// * `old_provider_file_id` - Previous provider file ID
    /// * `new_provider_file_id` - New provider file ID
    /// * `new_name` - New file name (optional)
    ///
    /// # Returns
    ///
    /// Returns `ResolutionResult::Renamed` if track was found and updated,
    /// or `ResolutionResult::NoAction` if no track with old ID exists.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_sync::conflict_resolver::ConflictResolver;
    /// # async fn example(resolver: ConflictResolver) -> Result<(), Box<dyn std::error::Error>> {
    /// resolver.resolve_rename(
    ///     "drive_file_123",
    ///     "drive_file_456",
    ///     "My Song (Remastered).mp3"
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn resolve_rename(
        &self,
        old_provider_file_id: &str,
        new_provider_file_id: &str,
        new_name: &str,
    ) -> Result<ResolutionResult> {
        debug!(
            "Resolving rename: {} -> {} ({})",
            old_provider_file_id, new_provider_file_id, new_name
        );

        // Find track with old provider file ID
        let rows = self
            .db
            .query(
                "SELECT id FROM tracks WHERE provider_file_id = ?",
                &[QueryValue::Text(old_provider_file_id.to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if rows.is_empty() {
            debug!(
                "No track found with provider_file_id: {}",
                old_provider_file_id
            );
            return Ok(ResolutionResult::NoAction);
        }

        let track_id_str = rows[0]
            .get("id")
            .and_then(|v| v.as_string())
            .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
        let track_id =
            TrackId::from_string(&track_id_str).map_err(|e| SyncError::InvalidInput {
                field: "track_id".to_string(),
                message: e.to_string(),
            })?;

        // Update provider file ID and title
        let now = chrono::Utc::now().timestamp();
        self.db
            .execute(
                r#"
            UPDATE tracks 
            SET provider_file_id = ?, 
                title = ?,
                normalized_title = LOWER(TRIM(?)),
                updated_at = ?
            WHERE id = ?
            "#,
                &[
                    QueryValue::Text(new_provider_file_id.to_string()),
                    QueryValue::Text(new_name.to_string()),
                    QueryValue::Text(new_name.to_string()),
                    QueryValue::Integer(now),
                    QueryValue::Text(track_id.to_string()),
                ],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        info!(
            "Renamed track {}: {} -> {}",
            track_id, old_provider_file_id, new_provider_file_id
        );

        Ok(ResolutionResult::Renamed {
            track_id,
            old_provider_file_id: old_provider_file_id.to_string(),
            new_provider_file_id: new_provider_file_id.to_string(),
        })
    }

    /// Handle a file deletion from the provider
    ///
    /// When a file is deleted from the cloud provider, we can either:
    /// - Soft delete: Mark the track as deleted but keep the metadata
    /// - Hard delete: Remove the track entirely from the database
    ///
    /// # Arguments
    ///
    /// * `provider_file_id` - Provider file ID of the deleted file
    /// * `hard_delete` - If true, removes track from database. If false, marks as deleted.
    ///
    /// # Returns
    ///
    /// Returns `ResolutionResult::Deleted` if track was found and deleted,
    /// or `ResolutionResult::NoAction` if no track with that ID exists.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_sync::conflict_resolver::ConflictResolver;
    /// # async fn example(resolver: ConflictResolver) -> Result<(), Box<dyn std::error::Error>> {
    /// // Soft delete (keeps metadata)
    /// resolver.handle_deletion("drive_file_123", false).await?;
    ///
    /// // Hard delete (removes from database)
    /// resolver.handle_deletion("drive_file_456", true).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn handle_deletion(
        &self,
        provider_file_id: &str,
        hard_delete: bool,
    ) -> Result<ResolutionResult> {
        debug!(
            "Handling deletion: {} (hard: {})",
            provider_file_id, hard_delete
        );

        // Find track with this provider file ID
        let rows = self
            .db
            .query(
                "SELECT id FROM tracks WHERE provider_file_id = ?",
                &[QueryValue::Text(provider_file_id.to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if rows.is_empty() {
            debug!("No track found with provider_file_id: {}", provider_file_id);
            return Ok(ResolutionResult::NoAction);
        }

        let track_id_str = rows[0]
            .get("id")
            .and_then(|v| v.as_string())
            .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
        let track_id =
            TrackId::from_string(&track_id_str).map_err(|e| SyncError::InvalidInput {
                field: "track_id".to_string(),
                message: e.to_string(),
            })?;

        if hard_delete {
            // Delete track from database
            self.db
                .execute(
                    "DELETE FROM tracks WHERE id = ?",
                    &[QueryValue::Text(track_id.to_string())],
                )
                .await
                .map_err(|e| SyncError::Database(e.to_string()))?;

            info!("Hard deleted track {}", track_id);
        } else {
            // Soft delete: mark as unavailable by setting a special marker in provider_file_id
            // Since provider_file_id is NOT NULL, we use a marker like "DELETED_<original_id>"
            let marker = format!("DELETED_{}", provider_file_id);
            self.db
                .execute(
                    "UPDATE tracks SET provider_file_id = ?, updated_at = ? WHERE id = ?",
                    &[
                        QueryValue::Text(marker),
                        QueryValue::Integer(chrono::Utc::now().timestamp()),
                        QueryValue::Text(track_id.to_string()),
                    ],
                )
                .await
                .map_err(|e| SyncError::Database(e.to_string()))?;

            info!(
                "Soft deleted track {} (marked provider_file_id as deleted)",
                track_id
            );
        }

        Ok(ResolutionResult::Deleted { track_id })
    }

    /// Merge metadata from remote file into local track
    ///
    /// When a track exists both locally and remotely with different metadata,
    /// this method intelligently merges the metadata based on the conflict policy.
    ///
    /// # Arguments
    ///
    /// * `track_id` - Local track ID
    /// * `remote_modified_at` - Remote file modification timestamp
    /// * `remote_metadata` - Metadata fields from remote file
    ///
    /// # Returns
    ///
    /// Returns `ResolutionResult::Updated` if metadata was merged,
    /// or `ResolutionResult::NoAction` if no changes were needed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_sync::conflict_resolver::ConflictResolver;
    /// # use core_library::models::TrackId;
    /// # use std::collections::HashMap;
    /// # async fn example(resolver: ConflictResolver, track_id: TrackId) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut metadata = HashMap::new();
    /// metadata.insert("title".to_string(), "New Title".to_string());
    /// metadata.insert("artist".to_string(), "New Artist".to_string());
    ///
    /// resolver.merge_metadata(track_id, 1699999999, metadata).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, remote_metadata))]
    pub async fn merge_metadata(
        &self,
        track_id: TrackId,
        remote_modified_at: i64,
        remote_metadata: HashMap<String, String>,
    ) -> Result<ResolutionResult> {
        debug!("Merging metadata for track {}", track_id);

        // Fetch current track
        let rows = self
            .db
            .query(
                "SELECT provider_modified_at, updated_at FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if rows.is_empty() {
            warn!("Track {} not found", track_id);
            return Ok(ResolutionResult::NoAction);
        }

        let local_provider_modified = rows[0].get("provider_modified_at").and_then(|v| v.as_i64());
        let _local_updated = rows[0]
            .get("updated_at")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| SyncError::Database("Missing updated_at field".to_string()))?;

        // Decide whether to update based on policy
        let should_update = match self.policy {
            ConflictPolicy::KeepNewest => {
                // Remote is newer if modified timestamp is greater
                if let Some(local_modified) = local_provider_modified {
                    remote_modified_at > local_modified
                } else {
                    // No local modified time, assume remote is newer
                    true
                }
            }
            ConflictPolicy::KeepBoth => {
                // For KeepBoth, we would create a duplicate entry (not implemented yet)
                warn!("KeepBoth policy not yet implemented for metadata merge");
                false
            }
            ConflictPolicy::UserPrompt => {
                // Would prompt user (not implemented yet)
                warn!("UserPrompt policy not yet implemented");
                false
            }
        };

        if !should_update {
            debug!("Keeping local metadata (policy: {:?})", self.policy);
            return Ok(ResolutionResult::NoAction);
        }

        // Build UPDATE query based on provided metadata fields
        let mut updates = Vec::new();
        let mut values: Vec<QueryValue> = Vec::new();

        for (key, value) in remote_metadata {
            match key.as_str() {
                "title" => {
                    updates.push("title = ?");
                    updates.push("normalized_title = LOWER(TRIM(?))");
                    values.push(QueryValue::Text(value.clone()));
                    values.push(QueryValue::Text(value));
                }
                "duration_ms" => {
                    updates.push("duration_ms = ?");
                    if let Ok(v) = value.parse::<i64>() {
                        values.push(QueryValue::Integer(v));
                    }
                }
                "bitrate" => {
                    updates.push("bitrate = ?");
                    if let Ok(v) = value.parse::<i64>() {
                        values.push(QueryValue::Integer(v));
                    }
                }
                "format" => {
                    updates.push("format = ?");
                    values.push(QueryValue::Text(value));
                }
                "year" => {
                    updates.push("year = ?");
                    if let Ok(v) = value.parse::<i64>() {
                        values.push(QueryValue::Integer(v));
                    }
                }
                _ => {
                    // Ignore unknown fields
                    debug!("Ignoring unknown metadata field: {}", key);
                }
            }
        }

        if updates.is_empty() {
            debug!("No metadata fields to update");
            return Ok(ResolutionResult::NoAction);
        }

        // Add standard fields
        updates.push("provider_modified_at = ?");
        updates.push("updated_at = ?");
        values.push(QueryValue::Integer(remote_modified_at));
        values.push(QueryValue::Integer(chrono::Utc::now().timestamp()));

        // Add track_id for WHERE clause
        values.push(QueryValue::Text(track_id.to_string()));

        // Build and execute query
        let query_str = format!("UPDATE tracks SET {} WHERE id = ?", updates.join(", "));

        self.db
            .execute(&query_str, &values)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        info!("Merged metadata for track {}", track_id);

        Ok(ResolutionResult::Updated { track_id })
    }

    /// Deduplicate tracks by merging duplicates into primary track
    ///
    /// For each duplicate set, keeps one "primary" track and removes the others.
    /// The primary track is chosen based on:
    /// - Highest audio quality (bitrate, format)
    /// - Most complete metadata
    /// - Most recent modification time
    ///
    /// # Arguments
    ///
    /// * `duplicate_set` - Set of duplicate tracks to deduplicate
    ///
    /// # Returns
    ///
    /// Returns a vector of `ResolutionResult::Merged` for each removed duplicate.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_sync::conflict_resolver::ConflictResolver;
    /// # async fn example(resolver: ConflictResolver) -> Result<(), Box<dyn std::error::Error>> {
    /// let duplicates = resolver.detect_duplicates().await?;
    /// for dup_set in duplicates {
    ///     let results = resolver.deduplicate(&dup_set).await?;
    ///     println!("Removed {} duplicate(s)", results.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn deduplicate(&self, duplicate_set: &DuplicateSet) -> Result<Vec<ResolutionResult>> {
        debug!(
            "Deduplicating {} tracks with hash {}",
            duplicate_set.track_ids.len(),
            duplicate_set.hash
        );

        if duplicate_set.track_ids.len() < 2 {
            return Ok(vec![]);
        }

        // Fetch all tracks in the duplicate set with quality metrics
        let track_ids_str: Vec<String> = duplicate_set
            .track_ids
            .iter()
            .map(|id| format!("'{}'", id))
            .collect();
        let query = format!(
            r#"
            SELECT id, bitrate, format, provider_modified_at, 
                   title, album_id, artist_id
            FROM tracks
            WHERE id IN ({})
            ORDER BY bitrate DESC, provider_modified_at DESC
            "#,
            track_ids_str.join(",")
        );

        let rows = self
            .db
            .query(&query, &[])
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        if rows.is_empty() {
            return Ok(vec![]);
        }

        // First row is the primary (highest quality, most recent)
        let primary_id_str = rows[0]
            .get("id")
            .and_then(|v| v.as_string())
            .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
        let primary_id =
            TrackId::from_string(&primary_id_str).map_err(|e| SyncError::InvalidInput {
                field: "track_id".to_string(),
                message: e.to_string(),
            })?;

        info!("Selected primary track: {}", primary_id);

        // Delete all other tracks in the set
        let mut results = Vec::new();
        for row in rows.iter().skip(1) {
            let dup_id_str = row
                .get("id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
            let dup_id =
                TrackId::from_string(&dup_id_str).map_err(|e| SyncError::InvalidInput {
                    field: "track_id".to_string(),
                    message: e.to_string(),
                })?;

            // TODO: Update playlist references to point to primary track
            // before deleting duplicate

            self.db
                .execute(
                    "DELETE FROM tracks WHERE id = ?",
                    &[QueryValue::Text(dup_id.to_string())],
                )
                .await
                .map_err(|e| SyncError::Database(e.to_string()))?;

            info!(
                "Merged duplicate track {} into primary {}",
                dup_id, primary_id
            );

            results.push(ResolutionResult::Merged {
                primary_id,
                duplicate_id: dup_id,
            });
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::database::DatabaseAdapter;
    use core_library::adapters::sqlite_native::SqliteAdapter;
    use core_library::db::DatabaseConfig;

    async fn create_test_db() -> Arc<dyn DatabaseAdapter> {
        let config = DatabaseConfig::in_memory();
        let pool = core_library::db::create_pool(config).await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(SqliteAdapter::from_pool(pool));

        // Create a test provider to satisfy foreign key constraints
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
        title: &str,
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
                QueryValue::Text(format!("file_{}", track_id)),
                QueryValue::Text(title.to_string()),
                QueryValue::Text(title.to_string()),
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
    async fn test_detect_duplicates() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        // Create tracks with same hash
        let hash = "abc123def456";
        let _track1 = create_test_track(&db, "Song 1", Some(hash)).await;
        let _track2 = create_test_track(&db, "Song 2", Some(hash)).await;
        let _track3 = create_test_track(&db, "Song 3", Some(hash)).await;

        // Create track with different hash
        let _track4 = create_test_track(&db, "Song 4", Some("different_hash")).await;

        let duplicates = resolver.detect_duplicates().await.unwrap();

        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].hash, hash);
        assert_eq!(duplicates[0].track_ids.len(), 3);
    }

    #[core_async::test]
    async fn test_resolve_rename() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        // Create a track
        let track_id = TrackId::new();
        let now = chrono::Utc::now().timestamp();
        let old_provider_id = "old_file_123";

        db.execute(
            r#"
            INSERT INTO tracks (
                id, provider_id, provider_file_id, title, normalized_title,
                duration_ms, format, file_size, created_at, updated_at
            ) VALUES (?, ?, ?, ?, LOWER(TRIM(?)), ?, ?, ?, ?, ?)
            "#,
            &[
                QueryValue::Text(track_id.to_string()),
                QueryValue::Text("test_provider".to_string()),
                QueryValue::Text(old_provider_id.to_string()),
                QueryValue::Text("Old Title".to_string()),
                QueryValue::Text("Old Title".to_string()),
                QueryValue::Integer(180000),
                QueryValue::Text("mp3".to_string()),
                QueryValue::Integer(5000000),
                QueryValue::Integer(now),
                QueryValue::Integer(now),
            ],
        )
        .await
        .unwrap();

        // Resolve rename
        let result = resolver
            .resolve_rename(old_provider_id, "new_file_456", "New Title")
            .await
            .unwrap();

        match result {
            ResolutionResult::Renamed {
                track_id: renamed_id,
                ..
            } => {
                assert_eq!(renamed_id, track_id);
            }
            _ => panic!("Expected Renamed result"),
        }

        // Verify database was updated
        let rows = db
            .query(
                "SELECT provider_file_id, title FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .unwrap();

        let new_provider_id = rows[0]
            .get("provider_file_id")
            .and_then(|v| v.as_string())
            .unwrap();
        let new_title = rows[0].get("title").and_then(|v| v.as_string()).unwrap();

        assert_eq!(new_provider_id, "new_file_456");
        assert_eq!(new_title, "New Title");
    }

    #[core_async::test]
    async fn test_handle_deletion_soft() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        let track_id = create_test_track(&db, "Test Track", None).await;
        let provider_file_id = format!("file_{}", track_id);

        // Soft delete
        let result = resolver
            .handle_deletion(&provider_file_id, false)
            .await
            .unwrap();

        match result {
            ResolutionResult::Deleted {
                track_id: deleted_id,
                ..
            } => {
                assert_eq!(deleted_id, track_id);
            }
            _ => panic!("Expected Deleted result"),
        }

        // Verify track still exists but provider_file_id is marked as DELETED
        let rows = db
            .query(
                "SELECT provider_file_id FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .unwrap();

        let provider_file_id_value = rows[0]
            .get("provider_file_id")
            .and_then(|v| v.as_string())
            .unwrap();
        assert!(provider_file_id_value.starts_with("DELETED_"));
    }

    #[core_async::test]
    async fn test_handle_deletion_hard() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        let track_id = create_test_track(&db, "Test Track", None).await;
        let provider_file_id = format!("file_{}", track_id);

        // Hard delete
        let result = resolver
            .handle_deletion(&provider_file_id, true)
            .await
            .unwrap();

        match result {
            ResolutionResult::Deleted {
                track_id: deleted_id,
                ..
            } => {
                assert_eq!(deleted_id, track_id);
            }
            _ => panic!("Expected Deleted result"),
        }

        // Verify track no longer exists
        let rows = db
            .query(
                "SELECT COUNT(*) as count FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .unwrap();

        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(count, 0);
    }

    #[core_async::test]
    async fn test_merge_metadata() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        let track_id = create_test_track(&db, "Old Title", None).await;

        // Update with modified timestamp from 10 seconds ago
        let old_timestamp = chrono::Utc::now().timestamp() - 10;
        db.execute(
            "UPDATE tracks SET provider_modified_at = ? WHERE id = ?",
            &[
                QueryValue::Integer(old_timestamp),
                QueryValue::Text(track_id.to_string()),
            ],
        )
        .await
        .unwrap();

        // Merge metadata with newer timestamp
        let new_timestamp = chrono::Utc::now().timestamp();
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "New Title".to_string());
        metadata.insert("year".to_string(), "2024".to_string());

        let result = resolver
            .merge_metadata(track_id, new_timestamp, metadata)
            .await
            .unwrap();

        match result {
            ResolutionResult::Updated {
                track_id: updated_id,
            } => {
                assert_eq!(updated_id, track_id);
            }
            _ => panic!("Expected Updated result"),
        }

        // Verify metadata was updated
        let rows = db
            .query(
                "SELECT title, year FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .unwrap();

        let title = rows[0].get("title").and_then(|v| v.as_string()).unwrap();
        let year = rows[0].get("year").and_then(|v| v.as_i64());

        assert_eq!(title, "New Title");
        assert_eq!(year, Some(2024));
    }

    #[core_async::test]
    async fn test_deduplicate() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        // Create duplicate tracks with same hash but different quality
        let hash = "duplicate_hash_123";

        // Low quality
        let track1 = create_test_track(&db, "Song - Low", Some(hash)).await;
        db.execute(
            "UPDATE tracks SET bitrate = 128000 WHERE id = ?",
            &[QueryValue::Text(track1.to_string())],
        )
        .await
        .unwrap();

        // High quality (should be kept)
        let track2 = create_test_track(&db, "Song - High", Some(hash)).await;
        db.execute(
            "UPDATE tracks SET bitrate = 320000 WHERE id = ?",
            &[QueryValue::Text(track2.to_string())],
        )
        .await
        .unwrap();

        // Medium quality
        let track3 = create_test_track(&db, "Song - Medium", Some(hash)).await;
        db.execute(
            "UPDATE tracks SET bitrate = 192000 WHERE id = ?",
            &[QueryValue::Text(track3.to_string())],
        )
        .await
        .unwrap();

        let duplicate_set = DuplicateSet {
            hash: hash.to_string(),
            track_ids: vec![track1, track2, track3],
            wasted_space: 10000000,
        };

        let results = resolver.deduplicate(&duplicate_set).await.unwrap();

        // Should merge 2 duplicates into the primary (highest quality)
        assert_eq!(results.len(), 2);

        // Verify only track2 (highest quality) still exists
        let rows = db
            .query(
                "SELECT COUNT(*) as count FROM tracks WHERE hash = ?",
                &[QueryValue::Text(hash.to_string())],
            )
            .await
            .unwrap();

        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(count, 1);

        // Verify it's the high quality track
        let rows = db
            .query(
                "SELECT bitrate FROM tracks WHERE hash = ?",
                &[QueryValue::Text(hash.to_string())],
            )
            .await
            .unwrap();

        let bitrate = rows[0].get("bitrate").and_then(|v| v.as_i64());
        assert_eq!(bitrate, Some(320000));
    }

    #[core_async::test]
    async fn test_conflict_policy_keep_newest() {
        let db = create_test_db().await;
        let resolver = ConflictResolver::new(db.clone(), ConflictPolicy::KeepNewest);

        let track_id = create_test_track(&db, "Title", None).await;

        // Set old modification time
        let old_time = chrono::Utc::now().timestamp() - 100;
        db.execute(
            "UPDATE tracks SET provider_modified_at = ? WHERE id = ?",
            &[
                QueryValue::Integer(old_time),
                QueryValue::Text(track_id.to_string()),
            ],
        )
        .await
        .unwrap();

        // Try to merge with older metadata (should not update)
        let older_time = old_time - 50;
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "Should Not Update".to_string());

        let result = resolver
            .merge_metadata(track_id, older_time, metadata)
            .await
            .unwrap();
        assert!(matches!(result, ResolutionResult::NoAction));

        // Try to merge with newer metadata (should update)
        let newer_time = old_time + 50;
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "Should Update".to_string());

        let result = resolver
            .merge_metadata(track_id, newer_time, metadata)
            .await
            .unwrap();
        assert!(matches!(result, ResolutionResult::Updated { .. }));

        // Verify the title was updated
        let rows = db
            .query(
                "SELECT title FROM tracks WHERE id = ?",
                &[QueryValue::Text(track_id.to_string())],
            )
            .await
            .unwrap();

        let title = rows[0].get("title").and_then(|v| v.as_string()).unwrap();
        assert_eq!(title, "Should Update");
    }
}
