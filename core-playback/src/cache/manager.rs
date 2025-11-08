//! # Offline Cache Manager
//!
//! Main orchestrator for downloading, encrypting, and managing offline cached tracks.
//!
//! This module provides production-ready caching with:
//! - Concurrent downloads with semaphore-based throttling
//! - Automatic LRU/LFU/FIFO eviction when cache is full
//! - Optional AES-256-GCM encryption
//! - Progress tracking and retry logic
//! - Integrity verification using SHA-256 hashes
//! - Cross-platform support (native and WASM)

use crate::cache::{
    config::{CacheConfig, EvictionPolicy},
    encryption::{CacheEncryptor, EncryptionKey},
    stats::{CacheStats, DownloadProgress},
    CachedTrack, CacheMetadataRepository, CacheStatus, RepoCacheStats, SqliteCacheMetadataRepository,
};
use crate::error::{PlaybackError, Result};
use bridge_traits::{
    database::DatabaseAdapter, http::HttpClient, storage::FileSystemAccess,
    storage::StorageProvider,
};
use bytes::Bytes;
use core_async::sync::{Mutex, Semaphore};
use core_async::time::timeout;
use core_library::models::{Track, TrackId};
use core_library::repositories::TrackRepository;
use core_runtime::events::{CoreEvent, EventBus};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Offline cache manager for downloading and managing cached tracks.
pub struct OfflineCacheManager {
    config: CacheConfig,
    repository: Arc<dyn CacheMetadataRepository>,
    track_repository: Arc<dyn TrackRepository>,
    fs: Arc<dyn FileSystemAccess>,
    http_client: Arc<dyn HttpClient>,
    storage_provider: Arc<dyn StorageProvider>,
    encryptor: Option<Arc<CacheEncryptor>>,
    event_bus: Option<Arc<EventBus>>,
    download_semaphore: Arc<Semaphore>,
    active_downloads: Arc<Mutex<HashMap<TrackId, Arc<Mutex<DownloadProgress>>>>>,
    cache_base_path: Arc<Mutex<Option<PathBuf>>>,
}

impl OfflineCacheManager {
    /// Create a new offline cache manager.
    ///
    /// # Arguments
    ///
    /// * `config` - Cache configuration
    /// * `db` - Database adapter for metadata storage
    /// * `track_repository` - Track repository for looking up track details
    /// * `fs` - Filesystem access for storing cached files
    /// * `http_client` - HTTP client for downloading tracks
    /// * `storage_provider` - Storage provider for accessing remote files
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use core_playback::cache::{OfflineCacheManager, CacheConfig};
    /// use std::sync::Arc;
    ///
    /// let config = CacheConfig::default();
    /// let manager = OfflineCacheManager::new(
    ///     config,
    ///     db_adapter,
    ///     track_repo,
    ///     filesystem,
    ///     http_client,
    ///     storage_provider,
    /// );
    /// ```
    pub fn new(
        config: CacheConfig,
        db: Arc<dyn DatabaseAdapter>,
        track_repository: Arc<dyn TrackRepository>,
        fs: Arc<dyn FileSystemAccess>,
        http_client: Arc<dyn HttpClient>,
        storage_provider: Arc<dyn StorageProvider>,
    ) -> Self {
        let repository = Arc::new(SqliteCacheMetadataRepository::new(db));
        let download_semaphore = Arc::new(Semaphore::new(config.max_concurrent_downloads));

        Self {
            config,
            repository,
            track_repository,
            fs,
            http_client,
            storage_provider,
            encryptor: None,
            event_bus: None,
            download_semaphore,
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            cache_base_path: Arc::new(Mutex::new(None)),
        }
    }

    /// Set encryption key for encrypted caching.
    pub fn with_encryption(mut self, key: EncryptionKey) -> Self {
        self.encryptor = Some(Arc::new(CacheEncryptor::new(key)));
        self
    }

    /// Set event bus for progress events.
    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Initialize the cache manager (create directories, initialize DB).
    #[instrument(skip(self))]
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing offline cache manager");

        // Validate configuration
        self.config.validate().map_err(|e| {
            PlaybackError::CacheError(format!("Invalid cache configuration: {}", e))
        })?;

        // Initialize database tables
        self.repository.initialize().await?;

        // Get cache directory path
        let cache_dir = self.fs.get_cache_directory().await.map_err(|e| {
            error!("Failed to get cache directory: {}", e);
            PlaybackError::CacheError(format!("Failed to get cache directory: {}", e))
        })?;

        let cache_path = cache_dir.join(&self.config.cache_directory);

        // Create cache directory
        self.fs.create_dir_all(&cache_path).await.map_err(|e| {
            error!("Failed to create cache directory: {}", e);
            PlaybackError::CacheError(format!("Failed to create cache directory: {}", e))
        })?;

        // Store cache base path
        *self.cache_base_path.lock().await = Some(cache_path.clone());

        info!("Cache manager initialized at {:?}", cache_path);
        Ok(())
    }

    /// Check if a track is cached and available for playback.
    #[instrument(skip(self))]
    pub async fn is_cached(&self, track_id: &TrackId) -> Result<bool> {
        match self.repository.find_by_track_id(track_id).await? {
            Some(track) => Ok(track.status.is_available()),
            None => Ok(false),
        }
    }

    /// Get the status of a cached track.
    #[instrument(skip(self))]
    pub async fn get_cache_status(&self, track_id: &TrackId) -> Result<CacheStatus> {
        match self.repository.find_by_track_id(track_id).await? {
            Some(track) => Ok(track.status),
            None => Ok(CacheStatus::NotCached),
        }
    }

    /// Download a track to the offline cache.
    ///
    /// This method:
    /// 1. Looks up track metadata from the library
    /// 2. Downloads the file from the storage provider
    /// 3. Optionally encrypts the data
    /// 4. Verifies integrity using SHA-256 hash
    /// 5. Stores the file and updates metadata
    ///
    /// # Arguments
    ///
    /// * `track_id` - ID of the track to download
    ///
    /// # Returns
    ///
    /// Ok(()) if successful, or an error if the download fails
    #[instrument(skip(self))]
    pub async fn download_track(&self, track_id: TrackId) -> Result<()> {
        info!("Starting download for track {}", track_id);

        // Check if already cached
        if self.is_cached(&track_id).await? {
            info!("Track {} is already cached", track_id);
            return Ok(());
        }

        // Acquire download permit from semaphore
        let _permit = timeout(
            Duration::from_secs(30),
            self.download_semaphore.acquire(),
        )
        .await
        .map_err(|_| {
            PlaybackError::CacheError("Timeout waiting for download slot".to_string())
        })?
        .map_err(|_| PlaybackError::CacheError("Semaphore closed".to_string()))?;

        // Look up track from library
        let track = self
            .track_repository
            .find_by_id(&track_id.to_string())
            .await
            .map_err(|e| {
                error!("Failed to find track {}: {}", track_id, e);
                PlaybackError::CacheError(format!("Track not found: {}", e))
            })?
            .ok_or_else(|| PlaybackError::TrackNotFound(track_id.to_string()))?;

        // Get or create cache entry
        let mut cached_track = match self.repository.find_by_track_id(&track_id).await? {
            Some(mut entry) => {
                // Reset if failed/stale
                if entry.status.needs_download() {
                    entry.mark_downloading();
                    entry
                } else {
                    entry
                }
            }
            None => {
                let cache_filename = format!("{}.cache", track_id);
                let file_size = track.file_size.unwrap_or(0) as u64;
                CachedTrack::new(track_id, cache_filename, file_size)
            }
        };

        cached_track.mark_downloading();

        // Insert or update in database
        if self.repository.find_by_track_id(&track_id).await?.is_some() {
            self.repository.update(&cached_track).await?;
        } else {
            self.repository.insert(&cached_track).await?;
        }

        // Register active download
        let file_size = track.file_size.unwrap_or(0) as u64;
        let progress = Arc::new(Mutex::new(DownloadProgress::new(
            track_id.to_string(),
            file_size,
        )));
        self.active_downloads
            .lock()
            .await
            .insert(track_id, progress.clone());

        // Perform download with retry
        let result = self
            .download_with_retry(&track, &mut cached_track, progress.clone())
            .await;

        // Remove from active downloads
        self.active_downloads.lock().await.remove(&track_id);

        // Update final status
        match result {
            Ok(_) => {
                info!("Successfully downloaded track {}", track_id);
                self.repository.update(&cached_track).await?;
                Ok(())
            }
            Err(e) => {
                error!("Failed to download track {}: {}", track_id, e);
                cached_track.mark_failed(e.to_string());
                self.repository.update(&cached_track).await?;
                Err(e)
            }
        }
    }

    /// Download with automatic retry logic.
    async fn download_with_retry(
        &self,
        track: &Track,
        cached_track: &mut CachedTrack,
        progress: Arc<Mutex<DownloadProgress>>,
    ) -> Result<()> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_retry_attempts {
            debug!(
                "Download attempt {}/{} for track {}",
                attempt, self.config.max_retry_attempts, track.id
            );

            match timeout(
                self.config.download_timeout,
                self.download_track_internal(track, cached_track, progress.clone()),
            )
            .await
            {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => {
                    warn!("Download attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                }
                Err(_) => {
                    warn!("Download attempt {} timed out", attempt);
                    last_error = Some(PlaybackError::CacheError("Download timeout".to_string()));
                }
            }

            // Wait before retry (exponential backoff)
            if attempt < self.config.max_retry_attempts {
                let delay = Duration::from_millis(100 * 2u64.pow((attempt - 1) as u32));
                core_async::time::sleep(delay).await;
            }
        }

        Err(last_error.unwrap_or_else(|| {
            PlaybackError::CacheError("Download failed after all retries".to_string())
        }))
    }

    /// Internal download implementation.
    async fn download_track_internal(
        &self,
        track: &Track,
        cached_track: &mut CachedTrack,
        progress: Arc<Mutex<DownloadProgress>>,
    ) -> Result<()> {
        // Get file metadata from storage provider
        let remote_file = self
            .storage_provider
            .get_metadata(&track.provider_file_id)
            .await
            .map_err(|e| {
                PlaybackError::CacheError(format!("Failed to get file metadata: {}", e))
            })?;

        // Download file content
        debug!("Downloading file from provider: {}", remote_file.name);
        let data = self
            .storage_provider
            .download(&track.provider_file_id, None)
            .await
            .map_err(|e| PlaybackError::CacheError(format!("Download failed: {}", e)))?;

        // Update progress
        {
            let mut p = progress.lock().await;
            p.update(data.len() as u64);
        }

        // Verify integrity
        if self.config.verify_integrity {
            if let Some(track_hash) = &track.hash {
                if !track_hash.is_empty() {
                    let hash = self.calculate_hash(&data);
                    if hash != *track_hash {
                        return Err(PlaybackError::CacheError(format!(
                            "Hash mismatch: expected {}, got {}",
                            track_hash, hash
                        )));
                    }
                }
            }
        }

        // Optionally encrypt
        let (final_data, encrypted) = if self.config.enable_encryption {
            if let Some(encryptor) = &self.encryptor {
                debug!("Encrypting cached file");
                let encrypted_data = encryptor.encrypt(&data)?;
                (encrypted_data, true)
            } else {
                warn!("Encryption enabled but no encryptor available");
                (data, false)
            }
        } else {
            (data, false)
        };

        // Get cache file path
        let cache_base = self
            .cache_base_path
            .lock()
            .await
            .clone()
            .ok_or_else(|| PlaybackError::CacheError("Cache not initialized".to_string()))?;

        let cache_file_path = cache_base.join(&cached_track.cache_path);

        // Write to filesystem
        debug!("Writing cached file to {:?}", cache_file_path);
        self.fs
            .write_file(&cache_file_path, final_data.clone())
            .await
            .map_err(|e| {
                PlaybackError::CacheError(format!("Failed to write cache file: {}", e))
            })?;

        // Calculate content hash for verification
        let content_hash = self.calculate_hash(&final_data);

        // Update cached track metadata
        cached_track.mark_cached(final_data.len() as u64, content_hash, encrypted);

        info!(
            "Track {} cached successfully (size: {} bytes, encrypted: {})",
            track.id,
            final_data.len(),
            encrypted
        );

        Ok(())
    }

    /// Calculate SHA-256 hash of data.
    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Get download progress for a track (if currently downloading).
    #[instrument(skip(self))]
    pub async fn get_download_progress(&self, track_id: &TrackId) -> Option<DownloadProgress> {
        let downloads = self.active_downloads.lock().await;
        if let Some(progress_mutex) = downloads.get(track_id) {
            Some(progress_mutex.lock().await.clone())
        } else {
            None
        }
    }

    /// Get all tracks currently being downloaded.
    pub async fn get_active_downloads(&self) -> Vec<DownloadProgress> {
        let downloads = self.active_downloads.lock().await;
        let mut result = Vec::new();

        for progress_mutex in downloads.values() {
            result.push(progress_mutex.lock().await.clone());
        }

        result
    }

    /// Evict tracks from cache to free up space.
    ///
    /// Uses the configured eviction policy to determine which tracks to remove.
    #[instrument(skip(self))]
    pub async fn evict_tracks(&self, bytes_needed: u64) -> Result<usize> {
        info!("Evicting tracks to free {} bytes", bytes_needed);

        let mut bytes_freed = 0u64;
        let mut tracks_evicted = 0usize;

        while bytes_freed < bytes_needed {
            // Find candidates for eviction based on policy
            let candidates = match self.config.eviction_policy {
                EvictionPolicy::LeastRecentlyUsed => {
                    self.repository.find_for_lru_eviction(10).await?
                }
                EvictionPolicy::LeastFrequentlyUsed => {
                    self.repository.find_for_lfu_eviction(10).await?
                }
                EvictionPolicy::FirstInFirstOut => {
                    self.repository.find_for_fifo_eviction(10).await?
                }
                EvictionPolicy::LargestFirst => {
                    self.repository.find_for_largest_eviction(10).await?
                }
            };

            if candidates.is_empty() {
                warn!("No more tracks available for eviction");
                break;
            }

            // Evict first candidate
            let track = &candidates[0];
            if let Err(e) = self.evict_single_track(&track.track_id).await {
                error!("Failed to evict track {}: {}", track.track_id, e);
                continue;
            }

            bytes_freed += track.cached_size;
            tracks_evicted += 1;

            info!(
                "Evicted track {} ({} bytes), total freed: {} bytes",
                track.track_id, track.cached_size, bytes_freed
            );
        }

        info!(
            "Eviction complete: {} tracks removed, {} bytes freed",
            tracks_evicted, bytes_freed
        );

        Ok(tracks_evicted)
    }

    /// Evict oldest tracks until cache is under the size limit.
    #[instrument(skip(self))]
    pub async fn evict_oldest(&self) -> Result<usize> {
        let stats = self.get_cache_stats().await?;
        let bytes_over = stats.space_needed(self.config.max_cache_size_bytes);

        if bytes_over > 0 {
            self.evict_tracks(bytes_over).await
        } else {
            Ok(0)
        }
    }

    /// Evict a single track from the cache.
    async fn evict_single_track(&self, track_id: &TrackId) -> Result<()> {
        // Get cache entry
        let cached_track = self
            .repository
            .find_by_track_id(track_id)
            .await?
            .ok_or_else(|| PlaybackError::NotCached(track_id.to_string()))?;

        // Get cache file path
        let cache_base = self
            .cache_base_path
            .lock()
            .await
            .clone()
            .ok_or_else(|| PlaybackError::CacheError("Cache not initialized".to_string()))?;

        let cache_file_path = cache_base.join(&cached_track.cache_path);

        // Delete file from filesystem
        if let Err(e) = self.fs.delete_file(&cache_file_path).await {
            warn!("Failed to delete cache file {:?}: {}", cache_file_path, e);
            // Continue anyway - file might already be deleted
        }

        // Remove from database
        self.repository.delete(track_id).await?;

        debug!("Evicted track {}", track_id);
        Ok(())
    }

    /// Read cached track data (decrypting if necessary).
    #[instrument(skip(self))]
    pub async fn read_cached_track(&self, track_id: &TrackId) -> Result<Bytes> {
        // Get cache entry
        let mut cached_track = self
            .repository
            .find_by_track_id(track_id)
            .await?
            .ok_or_else(|| PlaybackError::NotCached(track_id.to_string()))?;

        if !cached_track.status.is_available() {
            return Err(PlaybackError::NotCached(format!(
                "Track {} is not available (status: {:?})",
                track_id, cached_track.status
            )));
        }

        // Get cache file path
        let cache_base = self
            .cache_base_path
            .lock()
            .await
            .clone()
            .ok_or_else(|| PlaybackError::CacheError("Cache not initialized".to_string()))?;

        let cache_file_path = cache_base.join(&cached_track.cache_path);

        // Read file
        let data = self.fs.read_file(&cache_file_path).await.map_err(|e| {
            error!("Failed to read cached file: {}", e);
            PlaybackError::CacheError(format!("Failed to read cached file: {}", e))
        })?;

        // Decrypt if needed
        let final_data = if cached_track.encrypted {
            if let Some(encryptor) = &self.encryptor {
                debug!("Decrypting cached file");
                encryptor.decrypt(&data)?
            } else {
                return Err(PlaybackError::EncryptionError(
                    "Track is encrypted but no decryption key available".to_string(),
                ));
            }
        } else {
            data
        };

        // Verify integrity
        if self.config.verify_integrity && !cached_track.content_hash.is_empty() {
            let hash = self.calculate_hash(&final_data);
            if hash != cached_track.content_hash {
                warn!("Cache integrity check failed for track {}", track_id);
                cached_track.mark_stale();
                self.repository.update(&cached_track).await?;
                return Err(PlaybackError::CacheError(
                    "Cache integrity check failed".to_string(),
                ));
            }
        }

        // Update play count and last accessed time
        cached_track.record_play();
        self.repository.update(&cached_track).await?;

        Ok(final_data)
    }

    /// Get cache statistics.
    #[instrument(skip(self))]
    pub async fn get_cache_stats(&self) -> Result<CacheStats> {
        let repo_stats = self.repository.get_stats().await?;
        
        // Convert RepoCacheStats to CacheStats (same structure)
        Ok(CacheStats {
            total_tracks: repo_stats.total_tracks,
            cached_tracks: repo_stats.cached_tracks,
            downloading_tracks: repo_stats.downloading_tracks,
            failed_tracks: repo_stats.failed_tracks,
            total_bytes: repo_stats.total_bytes,
            total_original_bytes: repo_stats.total_original_bytes,
            encrypted_tracks: repo_stats.encrypted_tracks,
            total_plays: repo_stats.total_plays,
            tracks_pending_eviction: repo_stats.tracks_pending_eviction,
            calculated_at: repo_stats.calculated_at,
        })
    }

    /// Get current cache size in bytes.
    pub async fn get_cache_size(&self) -> Result<u64> {
        let stats = self.get_cache_stats().await?;
        Ok(stats.total_bytes)
    }

    /// Clear all cached tracks.
    #[instrument(skip(self))]
    pub async fn clear_cache(&self) -> Result<usize> {
        info!("Clearing all cached tracks");

        let all_tracks = self.repository.find_all().await?;
        let mut cleared = 0;

        for track in all_tracks {
            if let Err(e) = self.evict_single_track(&track.track_id).await {
                error!("Failed to clear track {}: {}", track.track_id, e);
            } else {
                cleared += 1;
            }
        }

        info!("Cleared {} tracks from cache", cleared);
        Ok(cleared)
    }

    /// Delete a specific track from the cache.
    #[instrument(skip(self))]
    pub async fn delete_cached_track(&self, track_id: &TrackId) -> Result<()> {
        self.evict_single_track(track_id).await
    }

    /// Get list of all cached tracks.
    pub async fn list_cached_tracks(&self) -> Result<Vec<CachedTrack>> {
        let tracks = self.repository
            .find_by_status(CacheStatus::Cached)
            .await?;
        Ok(tracks)
    }
}
