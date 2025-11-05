//! # Enrichment Service
//!
//! Core service for enriching track metadata with artwork and lyrics.
//! This service acts as a coordinator between the metadata repositories,
//! artwork service, and lyrics service to provide seamless enrichment operations.
//!
//! ## Overview
//!
//! The `EnrichmentService` is responsible for:
//! - Resolving artist and album IDs to names via repositories
//! - Coordinating artwork fetching with the `ArtworkService`
//! - Coordinating lyrics fetching with the `LyricsService`
//! - Updating track records with enrichment results
//! - Providing a clean API for the `EnrichmentJob` orchestrator
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────┐
//! │ EnrichmentService  │
//! │  - Repositories    │
//! │  - Services        │
//! └──────────┬─────────┘
//!            │
//!            ├──> ArtistRepository  (resolve artist_id → name)
//!            ├──> AlbumRepository   (resolve album_id → name)
//!            ├──> TrackRepository   (update track records)
//!            ├──> ArtworkService    (fetch remote artwork)
//!            └──> LyricsService     (fetch lyrics)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::enrichment_service::{EnrichmentService, EnrichmentRequest};
//! use std::sync::Arc;
//!
//! let service = EnrichmentService::new(
//!     Arc::new(artist_repository),
//!     Arc::new(album_repository),
//!     Arc::new(track_repository),
//!     Arc::new(artwork_service),
//!     Arc::new(lyrics_service),
//! );
//!
//! // Enrich a track with both artwork and lyrics
//! let request = EnrichmentRequest {
//!     track,
//!     fetch_artwork: true,
//!     fetch_lyrics: true,
//! };
//!
//! let result = service.enrich_track(request).await?;
//! ```

use crate::artwork::ArtworkService;
use crate::error::{MetadataError, Result};
use crate::lyrics::{LyricsSearchQuery, LyricsService};
use crate::providers::artist_enrichment::ArtistEnrichmentProvider;
use core_library::models::Track;
use core_library::repositories::album::AlbumRepository;
use core_library::repositories::artist::ArtistRepository;
use core_library::repositories::track::TrackRepository;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

// =============================================================================
// Request/Response Types
// =============================================================================

/// Request to enrich a track with metadata
#[derive(Debug, Clone)]
pub struct EnrichmentRequest {
    /// Track to enrich
    pub track: Track,
    /// Whether to fetch artwork
    pub fetch_artwork: bool,
    /// Whether to fetch lyrics
    pub fetch_lyrics: bool,
}

/// Result of enriching a single track
#[derive(Debug, Clone)]
pub struct EnrichmentResponse {
    /// Updated track (if any fields changed)
    pub track: Track,
    /// Whether artwork was successfully fetched
    pub artwork_fetched: bool,
    /// Whether lyrics were successfully fetched
    pub lyrics_fetched: bool,
    /// Artwork ID if fetched (None if skipped or failed)
    pub artwork_id: Option<String>,
    /// Lyrics status after operation
    pub lyrics_status: String,
}

// =============================================================================
// Enrichment Service
// =============================================================================

/// Service for enriching track metadata with artwork and lyrics
///
/// This service coordinates between repositories and metadata services to
/// provide complete enrichment capabilities. It resolves IDs to names,
/// fetches remote metadata, and updates track records atomically.
#[derive(Clone)]
pub struct EnrichmentService {
    artist_repository: Arc<dyn ArtistRepository>,
    album_repository: Arc<dyn AlbumRepository>,
    track_repository: Arc<dyn TrackRepository>,
    #[cfg_attr(not(feature = "artwork-remote"), allow(dead_code))]
    artwork_service: Arc<ArtworkService>,
    lyrics_service: Arc<LyricsService>,
    artist_enrichment_provider: Option<Arc<ArtistEnrichmentProvider>>,
}

impl EnrichmentService {
    /// Create a new enrichment service
    ///
    /// # Arguments
    /// * `artist_repository` - Repository for artist data
    /// * `album_repository` - Repository for album data
    /// * `track_repository` - Repository for track data
    /// * `artwork_service` - Service for fetching artwork
    /// * `lyrics_service` - Service for fetching lyrics
    pub fn new(
        artist_repository: Arc<dyn ArtistRepository>,
        album_repository: Arc<dyn AlbumRepository>,
        track_repository: Arc<dyn TrackRepository>,
        artwork_service: Arc<ArtworkService>,
        lyrics_service: Arc<LyricsService>,
    ) -> Self {
        Self {
            artist_repository,
            album_repository,
            track_repository,
            artwork_service,
            lyrics_service,
            artist_enrichment_provider: None,
        }
    }

    /// Set the artist enrichment provider
    ///
    /// Enables artist biography and country fetching from MusicBrainz.
    ///
    /// # Arguments
    /// * `provider` - Artist enrichment provider instance
    pub fn with_artist_enrichment(mut self, provider: Arc<ArtistEnrichmentProvider>) -> Self {
        self.artist_enrichment_provider = Some(provider);
        self
    }

    /// Enrich a track with artwork and/or lyrics
    ///
    /// This method:
    /// 1. Resolves artist and album names from their IDs
    /// 2. Fetches artwork if requested and missing
    /// 3. Fetches lyrics if requested and missing
    /// 4. Updates the track record in the database
    ///
    /// # Arguments
    /// * `request` - Enrichment request specifying what to fetch
    ///
    /// # Returns
    /// Result containing the enrichment response with updated track and status
    ///
    /// # Errors
    /// Returns error if:
    /// - Required metadata (artist/album) cannot be resolved
    /// - Database operations fail
    /// - Network errors occur during fetching
    #[instrument(skip(self), fields(track_id = %request.track.id))]
    pub async fn enrich_track(&self, request: EnrichmentRequest) -> Result<EnrichmentResponse> {
        let mut track = request.track.clone();
        let mut artwork_fetched = false;
        let mut lyrics_fetched = false;
        let mut artwork_id = None;

        debug!(
            fetch_artwork = request.fetch_artwork,
            fetch_lyrics = request.fetch_lyrics,
            "Starting track enrichment"
        );

        // Fetch artwork if requested and missing
        if request.fetch_artwork && track.artwork_id.is_none() {
            match self.fetch_and_store_artwork(&track).await {
                Ok(Some(id)) => {
                    artwork_id = Some(id.clone());
                    track.artwork_id = Some(id);
                    artwork_fetched = true;
                    info!("Artwork fetched successfully");
                }
                Ok(None) => {
                    debug!("No artwork found");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to fetch artwork");
                    // Continue with lyrics even if artwork fails
                }
            }
        }

        // Fetch lyrics if requested and not already available
        if request.fetch_lyrics && track.lyrics_status == "not_fetched" {
            match self.fetch_and_store_lyrics(&track).await {
                Ok(status) => {
                    track.lyrics_status = status.clone();
                    lyrics_fetched = status == "available";
                    if lyrics_fetched {
                        info!("Lyrics fetched successfully");
                    } else {
                        debug!("Lyrics not found");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to fetch lyrics");
                    track.lyrics_status = "unavailable".to_string();
                }
            }
        }

        // Update track in database if anything changed
        if artwork_fetched || lyrics_fetched {
            // Note: updated_at is managed by the database layer
            // We don't manually set it here
            self.track_repository
                .update(&track)
                .await
                .map_err(|e| MetadataError::Database(e.to_string()))?;

            debug!(artwork_fetched, lyrics_fetched, "Track updated in database");
        }

        Ok(EnrichmentResponse {
            track,
            artwork_fetched,
            lyrics_fetched,
            artwork_id,
            lyrics_status: request.track.lyrics_status.clone(),
        })
    }

    /// Fetch and store artwork for a track
    ///
    /// This method:
    /// 1. Resolves artist and album names from their IDs
    /// 2. Calls the artwork service to fetch remote artwork
    /// 3. Returns the artwork ID if successful
    ///
    /// # Returns
    /// - `Ok(Some(artwork_id))` if artwork was fetched and stored
    /// - `Ok(None)` if no artwork was found
    /// - `Err` if resolution or fetching fails
    #[cfg(feature = "artwork-remote")]
    #[instrument(skip(self), fields(track_id = %track.id))]
    async fn fetch_and_store_artwork(&self, track: &Track) -> Result<Option<String>> {
        // Validate track has required metadata
        if track.artist_id.is_none() || track.album_id.is_none() {
            return Err(MetadataError::ValidationError(
                "Track missing artist or album information".to_string(),
            ));
        }

        // Resolve artist name
        let artist_id = track.artist_id.as_ref().unwrap();
        let artist = self
            .artist_repository
            .find_by_id(artist_id)
            .await
            .map_err(|e| MetadataError::Database(e.to_string()))?
            .ok_or_else(|| MetadataError::Database(format!("Artist not found: {}", artist_id)))?;

        // Resolve album name
        let album_id = track.album_id.as_ref().unwrap();
        let album = self
            .album_repository
            .find_by_id(album_id)
            .await
            .map_err(|e| MetadataError::Database(e.to_string()))?
            .ok_or_else(|| MetadataError::Database(format!("Album not found: {}", album_id)))?;

        debug!(
            artist = %artist.name,
            album = %album.name,
            "Resolved artist and album names"
        );

        // Fetch remote artwork
        // Note: mbid is not currently stored in our models, pass None
        let artwork = self
            .artwork_service
            .fetch_remote(&artist.name, &album.name, None)
            .await?;

        match artwork {
            Some(processed_artwork) => {
                // The artwork service has already stored the artwork
                // and returned the ProcessedArtwork with ID
                Ok(Some(processed_artwork.id))
            }
            None => Ok(None),
        }
    }

    /// Fetch and store artwork for a track (stub when artwork-remote feature is disabled)
    ///
    /// When the artwork-remote feature is not enabled, this method returns None
    /// immediately without attempting to fetch artwork.
    #[cfg(not(feature = "artwork-remote"))]
    #[instrument(skip(self), fields(track_id = %track.id))]
    async fn fetch_and_store_artwork(&self, track: &Track) -> Result<Option<String>> {
        debug!("Remote artwork fetching disabled (artwork-remote feature not enabled)");
        Ok(None)
    }

    /// Fetch and store lyrics for a track
    ///
    /// This method:
    /// 1. Resolves artist and album names from their IDs
    /// 2. Creates a lyrics search query
    /// 3. Calls the lyrics service to fetch lyrics
    /// 4. Returns the lyrics status
    ///
    /// # Returns
    /// - `Ok("available")` if lyrics were fetched and stored
    /// - `Ok("unavailable")` if no lyrics were found
    /// - `Err` if resolution or fetching fails
    #[instrument(skip(self), fields(track_id = %track.id))]
    async fn fetch_and_store_lyrics(&self, track: &Track) -> Result<String> {
        // Validate track has required metadata
        if track.artist_id.is_none() {
            return Err(MetadataError::ValidationError(
                "Track missing artist information".to_string(),
            ));
        }

        // Resolve artist name
        let artist_id = track.artist_id.as_ref().unwrap();
        let artist = self
            .artist_repository
            .find_by_id(artist_id)
            .await
            .map_err(|e| MetadataError::Database(e.to_string()))?
            .ok_or_else(|| MetadataError::Database(format!("Artist not found: {}", artist_id)))?;

        // Resolve album name if available (optional for lyrics)
        let album_name = if let Some(album_id) = &track.album_id {
            match self.album_repository.find_by_id(album_id).await {
                Ok(Some(album)) => Some(album.name),
                _ => None,
            }
        } else {
            None
        };

        debug!(
            artist = %artist.name,
            album = ?album_name,
            title = %track.title,
            "Resolved metadata for lyrics search"
        );

        // Create lyrics search query
        let duration_secs = (track.duration_ms / 1000) as u32;
        let query = if let Some(album) = album_name {
            LyricsSearchQuery::new(
                artist.name,
                track.title.clone(),
                album,
                duration_secs,
                track.id.clone(),
            )
        } else {
            LyricsSearchQuery::minimal(artist.name, track.title.clone(), track.id.clone())
        };

        // Fetch lyrics
        let lyrics = self.lyrics_service.fetch_lyrics(&query).await?;

        match lyrics {
            Some(_lyrics) => {
                // Lyrics service has already stored the lyrics
                // in the lyrics table via LyricsRepository
                Ok("available".to_string())
            }
            None => Ok("unavailable".to_string()),
        }
    }

    /// Enrich an artist with biography and country information
    ///
    /// This method fetches artist metadata from MusicBrainz and updates
    /// the artist record in the database with biography and country fields.
    ///
    /// # Arguments
    /// * `artist_id` - ID of the artist to enrich
    ///
    /// # Returns
    /// Result indicating success or failure
    ///
    /// # Errors
    /// Returns error if:
    /// - Artist enrichment provider is not configured
    /// - Artist not found in database
    /// - Network errors occur during fetching
    /// - Database update fails
    #[instrument(skip(self), fields(artist_id = %artist_id))]
    pub async fn enrich_artist(&self, artist_id: &str) -> Result<()> {
        // Check if artist enrichment is enabled
        let provider = self.artist_enrichment_provider.as_ref().ok_or_else(|| {
            MetadataError::ValidationError("Artist enrichment provider not configured".to_string())
        })?;

        // Fetch artist from database
        let mut artist = self
            .artist_repository
            .find_by_id(artist_id)
            .await
            .map_err(|e| MetadataError::Database(e.to_string()))?
            .ok_or_else(|| MetadataError::RemoteApi(format!("Artist not found: {}", artist_id)))?;

        // Skip if already enriched
        if artist.bio.is_some() && artist.country.is_some() {
            debug!("Artist already enriched, skipping");
            return Ok(());
        }

        info!("Fetching artist metadata for: {}", artist.name);

        // Fetch metadata from provider
        let metadata = match provider.fetch_artist_metadata(&artist.name).await {
            Ok(meta) => meta,
            Err(e) => {
                warn!(error = %e, "Failed to fetch artist metadata");
                return Err(e);
            }
        };

        // Update artist record
        let mut updated = false;

        if let Some(bio) = metadata.bio {
            if artist.bio.is_none() {
                artist.bio = Some(bio);
                updated = true;
                info!("Added artist biography");
            }
        }

        if let Some(country) = metadata.country {
            if artist.country.is_none() {
                artist.country = Some(country);
                updated = true;
                info!("Added artist country");
            }
        }

        if updated {
            artist.updated_at = chrono::Utc::now().timestamp();

            self.artist_repository
                .update(&artist)
                .await
                .map_err(|e| MetadataError::Database(e.to_string()))?;

            info!("Artist enrichment completed successfully");
        } else {
            debug!("No new metadata available for artist");
        }

        Ok(())
    }

    /// Enrich multiple artists in batch
    ///
    /// This is a convenience method that calls `enrich_artist()` for each artist
    /// and continues even if some enrichments fail.
    ///
    /// # Arguments
    /// * `artist_ids` - List of artist IDs to enrich
    ///
    /// # Returns
    /// Tuple of (successful_count, failed_count)
    pub async fn enrich_artists_batch(&self, artist_ids: &[String]) -> (usize, usize) {
        let mut successful = 0;
        let mut failed = 0;

        for artist_id in artist_ids {
            match self.enrich_artist(artist_id).await {
                Ok(_) => successful += 1,
                Err(e) => {
                    warn!(artist_id = %artist_id, error = %e, "Artist enrichment failed");
                    failed += 1;
                }
            }
        }

        info!(
            successful = successful,
            failed = failed,
            "Batch artist enrichment completed"
        );

        (successful, failed)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core_library::models::Track;

    /// Test that EnrichmentRequest can be created
    #[test]
    fn test_enrichment_request_creation() {
        let track = Track {
            id: "track1".to_string(),
            provider_id: "gdrive".to_string(),
            provider_file_id: "file1".to_string(),
            hash: None,
            title: "Test Song".to_string(),
            normalized_title: "test song".to_string(),
            album_id: Some("album1".to_string()),
            artist_id: Some("artist1".to_string()),
            album_artist_id: None,
            track_number: Some(1),
            disc_number: 1,
            genre: None,
            year: None,
            duration_ms: 180000,
            bitrate: Some(320),
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(8_000_000),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: None,
            lyrics_status: "not_fetched".to_string(),
            created_at: 1000000,
            updated_at: 1000000,
            provider_modified_at: None,
        };

        let request = EnrichmentRequest {
            track: track.clone(),
            fetch_artwork: true,
            fetch_lyrics: true,
        };

        assert_eq!(request.track.id, "track1");
        assert!(request.fetch_artwork);
        assert!(request.fetch_lyrics);
    }

    /// Test that EnrichmentResponse can be created
    #[test]
    fn test_enrichment_response_creation() {
        let track = Track {
            id: "track1".to_string(),
            provider_id: "gdrive".to_string(),
            provider_file_id: "file1".to_string(),
            hash: None,
            title: "Test Song".to_string(),
            normalized_title: "test song".to_string(),
            album_id: Some("album1".to_string()),
            artist_id: Some("artist1".to_string()),
            album_artist_id: None,
            track_number: Some(1),
            disc_number: 1,
            genre: None,
            year: None,
            duration_ms: 180000,
            bitrate: Some(320),
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(8_000_000),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: Some("artwork1".to_string()),
            lyrics_status: "available".to_string(),
            created_at: 1000000,
            updated_at: 1000000,
            provider_modified_at: None,
        };

        let response = EnrichmentResponse {
            track: track.clone(),
            artwork_fetched: true,
            lyrics_fetched: true,
            artwork_id: Some("artwork1".to_string()),
            lyrics_status: "available".to_string(),
        };

        assert_eq!(response.track.id, "track1");
        assert!(response.artwork_fetched);
        assert!(response.lyrics_fetched);
        assert_eq!(response.artwork_id, Some("artwork1".to_string()));
        assert_eq!(response.lyrics_status, "available");
    }
}
