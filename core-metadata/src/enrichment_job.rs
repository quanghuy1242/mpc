//! # Metadata Enrichment Job
//!
//! Background job system for enriching existing library entries with artwork and lyrics.
//! This module provides batch processing with concurrency limits, retry logic, and
//! progress tracking while respecting platform constraints.
//!
//! ## Overview
//!
//! The enrichment job system:
//! - Queries tracks missing artwork or lyrics
//! - Processes tracks in configurable batches
//! - Respects network constraints (WiFi-only option)
//! - Retries failed fetches with exponential backoff
//! - Emits progress events for UI updates
//! - Integrates with BackgroundExecutor for scheduling
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────┐
//! │ EnrichmentJob    │
//! │ - Config         │
//! │ - State tracking │
//! └────────┬─────────┘
//!          │
//!          ├──> ArtworkService (fetch remote artwork)
//!          ├──> LyricsService  (fetch lyrics)
//!          ├──> TrackRepository (query & update tracks)
//!          ├──> NetworkMonitor (check connectivity)
//!          └──> EventBus (emit progress)
//! ```
//!
//! ## Usage
//!
//! ### Basic Setup
//!
//! ```ignore
//! use core_metadata::enrichment_job::{EnrichmentJob, EnrichmentConfig};
//! use core_metadata::{ArtworkService, LyricsService};
//! use core_library::repositories::track::SqliteTrackRepository;
//! use core_runtime::events::EventBus;
//! use std::sync::Arc;
//!
//! let config = EnrichmentConfig::default()
//!     .with_batch_size(50)
//!     .with_max_concurrent(5)
//!     .with_require_wifi(true);
//!
//! let job = EnrichmentJob::new(
//!     config,
//!     Arc::new(artwork_service),
//!     Arc::new(lyrics_service),
//!     Arc::new(track_repository),
//!     Arc::new(event_bus),
//! );
//!
//! // Run enrichment
//! job.run().await?;
//! ```
//!
//! ### With Network Monitor
//!
//! ```ignore
//! let job = EnrichmentJob::new(config, artwork, lyrics, repo, events)
//!     .with_network_monitor(Arc::new(network_monitor));
//!
//! job.run().await?;
//! ```
//!
//! ### With Background Executor
//!
//! ```ignore
//! use bridge_traits::background::{BackgroundExecutor, TaskConstraints};
//! use std::time::Duration;
//!
//! // Schedule daily enrichment
//! let constraints = TaskConstraints {
//!     requires_wifi: true,
//!     requires_network: true,
//!     ..Default::default()
//! };
//!
//! executor.schedule_task(
//!     "metadata_enrichment",
//!     Duration::from_secs(86400), // 24 hours
//!     constraints,
//! ).await?;
//! ```

use crate::enrichment_service::{EnrichmentRequest, EnrichmentResponse, EnrichmentService};
use crate::error::{MetadataError, Result};
use bridge_traits::network::{NetworkMonitor, NetworkType};
use core_async::sync::Semaphore;
use core_async::time::sleep;
use core_library::models::Track;
use core_library::repositories::track::TrackRepository;
use core_runtime::events::{CoreEvent, EventBus, LibraryEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for metadata enrichment job
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Number of tracks to process per batch
    pub batch_size: usize,

    /// Maximum number of concurrent operations
    pub max_concurrent: usize,

    /// Whether to fetch artwork
    pub enable_artwork: bool,

    /// Whether to fetch lyrics
    pub enable_lyrics: bool,

    /// Whether to enrich artists with biography and country
    pub enable_artist_enrichment: bool,

    /// Require WiFi connection (recommended for mobile)
    pub require_wifi: bool,

    /// Maximum retry attempts per track
    pub max_retries: u32,

    /// Base delay for exponential backoff (milliseconds)
    pub base_retry_delay_ms: u64,

    /// Timeout for individual operations (seconds)
    pub operation_timeout_secs: u64,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            max_concurrent: 5,
            enable_artwork: true,
            enable_lyrics: true,
            enable_artist_enrichment: false, // Optional, requires provider setup
            require_wifi: false,
            max_retries: 3,
            base_retry_delay_ms: 100,
            operation_timeout_secs: 30,
        }
    }
}

impl EnrichmentConfig {
    /// Create builder for configuration
    pub fn builder() -> Self {
        Self::default()
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set max concurrent operations
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Enable/disable artwork fetching
    pub fn with_artwork(mut self, enabled: bool) -> Self {
        self.enable_artwork = enabled;
        self
    }

    /// Enable/disable lyrics fetching
    pub fn with_lyrics(mut self, enabled: bool) -> Self {
        self.enable_lyrics = enabled;
        self
    }

    /// Enable/disable artist enrichment (biography, country)
    pub fn with_artist_enrichment(mut self, enabled: bool) -> Self {
        self.enable_artist_enrichment = enabled;
        self
    }

    /// Require WiFi connection
    pub fn with_require_wifi(mut self, required: bool) -> Self {
        self.require_wifi = required;
        self
    }

    /// Set max retry attempts
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Set retry delay base
    pub fn with_retry_delay_ms(mut self, delay_ms: u64) -> Self {
        self.base_retry_delay_ms = delay_ms;
        self
    }

    /// Set operation timeout
    pub fn with_timeout_secs(mut self, timeout: u64) -> Self {
        self.operation_timeout_secs = timeout;
        self
    }
}

// =============================================================================
// Progress Tracking
// =============================================================================

/// Progress statistics for enrichment job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentProgress {
    /// Total tracks to process
    pub total_tracks: usize,

    /// Tracks processed so far
    pub processed: usize,

    /// Artwork successfully fetched
    pub artwork_fetched: usize,

    /// Lyrics successfully fetched
    pub lyrics_fetched: usize,

    /// Number of failures
    pub failed: usize,

    /// Number of skipped tracks
    pub skipped: usize,

    /// Completion percentage (0-100)
    pub percent_complete: u8,

    /// Current phase
    pub phase: String,
}

impl EnrichmentProgress {
    pub fn new(total: usize) -> Self {
        Self {
            total_tracks: total,
            processed: 0,
            artwork_fetched: 0,
            lyrics_fetched: 0,
            failed: 0,
            skipped: 0,
            percent_complete: 0,
            phase: "Starting".to_string(),
        }
    }

    pub fn update(&mut self) {
        self.percent_complete = if self.total_tracks > 0 {
            ((self.processed as f64 / self.total_tracks as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };
    }
}

// =============================================================================
// Enrichment Result
// =============================================================================

/// Result of enriching a single track
#[derive(Debug, Clone)]
pub struct EnrichmentResult {
    /// Track ID
    pub track_id: String,

    /// Whether artwork was fetched
    pub artwork_fetched: bool,

    /// Whether lyrics were fetched
    pub lyrics_fetched: bool,

    /// Error if operation failed
    pub error: Option<String>,
}

// =============================================================================
// Enrichment Job
// =============================================================================

/// Background job for enriching library metadata
pub struct EnrichmentJob {
    config: EnrichmentConfig,
    enrichment_service: Arc<EnrichmentService>,
    track_repository: Arc<dyn TrackRepository>,
    event_bus: Arc<EventBus>,
    network_monitor: Option<Arc<dyn NetworkMonitor>>,
}

impl EnrichmentJob {
    /// Create new enrichment job
    pub fn new(
        config: EnrichmentConfig,
        enrichment_service: Arc<EnrichmentService>,
        track_repository: Arc<dyn TrackRepository>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        Self {
            config,
            enrichment_service,
            track_repository,
            event_bus,
            network_monitor: None,
        }
    }

    /// Add network monitor for WiFi-only mode
    pub fn with_network_monitor(mut self, monitor: Arc<dyn NetworkMonitor>) -> Self {
        self.network_monitor = Some(monitor);
        self
    }

    /// Run the enrichment job
    #[instrument(skip(self), name = "enrichment_job")]
    pub async fn run(&self) -> Result<EnrichmentProgress> {
        info!("Starting metadata enrichment job");

        // Check network constraints
        if self.config.require_wifi {
            self.check_wifi().await?;
        }

        // Query tracks needing enrichment
        let tracks = self.query_tracks_needing_enrichment().await?;

        if tracks.is_empty() {
            info!("No tracks need enrichment");
            return Ok(EnrichmentProgress::new(0));
        }

        info!(
            total_tracks = tracks.len(),
            "Found tracks needing enrichment"
        );

        // Initialize progress
        let mut progress = EnrichmentProgress::new(tracks.len());

        // Emit start event
        self.emit_progress(&progress);

        // Process tracks in batches
        let results = self.process_tracks(tracks, &mut progress).await;

        // Update final progress
        progress.phase = "Completed".to_string();
        progress.update();

        // Emit completion event
        self.emit_progress(&progress);

        info!(
            processed = progress.processed,
            artwork_fetched = progress.artwork_fetched,
            lyrics_fetched = progress.lyrics_fetched,
            failed = progress.failed,
            "Enrichment job completed"
        );

        // Log failures
        for result in results.iter().filter(|r| r.error.is_some()) {
            warn!(
                track_id = %result.track_id,
                error = result.error.as_ref().unwrap(),
                "Track enrichment failed"
            );
        }

        Ok(progress)
    }

    /// Check if WiFi is available
    async fn check_wifi(&self) -> Result<()> {
        if let Some(monitor) = &self.network_monitor {
            let info = monitor.get_network_info().await.map_err(|e| {
                MetadataError::ConfigurationError(format!("Failed to get network info: {}", e))
            })?;

            if !matches!(info.network_type, Some(NetworkType::WiFi)) {
                return Err(MetadataError::ConfigurationError(
                    "WiFi connection required but not available".to_string(),
                ));
            }

            debug!("WiFi connection confirmed");
        } else {
            warn!("WiFi check requested but no NetworkMonitor provided");
        }

        Ok(())
    }

    /// Query tracks that need enrichment
    async fn query_tracks_needing_enrichment(&self) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();

        // Query tracks without artwork
        if self.config.enable_artwork {
            let artwork_tracks = self
                .track_repository
                .find_by_missing_artwork()
                .await
                .map_err(|e| MetadataError::Database(e.to_string()))?;

            debug!(count = artwork_tracks.len(), "Found tracks missing artwork");

            tracks.extend(artwork_tracks);
        }

        // Query tracks without lyrics
        if self.config.enable_lyrics {
            let lyrics_tracks = self
                .track_repository
                .find_by_lyrics_status("not_fetched")
                .await
                .map_err(|e| MetadataError::Database(e.to_string()))?;

            debug!(count = lyrics_tracks.len(), "Found tracks missing lyrics");

            // Merge with existing tracks (deduplicate by ID)
            for track in lyrics_tracks {
                if !tracks.iter().any(|t| t.id == track.id) {
                    tracks.push(track);
                }
            }
        }

        Ok(tracks)
    }

    /// Process tracks in batches with concurrency control
    async fn process_tracks(
        &self,
        tracks: Vec<Track>,
        progress: &mut EnrichmentProgress,
    ) -> Vec<EnrichmentResult> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        let mut results = Vec::new();

        // Process in batches
        for batch in tracks.chunks(self.config.batch_size) {
            progress.phase = format!(
                "Processing batch {}/{}",
                (progress.processed / self.config.batch_size) + 1,
                tracks.len().div_ceil(self.config.batch_size)
            );

            let mut handles = Vec::new();

            for track in batch {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let track = track.clone();
                let job = self.clone_for_task();

                let handle = core_async::task::spawn(async move {
                    let result = job.enrich_track(&track).await;
                    drop(permit);
                    result
                });

                handles.push(handle);
            }

            // Wait for batch to complete
            for handle in handles {
                match handle.await {
                    Ok(result) => {
                        // Update progress
                        progress.processed += 1;

                        if result.artwork_fetched {
                            progress.artwork_fetched += 1;
                        }

                        if result.lyrics_fetched {
                            progress.lyrics_fetched += 1;
                        }

                        if result.error.is_some() {
                            progress.failed += 1;
                        }

                        results.push(result);
                    }
                    Err(e) => {
                        error!(error = %e, "Task panicked");
                        progress.processed += 1;
                        progress.failed += 1;
                    }
                }
            }

            // Update progress percentage
            progress.update();

            // Emit progress update
            self.emit_progress(progress);

            // Small delay between batches to avoid overwhelming services
            sleep(Duration::from_millis(100)).await;
        }

        results
    }

    /// Enrich a single track
    #[instrument(skip(self), fields(track_id = %track.id))]
    async fn enrich_track(&self, track: &Track) -> EnrichmentResult {
        let mut result = EnrichmentResult {
            track_id: track.id.clone(),
            artwork_fetched: false,
            lyrics_fetched: false,
            error: None,
        };

        debug!("Starting enrichment");

        // Create enrichment request
        let request = EnrichmentRequest {
            track: track.clone(),
            fetch_artwork: self.config.enable_artwork,
            fetch_lyrics: self.config.enable_lyrics,
        };

        // Perform enrichment with retry logic
        match self.enrich_with_retry(&request).await {
            Ok(response) => {
                result.artwork_fetched = response.artwork_fetched;
                result.lyrics_fetched = response.lyrics_fetched;

                // Emit library event
                let mut updated_fields = Vec::new();
                if response.artwork_fetched {
                    updated_fields.push("artwork_id".to_string());
                }
                if response.lyrics_fetched {
                    updated_fields.push("lyrics_status".to_string());
                }

                if !updated_fields.is_empty() {
                    self.event_bus
                        .emit(CoreEvent::Library(LibraryEvent::TrackUpdated {
                            track_id: track.id.clone(),
                            updated_fields,
                        }))
                        .ok();
                }

                debug!(
                    artwork_fetched = result.artwork_fetched,
                    lyrics_fetched = result.lyrics_fetched,
                    "Enrichment completed"
                );
            }
            Err(e) => {
                warn!(error = %e, "Enrichment failed");
                result.error = Some(e.to_string());
            }
        }

        result
    }

    /// Enrich track with retry logic
    async fn enrich_with_retry(&self, request: &EnrichmentRequest) -> Result<EnrichmentResponse> {
        for attempt in 0..=self.config.max_retries {
            match self.enrichment_service.enrich_track(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) if attempt < self.config.max_retries => {
                    let delay = self.calculate_backoff(attempt);
                    warn!(
                        attempt = attempt + 1,
                        max_attempts = self.config.max_retries + 1,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Enrichment failed, retrying"
                    );
                    sleep(delay).await;
                }
                Err(e) => return Err(e),
            }
        }

        unreachable!()
    }

    /// Calculate exponential backoff delay
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let delay_ms = self.config.base_retry_delay_ms * 2u64.pow(attempt);
        let max_delay_ms = 10_000; // Cap at 10 seconds
        Duration::from_millis(delay_ms.min(max_delay_ms))
    }

    /// Emit progress event
    fn emit_progress(&self, progress: &EnrichmentProgress) {
        self.event_bus
            .emit(CoreEvent::Library(LibraryEvent::TrackUpdated {
                track_id: format!("enrichment_progress_{}", progress.percent_complete),
                updated_fields: vec!["enrichment".to_string()],
            }))
            .ok();
    }

    /// Clone self for spawned tasks
    fn clone_for_task(&self) -> Self {
        Self {
            config: self.config.clone(),
            enrichment_service: Arc::clone(&self.enrichment_service),
            track_repository: Arc::clone(&self.track_repository),
            event_bus: Arc::clone(&self.event_bus),
            network_monitor: self.network_monitor.as_ref().map(Arc::clone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrichment_config_builder() {
        let config = EnrichmentConfig::builder()
            .with_batch_size(100)
            .with_max_concurrent(10)
            .with_require_wifi(true);

        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_concurrent, 10);
        assert!(config.require_wifi);
    }

    #[test]
    fn test_enrichment_progress_new() {
        let progress = EnrichmentProgress::new(100);

        assert_eq!(progress.total_tracks, 100);
        assert_eq!(progress.processed, 0);
        assert_eq!(progress.percent_complete, 0);
    }

    #[test]
    fn test_enrichment_progress_update() {
        let mut progress = EnrichmentProgress::new(100);
        progress.processed = 50;
        progress.update();

        assert_eq!(progress.percent_complete, 50);
    }

    #[test]
    fn test_enrichment_progress_complete() {
        let mut progress = EnrichmentProgress::new(100);
        progress.processed = 100;
        progress.update();

        assert_eq!(progress.percent_complete, 100);
    }

    #[test]
    fn test_enrichment_progress_over_100() {
        let mut progress = EnrichmentProgress::new(100);
        progress.processed = 150; // Should cap at 100%
        progress.update();

        assert_eq!(progress.percent_complete, 100);
    }

    #[test]
    fn test_enrichment_config_defaults() {
        let config = EnrichmentConfig::default();

        assert_eq!(config.batch_size, 50);
        assert_eq!(config.max_concurrent, 5);
        assert!(config.enable_artwork);
        assert!(config.enable_lyrics);
        assert!(!config.require_wifi);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_calculate_backoff() {
        let config = EnrichmentConfig::default();
        let job = create_test_job(config);

        assert_eq!(job.calculate_backoff(0), Duration::from_millis(100));
        assert_eq!(job.calculate_backoff(1), Duration::from_millis(200));
        assert_eq!(job.calculate_backoff(2), Duration::from_millis(400));
        assert_eq!(job.calculate_backoff(3), Duration::from_millis(800));

        // Test capping at 10 seconds
        assert_eq!(job.calculate_backoff(10), Duration::from_millis(10_000));
    }

    // Helper function to create test job
    fn create_test_job(config: EnrichmentConfig) -> EnrichmentJob {
        use crate::artwork::ArtworkService;
        use crate::enrichment_service::EnrichmentService;
        use crate::lyrics::LyricsService;
        use core_library::db::create_test_pool;
        use core_library::repositories::album::SqliteAlbumRepository;
        use core_library::repositories::artist::SqliteArtistRepository;
        use core_library::repositories::artwork::SqliteArtworkRepository;
        use core_library::repositories::lyrics::SqliteLyricsRepository;
        use core_library::repositories::track::{SqliteTrackRepository, TrackRepository};
        use core_runtime::events::EventBus;
        use std::sync::Once;

        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Initialize test runtime if needed
        });

        // Create in-memory database for testing
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pool = rt.block_on(create_test_pool()).unwrap();

        let artist_repo = Arc::new(SqliteArtistRepository::new(pool.clone()));
        let album_repo = Arc::new(SqliteAlbumRepository::new(pool.clone()));
        let artwork_repo = Arc::new(SqliteArtworkRepository::new(pool.clone()));
        let lyrics_repo = Arc::new(SqliteLyricsRepository::new(pool.clone()));
        let track_repo: Arc<dyn TrackRepository> =
            Arc::new(SqliteTrackRepository::new(pool.clone()));
        let event_bus = Arc::new(EventBus::new(100));

        let artwork_service = Arc::new(ArtworkService::new(artwork_repo, 200 * 1024 * 1024));
        let lyrics_service = Arc::new(LyricsService::without_providers(lyrics_repo));

        let enrichment_service = Arc::new(EnrichmentService::new(
            artist_repo,
            album_repo,
            Arc::clone(&track_repo),
            artwork_service,
            lyrics_service,
        ));

        EnrichmentJob::new(config, enrichment_service, track_repo, event_bus)
    }
}
