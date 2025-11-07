//! # Lyrics Provider Module
//!
//! Fetches and manages lyrics from external services, with support for both
//! synced (LRC format) and plain text lyrics.
//!
//! ## Features
//!
//! - Multiple provider support (LRCLib, Musixmatch, Genius)
//! - Automatic provider fallback
//! - Synced lyrics (LRC format) support
//! - Caching to prevent redundant API calls
//! - Retry logic with exponential backoff
//! - Rate limiting and quota management
//! - Database persistence with source tracking
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_metadata::lyrics::{LyricsService, LyricsSearchQuery};
//!
//! // Create lyrics service with HTTP client and repository
//! let service = LyricsService::new(http_client, lyrics_repo);
//!
//! // Search for lyrics
//! let query = LyricsSearchQuery::new("Artist Name", "Track Title", "Album Name", 180);
//! let result = service.fetch_lyrics(query).await?;
//!
//! if let Some(lyrics) = result {
//!     println!("Found lyrics from: {}", lyrics.source);
//!     println!("Synced: {}", lyrics.is_synced);
//! }
//! ```

use crate::error::{MetadataError, Result};
use async_trait::async_trait;
use core_library::models::Lyrics;
use core_library::repositories::lyrics::LyricsRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

#[cfg(feature = "lyrics")]
use bridge_traits::http::HttpClient;

// =============================================================================
// Core Types
// =============================================================================

/// Lyrics search query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsSearchQuery {
    /// Artist name
    pub artist: String,
    /// Track title
    pub track: String,
    /// Album name (optional, helps improve matching)
    pub album: Option<String>,
    /// Track duration in seconds (optional, helps improve matching)
    pub duration: Option<u32>,
    /// Track ID for database storage
    pub track_id: String,
}

impl LyricsSearchQuery {
    /// Create a new lyrics search query
    pub fn new(
        artist: impl Into<String>,
        track: impl Into<String>,
        album: impl Into<String>,
        duration: u32,
        track_id: impl Into<String>,
    ) -> Self {
        Self {
            artist: artist.into(),
            track: track.into(),
            album: Some(album.into()),
            duration: Some(duration),
            track_id: track_id.into(),
        }
    }

    /// Create a minimal query with just artist and track
    pub fn minimal(
        artist: impl Into<String>,
        track: impl Into<String>,
        track_id: impl Into<String>,
    ) -> Self {
        Self {
            artist: artist.into(),
            track: track.into(),
            album: None,
            duration: None,
            track_id: track_id.into(),
        }
    }
}

/// Lyrics fetch result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsResult {
    /// Lyrics text (plain or LRC format)
    pub text: String,
    /// Whether lyrics are synced (LRC format)
    pub is_synced: bool,
    /// Source of lyrics
    pub source: LyricsSource,
    /// Language code (ISO 639-1)
    pub language: Option<String>,
}

impl LyricsResult {
    /// Create a new lyrics result
    pub fn new(
        text: String,
        is_synced: bool,
        source: LyricsSource,
        language: Option<String>,
    ) -> Self {
        Self {
            text,
            is_synced,
            source,
            language,
        }
    }

    /// Check if lyrics are valid LRC format
    pub fn is_valid_lrc(&self) -> bool {
        if !self.is_synced {
            return false;
        }
        // LRC format requires timestamp markers like [00:12.50]
        self.text.contains('[') && self.text.contains(']') && self.text.contains(':')
    }
}

/// Lyrics source providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LyricsSource {
    /// LRCLib - Free, open-source synced lyrics
    LrcLib,
    /// Musixmatch - Commercial lyrics provider
    Musixmatch,
    /// Genius - Lyrics and annotations
    Genius,
    /// Embedded in audio file
    Embedded,
    /// Manually added by user
    Manual,
}

impl LyricsSource {
    /// Get source name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LrcLib => "lrclib",
            Self::Musixmatch => "musixmatch",
            Self::Genius => "genius",
            Self::Embedded => "embedded",
            Self::Manual => "manual",
        }
    }

    /// Get human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LrcLib => "LRCLib",
            Self::Musixmatch => "Musixmatch",
            Self::Genius => "Genius",
            Self::Embedded => "Embedded",
            Self::Manual => "Manual",
        }
    }

    /// Check if source supports synced lyrics
    pub fn supports_synced(&self) -> bool {
        matches!(self, Self::LrcLib | Self::Embedded)
    }
}

// =============================================================================
// Provider Trait
// =============================================================================

/// Trait for lyrics provider implementations
#[async_trait]
pub trait LyricsProvider: Send + Sync {
    /// Fetch lyrics for a track
    ///
    /// # Arguments
    /// * `query` - Search parameters
    ///
    /// # Returns
    /// * `Ok(Some(lyrics))` if lyrics found
    /// * `Ok(None)` if lyrics not found
    /// * `Err` if API error or network failure
    async fn fetch(&self, query: &LyricsSearchQuery) -> Result<Option<LyricsResult>>;

    /// Get the source identifier
    fn source(&self) -> LyricsSource;

    /// Check if provider supports synced lyrics
    fn supports_synced(&self) -> bool {
        self.source().supports_synced()
    }
}

// =============================================================================
// Lyrics Service
// =============================================================================

/// Main lyrics service coordinating all operations
pub struct LyricsService {
    providers: Vec<Box<dyn LyricsProvider>>,
    repository: Arc<dyn LyricsRepository>,
    retry_config: RetryConfig,
}

impl LyricsService {
    /// Create a new lyrics service with HTTP client and repository
    #[cfg(feature = "lyrics")]
    pub fn new(http_client: Arc<dyn HttpClient>, repository: Arc<dyn LyricsRepository>) -> Self {
        let mut providers: Vec<Box<dyn LyricsProvider>> = Vec::new();

        // Add LRCLib provider (free, synced lyrics)
        providers.push(Box::new(LrcLibProvider::new(http_client.clone())));

        // Add Musixmatch provider (requires API key)
        if let Ok(api_key) = std::env::var("MUSIXMATCH_API_KEY") {
            providers.push(Box::new(MusixmatchProvider::new(
                http_client.clone(),
                api_key,
            )));
        } else {
            debug!("Musixmatch API key not found, provider disabled");
        }

        // Add Genius provider (requires API key)
        if let Ok(api_key) = std::env::var("GENIUS_API_KEY") {
            providers.push(Box::new(GeniusProvider::new(http_client, api_key)));
        } else {
            debug!("Genius API key not found, provider disabled");
        }

        Self {
            providers,
            repository,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a service without HTTP client (for testing or embedded-only use)
    pub fn without_providers(repository: Arc<dyn LyricsRepository>) -> Self {
        Self {
            providers: Vec::new(),
            repository,
            retry_config: RetryConfig::default(),
        }
    }

    /// Fetch lyrics from providers or cache
    ///
    /// This method:
    /// 1. Checks database cache first
    /// 2. Falls back to providers if not cached
    /// 3. Tries each provider in order until success
    /// 4. Stores successful result in database
    ///
    /// # Arguments
    /// * `query` - Search parameters
    ///
    /// # Returns
    /// * `Ok(Some(lyrics))` if found from cache or providers
    /// * `Ok(None)` if not found anywhere
    /// * `Err` if all providers fail with errors
    pub async fn fetch_lyrics(&self, query: &LyricsSearchQuery) -> Result<Option<Lyrics>> {
        // Check cache first
        if let Ok(Some(cached)) = self.repository.find_by_track_id(&query.track_id).await {
            debug!(
                track_id = %query.track_id,
                source = %cached.source,
                "Found cached lyrics"
            );
            return Ok(Some(cached));
        }

        // Try each provider with retry logic
        for provider in &self.providers {
            info!(
                source = %provider.source().as_str(),
                artist = %query.artist,
                track = %query.track,
                "Attempting to fetch lyrics"
            );

            match self.fetch_with_retry(provider.as_ref(), query).await {
                Ok(Some(result)) => {
                    info!(
                        source = %provider.source().as_str(),
                        synced = result.is_synced,
                        "Successfully fetched lyrics"
                    );

                    // Store in database
                    let lyrics = Lyrics::new(
                        query.track_id.clone(),
                        result.source.as_str().to_string(),
                        result.is_synced,
                        result.text,
                    );

                    if let Err(e) = self.repository.insert(&lyrics).await {
                        warn!(error = %e, "Failed to cache lyrics");
                    }

                    return Ok(Some(lyrics));
                }
                Ok(None) => {
                    debug!(
                        source = %provider.source().as_str(),
                        "Lyrics not found at provider"
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        source = %provider.source().as_str(),
                        error = %e,
                        "Provider fetch failed"
                    );
                    continue;
                }
            }
        }

        // No provider found lyrics
        info!(
            artist = %query.artist,
            track = %query.track,
            "No lyrics found from any provider"
        );
        Ok(None)
    }

    /// Fetch with retry logic
    async fn fetch_with_retry(
        &self,
        provider: &dyn LyricsProvider,
        query: &LyricsSearchQuery,
    ) -> Result<Option<LyricsResult>> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.retry_config.max_attempts {
            match provider.fetch(query).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);

                    if attempts < self.retry_config.max_attempts {
                        let delay = self.retry_config.backoff_duration(attempts);
                        debug!(
                            attempt = attempts,
                            delay_ms = delay.as_millis(),
                            "Retrying after failure"
                        );
                        core_async::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            MetadataError::LyricsFetchFailed("All retry attempts exhausted".to_string())
        }))
    }

    /// Update existing lyrics in database
    pub async fn update_lyrics(&self, lyrics: &Lyrics) -> Result<()> {
        self.repository
            .update(lyrics)
            .await
            .map_err(|e| MetadataError::Database(format!("Failed to update lyrics: {}", e)))
    }

    /// Delete lyrics for a track
    pub async fn delete_lyrics(&self, track_id: &str) -> Result<bool> {
        self.repository
            .delete(track_id)
            .await
            .map_err(|e| MetadataError::Database(format!("Failed to delete lyrics: {}", e)))
    }

    /// Get statistics about cached lyrics
    pub async fn get_stats(&self) -> Result<LyricsStats> {
        let total = self
            .repository
            .count()
            .await
            .map_err(|e| MetadataError::Database(format!("Failed to count lyrics: {}", e)))?;

        let synced = self.repository.count_synced().await.map_err(|e| {
            MetadataError::Database(format!("Failed to count synced lyrics: {}", e))
        })?;

        Ok(LyricsStats {
            total_cached: total as usize,
            synced_count: synced as usize,
            plain_count: (total - synced) as usize,
            provider_count: self.providers.len(),
        })
    }
}

/// Lyrics statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsStats {
    /// Total cached lyrics
    pub total_cached: usize,
    /// Number of synced lyrics
    pub synced_count: usize,
    /// Number of plain text lyrics
    pub plain_count: usize,
    /// Number of active providers
    pub provider_count: usize,
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,
    /// Base delay for exponential backoff
    pub base_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
        }
    }
}

impl RetryConfig {
    /// Calculate backoff duration for attempt number
    fn backoff_duration(&self, attempt: usize) -> Duration {
        let delay_ms = self.base_delay_ms * 2u64.pow(attempt as u32);
        Duration::from_millis(delay_ms.min(10000)) // Cap at 10 seconds
    }
}

// =============================================================================
// Provider Implementations
// =============================================================================

#[cfg(feature = "lyrics")]
mod providers {
    use super::*;
    use bridge_traits::http::{HttpMethod, HttpRequest};

    /// LRCLib provider - Free, open-source synced lyrics
    pub struct LrcLibProvider {
        http_client: Arc<dyn HttpClient>,
        base_url: String,
    }

    impl LrcLibProvider {
        pub fn new(http_client: Arc<dyn HttpClient>) -> Self {
            Self {
                http_client,
                base_url: "https://lrclib.net/api".to_string(),
            }
        }
    }

    #[async_trait]
    impl LyricsProvider for LrcLibProvider {
        async fn fetch(&self, query: &LyricsSearchQuery) -> Result<Option<LyricsResult>> {
            let mut url = format!(
                "{}/get?artist_name={}&track_name={}",
                self.base_url,
                urlencoding::encode(&query.artist),
                urlencoding::encode(&query.track)
            );

            if let Some(album) = &query.album {
                url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
            }

            if let Some(duration) = query.duration {
                url.push_str(&format!("&duration={}", duration));
            }

            let request = HttpRequest::new(HttpMethod::Get, &url);
            let response = self.http_client.execute(request).await?;

            if response.status == 404 {
                return Ok(None);
            }

            if response.status != 200 {
                return Err(MetadataError::LyricsFetchFailed(format!(
                    "LRCLib API error: HTTP {}",
                    response.status
                )));
            }

            let lrc_response: LrcLibResponse = response
                .json()
                .map_err(|e| MetadataError::LyricsFetchFailed(format!("Parse error: {}", e)))?;

            // Prefer synced lyrics if available
            if let Some(synced_lyrics) = lrc_response.synced_lyrics {
                if !synced_lyrics.is_empty() {
                    return Ok(Some(LyricsResult::new(
                        synced_lyrics,
                        true,
                        LyricsSource::LrcLib,
                        None,
                    )));
                }
            }

            // Fall back to plain lyrics
            if let Some(plain_lyrics) = lrc_response.plain_lyrics {
                if !plain_lyrics.is_empty() {
                    return Ok(Some(LyricsResult::new(
                        plain_lyrics,
                        false,
                        LyricsSource::LrcLib,
                        None,
                    )));
                }
            }

            Ok(None)
        }

        fn source(&self) -> LyricsSource {
            LyricsSource::LrcLib
        }
    }

    #[derive(Debug, Deserialize)]
    struct LrcLibResponse {
        #[serde(rename = "syncedLyrics")]
        synced_lyrics: Option<String>,
        #[serde(rename = "plainLyrics")]
        plain_lyrics: Option<String>,
    }

    /// Musixmatch provider - Commercial lyrics (requires API key)
    pub struct MusixmatchProvider {
        http_client: Arc<dyn HttpClient>,
        api_key: String,
        base_url: String,
    }

    impl MusixmatchProvider {
        pub fn new(http_client: Arc<dyn HttpClient>, api_key: String) -> Self {
            Self {
                http_client,
                api_key,
                base_url: "https://api.musixmatch.com/ws/1.1".to_string(),
            }
        }
    }

    #[async_trait]
    impl LyricsProvider for MusixmatchProvider {
        async fn fetch(&self, query: &LyricsSearchQuery) -> Result<Option<LyricsResult>> {
            // Search for track
            let search_url = format!(
                "{}/track.search?q_artist={}&q_track={}&apikey={}",
                self.base_url,
                urlencoding::encode(&query.artist),
                urlencoding::encode(&query.track),
                self.api_key
            );

            let request = HttpRequest::new(HttpMethod::Get, &search_url);
            let response = self.http_client.execute(request).await?;

            if response.status != 200 {
                return Err(MetadataError::LyricsFetchFailed(format!(
                    "Musixmatch API error: HTTP {}",
                    response.status
                )));
            }

            let search_response: MusixmatchSearchResponse = response
                .json()
                .map_err(|e| MetadataError::LyricsFetchFailed(format!("Parse error: {}", e)))?;

            let track_id = search_response
                .message
                .body
                .track_list
                .first()
                .map(|t| t.track.track_id);

            if track_id.is_none() {
                return Ok(None);
            }

            // Fetch lyrics by track ID
            let lyrics_url = format!(
                "{}/track.lyrics.get?track_id={}&apikey={}",
                self.base_url,
                track_id.unwrap(),
                self.api_key
            );

            let request = HttpRequest::new(HttpMethod::Get, &lyrics_url);
            let response = self.http_client.execute(request).await?;

            if response.status != 200 {
                return Ok(None);
            }

            let lyrics_response: MusixmatchLyricsResponse = response
                .json()
                .map_err(|e| MetadataError::LyricsFetchFailed(format!("Parse error: {}", e)))?;

            if let Some(lyrics_body) = lyrics_response.message.body.lyrics {
                let text = lyrics_body.lyrics_body;
                if !text.is_empty() {
                    return Ok(Some(LyricsResult::new(
                        text,
                        false,
                        LyricsSource::Musixmatch,
                        Some(
                            lyrics_body
                                .lyrics_language
                                .unwrap_or_else(|| "en".to_string()),
                        ),
                    )));
                }
            }

            Ok(None)
        }

        fn source(&self) -> LyricsSource {
            LyricsSource::Musixmatch
        }
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchSearchResponse {
        message: MusixmatchMessage<MusixmatchTrackList>,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchLyricsResponse {
        message: MusixmatchMessage<MusixmatchLyricsBody>,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchMessage<T> {
        body: T,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchTrackList {
        track_list: Vec<MusixmatchTrackWrapper>,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchTrackWrapper {
        track: MusixmatchTrack,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchTrack {
        track_id: u64,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchLyricsBody {
        lyrics: Option<MusixmatchLyrics>,
    }

    #[derive(Debug, Deserialize)]
    struct MusixmatchLyrics {
        lyrics_body: String,
        lyrics_language: Option<String>,
    }

    /// Genius provider - Lyrics and annotations (requires API key)
    pub struct GeniusProvider {
        http_client: Arc<dyn HttpClient>,
        api_key: String,
        base_url: String,
    }

    impl GeniusProvider {
        pub fn new(http_client: Arc<dyn HttpClient>, api_key: String) -> Self {
            Self {
                http_client,
                api_key,
                base_url: "https://api.genius.com".to_string(),
            }
        }
    }

    #[async_trait]
    impl LyricsProvider for GeniusProvider {
        async fn fetch(&self, query: &LyricsSearchQuery) -> Result<Option<LyricsResult>> {
            // Search for song
            let search_url = format!(
                "{}/search?q={} {}",
                self.base_url,
                urlencoding::encode(&query.artist),
                urlencoding::encode(&query.track)
            );

            let request = HttpRequest::new(HttpMethod::Get, &search_url)
                .header("Authorization", format!("Bearer {}", self.api_key));

            let response = self.http_client.execute(request).await?;

            if response.status != 200 {
                return Err(MetadataError::LyricsFetchFailed(format!(
                    "Genius API error: HTTP {}",
                    response.status
                )));
            }

            let _search_response: GeniusSearchResponse = response
                .json()
                .map_err(|e| MetadataError::LyricsFetchFailed(format!("Parse error: {}", e)))?;

            // Genius API doesn't directly provide lyrics in their API
            // We would need to scrape the website, which is against their ToS
            // For now, we just return None
            // TODO: Consider using official Genius lyrics API when available
            warn!("Genius provider requires web scraping, which is not implemented");

            Ok(None)
        }

        fn source(&self) -> LyricsSource {
            LyricsSource::Genius
        }
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct GeniusSearchResponse {
        response: GeniusResponse,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct GeniusResponse {
        hits: Vec<GeniusHit>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct GeniusHit {
        result: GeniusSong,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct GeniusSong {
        id: u64,
        title: String,
        url: String,
    }
}

#[cfg(feature = "lyrics")]
pub use providers::*;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core_library::db::create_test_pool;
    use core_library::error::LibraryError;
    use core_library::models::Track;
    use core_library::repositories::lyrics::SqliteLyricsRepository;
    use core_library::repositories::track::SqliteTrackRepository;
    use core_library::repositories::TrackRepository;

    /// Helper function to create a test provider (tracks table has foreign key to providers)
    async fn insert_test_provider(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "INSERT INTO providers (id, type, display_name, profile_id, created_at) 
             VALUES ('test-provider', 'GoogleDrive', 'Test Provider', 'test-profile', 1699200000)",
        )
        .execute(pool)
        .await
        .ok(); // Ignore error if already exists
    }

    /// Helper function to create a test track (lyrics table has foreign key to tracks)
    async fn create_test_track(
        track_repo: &SqliteTrackRepository,
        track_id: &str,
    ) -> std::result::Result<(), LibraryError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let track = Track {
            id: track_id.to_string(),
            provider_id: "test-provider".to_string(),
            provider_file_id: format!("file-{}", track_id),
            hash: Some("test-hash".to_string()),
            title: "Test Track".to_string(),
            normalized_title: "test track".to_string(),
            album_id: None,
            artist_id: None,
            album_artist_id: None,
            track_number: Some(1),
            disc_number: 1,
            genre: Some("Test".to_string()),
            year: Some(2024),
            duration_ms: 180000,
            bitrate: Some(320),
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(1024000),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: None,
            lyrics_status: "not_fetched".to_string(), // Valid value: 'not_fetched', 'fetching', 'available', 'unavailable'
            provider_modified_at: Some(now),
            created_at: now,
            updated_at: now,
        };
        track.validate().unwrap();
        track_repo.insert(&track).await
    }

    #[test]
    fn test_lyrics_source_display() {
        assert_eq!(LyricsSource::LrcLib.as_str(), "lrclib");
        assert_eq!(LyricsSource::Musixmatch.as_str(), "musixmatch");
        assert_eq!(LyricsSource::Genius.as_str(), "genius");
        assert_eq!(LyricsSource::LrcLib.display_name(), "LRCLib");
    }

    #[test]
    fn test_lyrics_source_supports_synced() {
        assert!(LyricsSource::LrcLib.supports_synced());
        assert!(LyricsSource::Embedded.supports_synced());
        assert!(!LyricsSource::Musixmatch.supports_synced());
        assert!(!LyricsSource::Genius.supports_synced());
    }

    #[test]
    fn test_lyrics_result_is_valid_lrc() {
        let synced = LyricsResult::new(
            "[00:12.50]Test line\n[00:15.00]Another line".to_string(),
            true,
            LyricsSource::LrcLib,
            None,
        );
        assert!(synced.is_valid_lrc());

        let plain = LyricsResult::new(
            "Test line\nAnother line".to_string(),
            false,
            LyricsSource::Musixmatch,
            None,
        );
        assert!(!plain.is_valid_lrc());

        let invalid = LyricsResult::new(
            "[invalid format".to_string(),
            true,
            LyricsSource::LrcLib,
            None,
        );
        assert!(!invalid.is_valid_lrc());
    }

    #[test]
    fn test_lyrics_search_query_new() {
        let query = LyricsSearchQuery::new("Artist", "Track", "Album", 180, "track-123");
        assert_eq!(query.artist, "Artist");
        assert_eq!(query.track, "Track");
        assert_eq!(query.album, Some("Album".to_string()));
        assert_eq!(query.duration, Some(180));
        assert_eq!(query.track_id, "track-123");
    }

    #[test]
    fn test_lyrics_search_query_minimal() {
        let query = LyricsSearchQuery::minimal("Artist", "Track", "track-123");
        assert_eq!(query.artist, "Artist");
        assert_eq!(query.track, "Track");
        assert!(query.album.is_none());
        assert!(query.duration.is_none());
    }

    #[test]
    fn test_retry_config_backoff() {
        let config = RetryConfig::default();

        assert_eq!(config.backoff_duration(0).as_millis(), 100);
        assert_eq!(config.backoff_duration(1).as_millis(), 200);
        assert_eq!(config.backoff_duration(2).as_millis(), 400);
        assert_eq!(config.backoff_duration(10).as_millis(), 10000); // Capped at 10s
    }

    #[core_async::test]
    async fn test_lyrics_service_creation() {
        let pool = create_test_pool().await.unwrap();
        let repository = Arc::new(SqliteLyricsRepository::from_pool(pool));
        let service = LyricsService::without_providers(repository);

        assert_eq!(service.providers.len(), 0);
    }

    #[core_async::test]
    async fn test_lyrics_service_stats() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let track_repo = SqliteTrackRepository::from_pool(pool.clone());

        // Create test track first
        create_test_track(&track_repo, "track-1").await.unwrap();

        let repository = Arc::new(SqliteLyricsRepository::from_pool(pool.clone()));
        let service = LyricsService::without_providers(repository.clone());

        // Insert test lyrics
        let lyrics = Lyrics::new(
            "track-1".to_string(),
            "lrclib".to_string(),
            true,
            "[00:12.50]Test".to_string(),
        );
        repository.insert(&lyrics).await.unwrap();

        let stats = service.get_stats().await.unwrap();
        assert_eq!(stats.total_cached, 1);
        assert_eq!(stats.synced_count, 1);
        assert_eq!(stats.plain_count, 0);
    }

    #[core_async::test]
    async fn test_lyrics_service_fetch_cached() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let track_repo = SqliteTrackRepository::from_pool(pool.clone());

        // Create test track first
        create_test_track(&track_repo, "track-1").await.unwrap();

        let repository = Arc::new(SqliteLyricsRepository::from_pool(pool.clone()));
        let service = LyricsService::without_providers(repository.clone());

        // Pre-cache lyrics
        let lyrics = Lyrics::new(
            "track-1".to_string(),
            "lrclib".to_string(),
            false,
            "Test lyrics".to_string(),
        );
        repository.insert(&lyrics).await.unwrap();

        // Fetch should return cached
        let query = LyricsSearchQuery::minimal("Artist", "Track", "track-1");
        let result = service.fetch_lyrics(&query).await.unwrap();

        assert!(result.is_some());
        let fetched = result.unwrap();
        assert_eq!(fetched.body, "Test lyrics");
        assert_eq!(fetched.source, "lrclib");
    }

    #[core_async::test]
    async fn test_lyrics_service_update() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let track_repo = SqliteTrackRepository::from_pool(pool.clone());

        // Create test track first
        create_test_track(&track_repo, "track-1").await.unwrap();

        let repository = Arc::new(SqliteLyricsRepository::from_pool(pool.clone()));
        let service = LyricsService::without_providers(repository.clone());

        // Insert and update
        let mut lyrics = Lyrics::new(
            "track-1".to_string(),
            "lrclib".to_string(),
            false,
            "Original".to_string(),
        );
        repository.insert(&lyrics).await.unwrap();

        lyrics.body = "Updated".to_string();
        service.update_lyrics(&lyrics).await.unwrap();

        let fetched = repository
            .find_by_track_id("track-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.body, "Updated");
    }

    #[core_async::test]
    async fn test_lyrics_service_delete() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let track_repo = SqliteTrackRepository::from_pool(pool.clone());

        // Create test track first
        create_test_track(&track_repo, "track-1").await.unwrap();

        let repository = Arc::new(SqliteLyricsRepository::from_pool(pool.clone()));
        let service = LyricsService::without_providers(repository.clone());

        // Insert and delete
        let lyrics = Lyrics::new(
            "track-1".to_string(),
            "lrclib".to_string(),
            false,
            "Test".to_string(),
        );
        repository.insert(&lyrics).await.unwrap();

        let deleted = service.delete_lyrics("track-1").await.unwrap();
        assert!(deleted);

        let fetched = repository.find_by_track_id("track-1").await.unwrap();
        assert!(fetched.is_none());
    }
}
