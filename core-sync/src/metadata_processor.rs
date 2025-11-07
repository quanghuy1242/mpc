//! Metadata Processing Module
//!
//! Handles the extraction and persistence of metadata for audio files during sync operations.
//! This module bridges the gap between the `SyncCoordinator` and the `MetadataExtractor`.
//!
//! ## Overview
//!
//! The `MetadataProcessor` is responsible for:
//! - Downloading audio files from cloud storage providers
//! - Extracting metadata using the `MetadataExtractor`
//! - Creating or updating database entities (Track, Artist, Album)
//! - Handling embedded artwork via `ArtworkService`
//! - Managing temporary file lifecycle
//! - Coordinating transaction boundaries
//!
//! ## Workflow
//!
//! 1. Download file from provider (or just the audio header for quick extraction)
//! 2. Save to temporary location via `FileSystemAccess`
//! 3. Extract metadata using `MetadataExtractor`
//! 4. Resolve or create Artist and Album entities
//! 5. Create or update Track entity
//! 6. Extract and store embedded artwork if present
//! 7. Clean up temporary files
//! 8. Return processing result with statistics
//!
//! ## Error Handling
//!
//! The processor implements graceful degradation:
//! - Partial metadata extraction is allowed (e.g., missing artist)
//! - Artwork extraction failures don't block track persistence
//! - Network errors are retried according to policy
//! - Corrupted files are logged but don't fail the entire sync

use crate::error::{Result, SyncError};
use crate::scan_queue::WorkItem;
use bridge_traits::database::DatabaseAdapter;
use bridge_traits::storage::{FileSystemAccess, StorageProvider};
use bridge_traits::time::{Clock, SystemClock};
use bytes::Bytes;
use core_library::models::{Album, AlbumId, Artist, ArtistId, Track, TrackId};
use core_library::repositories::{
    AlbumRepository, ArtistRepository, ArtworkRepository, TrackRepository,
};
use core_metadata::artwork::ArtworkService;
use core_metadata::extractor::{ExtractedMetadata, MetadataExtractor};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Result of processing a single work item
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    /// Whether the track was newly added (true) or updated (false)
    pub is_new: bool,
    /// Track ID that was created or updated
    pub track_id: String,
    /// Whether artwork was extracted and stored
    pub artwork_processed: bool,
    /// Number of bytes downloaded
    pub bytes_downloaded: u64,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Configuration for metadata processing
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Whether to download full files or just headers for metadata extraction
    /// Header-only mode downloads approximately the first 256KB of the file
    pub header_only: bool,

    /// Maximum bytes to download for header-only mode
    pub header_size_bytes: u64,

    /// Whether to extract and store embedded artwork
    pub extract_artwork: bool,

    /// Whether to update existing tracks or skip them
    pub update_existing: bool,

    /// Maximum retries for download operations
    pub max_download_retries: u32,

    /// Timeout for download operations (seconds)
    pub download_timeout_secs: u64,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            header_only: true,
            header_size_bytes: 256 * 1024, // 256KB
            extract_artwork: true,
            update_existing: false,
            max_download_retries: 3,
            download_timeout_secs: 300, // 5 minutes
        }
    }
}

/// Metadata processor for audio files during sync
pub struct MetadataProcessor {
    config: ProcessorConfig,
    metadata_extractor: Arc<MetadataExtractor>,
    file_system: Arc<dyn FileSystemAccess>,
    track_repository: Arc<dyn TrackRepository>,
    #[allow(dead_code)]
    artist_repository: Arc<dyn ArtistRepository>,
    #[allow(dead_code)]
    album_repository: Arc<dyn AlbumRepository>,
    #[allow(dead_code)]
    artwork_repository: Arc<dyn ArtworkRepository>,
    artwork_service: Option<Arc<ArtworkService>>,
    db: Arc<dyn DatabaseAdapter>,
    clock: Arc<dyn Clock>,
}

impl MetadataProcessor {
    /// Create a new metadata processor
    ///
    /// # Arguments
    ///
    /// * `config` - Processing configuration
    /// * `file_system` - File system access bridge
    /// * `track_repository` - Track data access layer
    /// * `artist_repository` - Artist data access layer
    /// * `album_repository` - Album data access layer
    /// * `artwork_repository` - Artwork data access layer
    /// * `artwork_service` - Optional artwork processing service
    /// * `db` - Database adapter for transactions
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ProcessorConfig,
        file_system: Arc<dyn FileSystemAccess>,
        track_repository: Arc<dyn TrackRepository>,
        artist_repository: Arc<dyn ArtistRepository>,
        album_repository: Arc<dyn AlbumRepository>,
        artwork_repository: Arc<dyn ArtworkRepository>,
        artwork_service: Option<Arc<ArtworkService>>,
        db: Arc<dyn DatabaseAdapter>,
    ) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Self::with_clock(
            config,
            file_system,
            track_repository,
            artist_repository,
            album_repository,
            artwork_repository,
            artwork_service,
            db,
            clock,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_clock(
        config: ProcessorConfig,
        file_system: Arc<dyn FileSystemAccess>,
        track_repository: Arc<dyn TrackRepository>,
        artist_repository: Arc<dyn ArtistRepository>,
        album_repository: Arc<dyn AlbumRepository>,
        artwork_repository: Arc<dyn ArtworkRepository>,
        artwork_service: Option<Arc<ArtworkService>>,
        db: Arc<dyn DatabaseAdapter>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let metadata_extractor = Arc::new(MetadataExtractor::new());

        Self {
            config,
            metadata_extractor,
            file_system,
            track_repository,
            artist_repository,
            album_repository,
            artwork_repository,
            artwork_service,
            db,
            clock,
        }
    }

    fn elapsed_since(&self, start_ms: i64) -> u64 {
        let now = self.clock.unix_timestamp_millis();
        let diff = now - start_ms;
        if diff < 0 {
            0
        } else {
            diff as u64
        }
    }

    /// Process a single work item
    ///
    /// Downloads the file (or header), extracts metadata, and persists to the database.
    ///
    /// # Arguments
    ///
    /// * `work_item` - The work item to process
    /// * `provider` - Storage provider to download from
    /// * `provider_id` - The provider ID (profile ID) for this track
    /// * `remote_file` - The remote file metadata containing name and path
    ///
    /// # Returns
    ///
    /// Returns a `ProcessingResult` with statistics about the operation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Download fails after retries
    /// - File system operations fail
    /// - Metadata extraction fails completely
    /// - Database operations fail
    pub async fn process_work_item(
        &self,
        work_item: &WorkItem,
        provider: &Arc<dyn StorageProvider>,
        provider_id: &str,
        file_name: &str,
    ) -> Result<ProcessingResult> {
        let start_time = self.clock.unix_timestamp_millis();

        debug!(
            "Processing work item: {} ({})",
            work_item.remote_file_id, file_name
        );

        // Step 1: Download file to temporary location
        let (temp_path, bytes_downloaded) = self
            .download_file(work_item, provider, file_name)
            .await
            .map_err(|e| {
                error!(
                    "Failed to download file {}: {}",
                    work_item.remote_file_id, e
                );
                e
            })?;

        // Step 2: Extract metadata
        let metadata = match self.extract_metadata(&temp_path).await {
            Ok(meta) => meta,
            Err(e) => {
                // Clean up temp file on error
                self.cleanup_temp_file(&temp_path).await;
                error!("Failed to extract metadata from {}: {}", file_name, e);
                return Err(e);
            }
        };

        // Step 3: Check if track already exists
        let existing_track = self
            .track_repository
            .find_by_provider_file(provider_id, &work_item.remote_file_id)
            .await?;

        let is_new = existing_track.is_none();

        // Skip if track exists and update_existing is false
        if !is_new && !self.config.update_existing {
            debug!("Track already exists, skipping: {}", file_name);
            self.cleanup_temp_file(&temp_path).await;
            return Ok(ProcessingResult {
                is_new: false,
                track_id: existing_track.unwrap().id,
                artwork_processed: false,
                bytes_downloaded,
                processing_time_ms: self.elapsed_since(start_time),
            });
        }

        // Step 4: Begin database transaction for atomicity
        let tx_id = self
            .db
            .begin_transaction()
            .await
            .map_err(|e| SyncError::Internal(format!("Failed to begin transaction: {}", e)))?;

        // Step 5: Resolve or create artist
        let artist_id = self
            .resolve_or_create_artist(&metadata, tx_id)
            .await
            .map_err(|e| {
                error!("Failed to resolve artist: {}", e);
                e
            })?;

        // Step 6: Resolve or create album
        let album_id = self
            .resolve_or_create_album(&metadata, artist_id.as_ref(), tx_id)
            .await
            .map_err(|e| {
                error!("Failed to resolve album: {}", e);
                e
            })?;

        // Step 7: Process embedded artwork if configured
        let mut artwork_processed = false;
        let artwork_id = if self.config.extract_artwork && !metadata.artwork.is_empty() {
            match self.process_artwork(&metadata).await {
                Ok(Some(id)) => {
                    artwork_processed = true;
                    Some(id)
                }
                Ok(None) => None,
                Err(e) => {
                    warn!("Failed to process artwork for {}: {}", file_name, e);
                    None
                }
            }
        } else {
            None
        };

        // Step 7: Create or update track
        let track_id = if is_new {
            self.create_track(
                work_item,
                &metadata,
                provider_id,
                artist_id,
                album_id,
                artwork_id,
                tx_id,
                file_name,
            )
            .await?
        } else {
            self.update_track(
                &existing_track.unwrap(),
                &metadata,
                artist_id,
                album_id,
                artwork_id,
                tx_id,
            )
            .await?
        };

        // Step 9: Commit transaction
        self.db
            .commit_transaction(tx_id)
            .await
            .map_err(|e| SyncError::Internal(format!("Failed to commit transaction: {}", e)))?;

        // Step 10: Clean up temporary file
        self.cleanup_temp_file(&temp_path).await;

        let processing_time_ms = self.elapsed_since(start_time);

        info!(
            "Successfully processed {} in {}ms (new: {}, artwork: {})",
            file_name, processing_time_ms, is_new, artwork_processed
        );

        Ok(ProcessingResult {
            is_new,
            track_id,
            artwork_processed,
            bytes_downloaded,
            processing_time_ms,
        })
    }

    /// Download file from provider to temporary location
    async fn download_file(
        &self,
        work_item: &WorkItem,
        provider: &Arc<dyn StorageProvider>,
        file_name: &str,
    ) -> Result<(PathBuf, u64)> {
        let cache_dir =
            self.file_system.get_cache_directory().await.map_err(|e| {
                SyncError::Provider(format!("Failed to get cache directory: {}", e))
            })?;

        // Create temp directory if it doesn't exist
        let temp_dir = cache_dir.join("sync_temp");
        self.file_system
            .create_dir_all(&temp_dir)
            .await
            .map_err(|e| SyncError::Provider(format!("Failed to create temp directory: {}", e)))?;

        // Generate temporary file path using file name
        let temp_path = temp_dir.join(format!("{}_{}", work_item.id, file_name));

        // Determine download range
        let range = if self.config.header_only {
            Some(format!("bytes=0-{}", self.config.header_size_bytes - 1))
        } else {
            None
        };

        // Download file with retries
        let mut attempt = 0;
        let data = loop {
            attempt += 1;
            match self
                .download_with_timeout(provider, &work_item.remote_file_id, range.as_deref())
                .await
            {
                Ok(data) => break data,
                Err(e) if attempt < self.config.max_download_retries => {
                    warn!(
                        "Download attempt {} failed for {}: {}. Retrying...",
                        attempt, work_item.remote_file_id, e
                    );
                    core_async::time::sleep(core_async::time::Duration::from_secs(
                        2u64.pow(attempt - 1),
                    ))
                    .await;
                }
                Err(e) => {
                    return Err(SyncError::Provider(format!(
                        "Failed to download {} after {} attempts: {}",
                        work_item.remote_file_id, attempt, e
                    )));
                }
            }
        };

        let bytes_downloaded = data.len() as u64;

        // Write to temporary file
        self.file_system
            .write_file(&temp_path, data)
            .await
            .map_err(|e| SyncError::Provider(format!("Failed to write temporary file: {}", e)))?;

        debug!("Downloaded {} bytes to {:?}", bytes_downloaded, temp_path);

        Ok((temp_path, bytes_downloaded))
    }

    /// Download with timeout
    async fn download_with_timeout(
        &self,
        provider: &Arc<dyn StorageProvider>,
        file_id: &str,
        range: Option<&str>,
    ) -> Result<Bytes> {
        let timeout_duration =
            core_async::time::Duration::from_secs(self.config.download_timeout_secs);

        core_async::time::timeout(timeout_duration, provider.download(file_id, range))
            .await
            .map_err(|_| SyncError::Timeout(self.config.download_timeout_secs))?
            .map_err(|e| SyncError::Provider(format!("Download failed: {}", e)))
    }

    /// Extract metadata from file
    async fn extract_metadata(&self, path: &Path) -> Result<ExtractedMetadata> {
        let file_data = self
            .file_system
            .read_file(path)
            .await
            .map_err(|e| SyncError::Provider(format!("Failed to read temp file: {}", e)))?;

        self.metadata_extractor
            .extract_from_bytes(file_data.as_ref(), path)
            .await
            .map_err(|e| SyncError::Internal(format!("Metadata extraction failed: {}", e)))
    }

    /// Resolve or create artist entity
    async fn resolve_or_create_artist(
        &self,
        metadata: &ExtractedMetadata,
        tx_id: bridge_traits::database::TransactionId,
    ) -> Result<Option<ArtistId>> {
        let artist_name = match &metadata.artist {
            Some(name) if !name.trim().is_empty() => name.trim(),
            _ => return Ok(None),
        };

        // Try to find existing artist by normalized name
        let normalized_name = normalize_name(artist_name);

        // Query within transaction
        let rows = self
            .db
            .query_in_transaction(
                tx_id,
                "SELECT id FROM artists WHERE normalized_name = ?",
                &[bridge_traits::database::QueryValue::Text(
                    normalized_name.clone(),
                )],
            )
            .await
            .map_err(|e| SyncError::Internal(format!("Failed to query artist: {}", e)))?;

        if !rows.is_empty() {
            let id = rows[0]
                .get("id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
            let artist_id = ArtistId::from_string(&id)
                .map_err(|e| SyncError::Internal(format!("Invalid artist ID: {}", e)))?;
            return Ok(Some(artist_id));
        }

        // Create new artist
        let artist = Artist {
            id: ArtistId::new().to_string(),
            name: artist_name.to_string(),
            normalized_name: normalized_name.clone(),
            sort_name: None,
            bio: None,
            country: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.db.execute_in_transaction(
            tx_id,
            "INSERT INTO artists (id, name, normalized_name, sort_name, bio, country, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                bridge_traits::database::QueryValue::Text(artist.id.clone()),
                bridge_traits::database::QueryValue::Text(artist.name.clone()),
                bridge_traits::database::QueryValue::Text(artist.normalized_name),
                bridge_traits::database::QueryValue::Null,
                bridge_traits::database::QueryValue::Null,
                bridge_traits::database::QueryValue::Null,
                bridge_traits::database::QueryValue::Integer(artist.created_at),
                bridge_traits::database::QueryValue::Integer(artist.updated_at),
            ],
        )
        .await
        .map_err(|e| SyncError::Internal(format!("Failed to insert artist: {}", e)))?;

        debug!("Created new artist: {} ({})", artist.name, artist.id);

        Ok(Some(ArtistId::from_string(&artist.id).map_err(|e| {
            SyncError::Internal(format!("Invalid artist ID: {}", e))
        })?))
    }

    /// Resolve or create album entity
    async fn resolve_or_create_album(
        &self,
        metadata: &ExtractedMetadata,
        artist_id: Option<&ArtistId>,
        tx_id: bridge_traits::database::TransactionId,
    ) -> Result<Option<AlbumId>> {
        let album_name = match &metadata.album {
            Some(name) if !name.trim().is_empty() => name.trim(),
            _ => return Ok(None),
        };

        let normalized_name = normalize_name(album_name);
        let artist_id_str = artist_id.map(|id| id.to_string());

        // Try to find existing album by normalized name and artist
        let rows = if let Some(ref aid) = artist_id_str {
            self.db
                .query_in_transaction(
                    tx_id,
                    "SELECT id FROM albums WHERE normalized_name = ? AND artist_id = ?",
                    &[
                        bridge_traits::database::QueryValue::Text(normalized_name.clone()),
                        bridge_traits::database::QueryValue::Text(aid.clone()),
                    ],
                )
                .await
        } else {
            self.db
                .query_in_transaction(
                    tx_id,
                    "SELECT id FROM albums WHERE normalized_name = ? AND artist_id IS NULL",
                    &[bridge_traits::database::QueryValue::Text(
                        normalized_name.clone(),
                    )],
                )
                .await
        }
        .map_err(|e| SyncError::Internal(format!("Failed to query album: {}", e)))?;

        if !rows.is_empty() {
            let id = rows[0]
                .get("id")
                .and_then(|v| v.as_string())
                .ok_or_else(|| SyncError::Database("Missing id field".to_string()))?;
            let album_id = AlbumId::from_string(&id)
                .map_err(|e| SyncError::Internal(format!("Invalid album ID: {}", e)))?;
            return Ok(Some(album_id));
        }

        // Create new album
        let album = Album {
            id: AlbumId::new().to_string(),
            name: album_name.to_string(),
            normalized_name: normalized_name.clone(),
            artist_id: artist_id_str.clone(),
            year: metadata.year,
            genre: metadata.genre.clone(),
            artwork_id: None,
            track_count: 0,
            total_duration_ms: 0,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.db.execute_in_transaction(
            tx_id,
            "INSERT INTO albums (id, name, normalized_name, artist_id, year, genre, artwork_id, track_count, total_duration_ms, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                bridge_traits::database::QueryValue::Text(album.id.clone()),
                bridge_traits::database::QueryValue::Text(album.name.clone()),
                bridge_traits::database::QueryValue::Text(album.normalized_name),
                album.artist_id.as_ref().map(|s| bridge_traits::database::QueryValue::Text(s.clone())).unwrap_or(bridge_traits::database::QueryValue::Null),
                album.year.map(|y| bridge_traits::database::QueryValue::Integer(y as i64)).unwrap_or(bridge_traits::database::QueryValue::Null),
                album.genre.as_ref().map(|g| bridge_traits::database::QueryValue::Text(g.clone())).unwrap_or(bridge_traits::database::QueryValue::Null),
                bridge_traits::database::QueryValue::Null,
                bridge_traits::database::QueryValue::Integer(album.track_count as i64),
                bridge_traits::database::QueryValue::Integer(album.total_duration_ms as i64),
                bridge_traits::database::QueryValue::Integer(album.created_at),
                bridge_traits::database::QueryValue::Integer(album.updated_at),
            ],
        )
        .await
        .map_err(|e| SyncError::Internal(format!("Failed to insert album: {}", e)))?;

        debug!("Created new album: {} ({})", album.name, album.id);

        Ok(Some(AlbumId::from_string(&album.id).map_err(|e| {
            SyncError::Internal(format!("Invalid album ID: {}", e))
        })?))
    }

    /// Process embedded artwork
    async fn process_artwork(&self, metadata: &ExtractedMetadata) -> Result<Option<String>> {
        if metadata.artwork.is_empty() {
            return Ok(None);
        }

        let artwork_service = match &self.artwork_service {
            Some(svc) => svc,
            None => return Ok(None),
        };

        // Extract embedded artwork
        match artwork_service
            .extract_embedded(metadata.artwork.clone())
            .await
        {
            Ok(ids) if !ids.is_empty() => Ok(Some(ids[0].id.clone())),
            Ok(_) => Ok(None),
            Err(e) => Err(SyncError::Internal(format!(
                "Failed to extract artwork: {}",
                e
            ))),
        }
    }

    /// Create new track entity
    #[allow(clippy::too_many_arguments)]
    async fn create_track(
        &self,
        work_item: &WorkItem,
        metadata: &ExtractedMetadata,
        provider_id: &str,
        artist_id: Option<ArtistId>,
        album_id: Option<AlbumId>,
        artwork_id: Option<String>,
        tx_id: bridge_traits::database::TransactionId,
        file_name: &str,
    ) -> Result<String> {
        let track_id = TrackId::new().to_string();
        let title = metadata.title.clone().unwrap_or_else(|| {
            Path::new(file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

        let normalized_title = normalize_name(&title);
        let now = chrono::Utc::now().timestamp();

        self.db
            .execute_in_transaction(
                tx_id,
                r#"
            INSERT INTO tracks (
                id, provider_id, provider_file_id, hash,
                title, normalized_title, album_id, artist_id, album_artist_id,
                track_number, disc_number, duration_ms, bitrate, sample_rate,
                channels, format, mime_type, file_size, artwork_id, lyrics_status,
                year, genre, composer, comment, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
                &[
                    bridge_traits::database::QueryValue::Text(track_id.clone()),
                    bridge_traits::database::QueryValue::Text(provider_id.to_string()),
                    bridge_traits::database::QueryValue::Text(work_item.remote_file_id.clone()),
                    bridge_traits::database::QueryValue::Text(metadata.content_hash.clone()),
                    bridge_traits::database::QueryValue::Text(title.clone()),
                    bridge_traits::database::QueryValue::Text(normalized_title),
                    album_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |id| {
                            bridge_traits::database::QueryValue::Text(id.to_string())
                        }),
                    artist_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |id| {
                            bridge_traits::database::QueryValue::Text(id.to_string())
                        }),
                    artist_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |id| {
                            bridge_traits::database::QueryValue::Text(id.to_string())
                        }),
                    metadata
                        .track_number
                        .map_or(bridge_traits::database::QueryValue::Null, |n| {
                            bridge_traits::database::QueryValue::Integer(n as i64)
                        }),
                    metadata
                        .disc_number
                        .map_or(bridge_traits::database::QueryValue::Null, |n| {
                            bridge_traits::database::QueryValue::Integer(n as i64)
                        }),
                    bridge_traits::database::QueryValue::Integer(metadata.duration_ms as i64),
                    metadata
                        .bitrate
                        .map_or(bridge_traits::database::QueryValue::Null, |b| {
                            bridge_traits::database::QueryValue::Integer(b as i64)
                        }),
                    metadata
                        .sample_rate
                        .map_or(bridge_traits::database::QueryValue::Null, |sr| {
                            bridge_traits::database::QueryValue::Integer(sr as i64)
                        }),
                    metadata
                        .channels
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Integer(c as i64)
                        }),
                    bridge_traits::database::QueryValue::Text(metadata.format.clone()),
                    bridge_traits::database::QueryValue::Text(metadata.mime_type.clone()),
                    bridge_traits::database::QueryValue::Integer(metadata.file_size as i64),
                    artwork_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |aid| {
                            bridge_traits::database::QueryValue::Text(aid.clone())
                        }),
                    bridge_traits::database::QueryValue::Text("not_fetched".to_string()),
                    metadata
                        .year
                        .map_or(bridge_traits::database::QueryValue::Null, |y| {
                            bridge_traits::database::QueryValue::Integer(y as i64)
                        }),
                    metadata
                        .genre
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |g| {
                            bridge_traits::database::QueryValue::Text(g.clone())
                        }),
                    metadata
                        .composer
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Text(c.clone())
                        }),
                    metadata
                        .comment
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Text(c.clone())
                        }),
                    bridge_traits::database::QueryValue::Integer(now),
                    bridge_traits::database::QueryValue::Integer(now),
                ],
            )
            .await
            .map_err(|e| SyncError::Internal(format!("Failed to insert track: {}", e)))?;

        debug!("Created new track: {} ({})", title, track_id);

        Ok(track_id)
    }

    /// Update existing track entity
    #[allow(clippy::too_many_arguments)]
    async fn update_track(
        &self,
        existing_track: &Track,
        metadata: &ExtractedMetadata,
        artist_id: Option<ArtistId>,
        album_id: Option<AlbumId>,
        artwork_id: Option<String>,
        tx_id: bridge_traits::database::TransactionId,
    ) -> Result<String> {
        let title = metadata
            .title
            .clone()
            .unwrap_or_else(|| existing_track.title.clone());
        let normalized_title = normalize_name(&title);

        // Use COALESCE for artwork_id to preserve existing value if new one is None
        self.db
            .execute_in_transaction(
                tx_id,
                r#"
            UPDATE tracks SET
                hash = ?, title = ?, normalized_title = ?, album_id = ?, artist_id = ?,
                track_number = ?, disc_number = ?, duration_ms = ?, bitrate = ?,
                sample_rate = ?, channels = ?, format = ?, mime_type = ?, file_size = ?,
                artwork_id = COALESCE(?, artwork_id), year = ?, genre = ?,
                composer = ?, comment = ?, updated_at = ?
            WHERE id = ?
            "#,
                &[
                    bridge_traits::database::QueryValue::Text(metadata.content_hash.clone()),
                    bridge_traits::database::QueryValue::Text(title.clone()),
                    bridge_traits::database::QueryValue::Text(normalized_title),
                    album_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |id| {
                            bridge_traits::database::QueryValue::Text(id.to_string())
                        }),
                    artist_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |id| {
                            bridge_traits::database::QueryValue::Text(id.to_string())
                        }),
                    metadata
                        .track_number
                        .map_or(bridge_traits::database::QueryValue::Null, |n| {
                            bridge_traits::database::QueryValue::Integer(n as i64)
                        }),
                    metadata
                        .disc_number
                        .map_or(bridge_traits::database::QueryValue::Null, |n| {
                            bridge_traits::database::QueryValue::Integer(n as i64)
                        }),
                    bridge_traits::database::QueryValue::Integer(metadata.duration_ms as i64),
                    metadata
                        .bitrate
                        .map_or(bridge_traits::database::QueryValue::Null, |b| {
                            bridge_traits::database::QueryValue::Integer(b as i64)
                        }),
                    metadata
                        .sample_rate
                        .map_or(bridge_traits::database::QueryValue::Null, |sr| {
                            bridge_traits::database::QueryValue::Integer(sr as i64)
                        }),
                    metadata
                        .channels
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Integer(c as i64)
                        }),
                    bridge_traits::database::QueryValue::Text(metadata.format.clone()),
                    bridge_traits::database::QueryValue::Text(metadata.mime_type.clone()),
                    bridge_traits::database::QueryValue::Integer(metadata.file_size as i64),
                    artwork_id
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |aid| {
                            bridge_traits::database::QueryValue::Text(aid.clone())
                        }),
                    metadata
                        .year
                        .map_or(bridge_traits::database::QueryValue::Null, |y| {
                            bridge_traits::database::QueryValue::Integer(y as i64)
                        }),
                    metadata
                        .genre
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |g| {
                            bridge_traits::database::QueryValue::Text(g.clone())
                        }),
                    metadata
                        .composer
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Text(c.clone())
                        }),
                    metadata
                        .comment
                        .as_ref()
                        .map_or(bridge_traits::database::QueryValue::Null, |c| {
                            bridge_traits::database::QueryValue::Text(c.clone())
                        }),
                    bridge_traits::database::QueryValue::Integer(chrono::Utc::now().timestamp()),
                    bridge_traits::database::QueryValue::Text(existing_track.id.clone()),
                ],
            )
            .await
            .map_err(|e| SyncError::Internal(format!("Failed to update track: {}", e)))?;

        debug!("Updated track: {} ({})", title, existing_track.id);

        Ok(existing_track.id.clone())
    }

    /// Clean up temporary file
    async fn cleanup_temp_file(&self, path: &Path) {
        if let Err(e) = self.file_system.delete_file(path).await {
            warn!("Failed to clean up temporary file {:?}: {}", path, e);
        }
    }
}

/// Normalize name for searching and matching
fn normalize_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("The Beatles"), "the beatles");
        assert_eq!(normalize_name("AC/DC"), "acdc");
        assert_eq!(normalize_name("  Pink Floyd  "), "pink floyd");
        assert_eq!(normalize_name("Guns N' Roses"), "guns n roses");
    }

    #[test]
    fn test_processor_config_default() {
        let config = ProcessorConfig::default();
        assert!(config.header_only);
        assert_eq!(config.header_size_bytes, 256 * 1024);
        assert!(config.extract_artwork);
        assert!(!config.update_existing);
    }
}
