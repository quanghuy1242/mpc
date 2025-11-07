//! MusicBrainz API Client
//!
//! Provides integration with the MusicBrainz API and Cover Art Archive for fetching album artwork.
//!
//! ## API Endpoints
//!
//! - **Search**: `https://musicbrainz.org/ws/2/release-group/?query={query}&fmt=json`
//! - **Cover Art**: `https://coverartarchive.org/release-group/{mbid}/front`
//!
//! ## Rate Limiting
//!
//! MusicBrainz API enforces rate limiting:
//! - Anonymous clients: 1 request/second
//! - Identified clients (with User-Agent): 1 request/second
//! - Authenticated clients (MusicBrainz account): 5 requests/second (not implemented)
//!
//! The client automatically enforces rate limiting to comply with API terms.
//!
//! ## User Agent Requirement
//!
//! MusicBrainz requires all API clients to identify themselves with a proper User-Agent header:
//! Format: "ApplicationName/Version (ContactEmail)"
//! Example: "MyMusicApp/1.0 (contact@example.com)"
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::providers::musicbrainz::MusicBrainzClient;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = MusicBrainzClient::new(
//!     http_client,
//!     "MyMusicApp/1.0 (contact@example.com)".to_string(),
//!     1000, // 1 request per second
//! );
//!
//! let artwork = client.fetch_cover_art("The Beatles", "Abbey Road", None).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{MetadataError, Result};
use bridge_traits::http::{HttpClient, HttpMethod, HttpRequest};
use bridge_traits::time::{Clock, SystemClock};
use bytes::Bytes;
use core_async::sync::Mutex;
use core_async::time::sleep;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// MusicBrainz API base URL
const MUSICBRAINZ_API_BASE: &str = "https://musicbrainz.org/ws/2";

/// Cover Art Archive base URL
const COVERART_ARCHIVE_BASE: &str = "https://coverartarchive.org";

/// Maximum number of search results to retrieve
const MAX_SEARCH_RESULTS: u32 = 5;

/// Timeout for API requests
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// MusicBrainz API client
///
/// Handles searching for releases and fetching cover artwork from the Cover Art Archive.
/// Implements automatic rate limiting to comply with MusicBrainz API terms.
pub struct MusicBrainzClient {
    http_client: Arc<dyn HttpClient>,
    user_agent: String,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

/// Simple rate limiter to enforce delay between requests
struct RateLimiter {
    clock: Arc<dyn Clock>,
    last_request_ms: Option<i64>,
    min_delay: Duration,
}

impl RateLimiter {
    fn new(delay_ms: u64, clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            last_request_ms: None,
            min_delay: Duration::from_millis(delay_ms),
        }
    }

    async fn wait_if_needed(&mut self) {
        if let Some(last) = self.last_request_ms {
            let now = self.clock.unix_timestamp_millis();
            let elapsed_ms = now - last;
            let required_ms = self.min_delay.as_millis() as i64;
            if elapsed_ms < required_ms {
                let wait_ms = (required_ms - elapsed_ms) as u64;
                let wait_time = Duration::from_millis(wait_ms);
                debug!("Rate limiting: waiting {:?}", wait_time);
                sleep(wait_time).await;
            }
        }
        self.last_request_ms = Some(self.clock.unix_timestamp_millis());
    }
}

/// MusicBrainz release group search result
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)] // Fields used for future features
struct ReleaseGroup {
    id: String,
    title: String,
    #[serde(default)]
    primary_type: Option<String>,
    #[serde(default)]
    first_release_date: Option<String>,
}

/// MusicBrainz search response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct SearchResponse {
    #[serde(default)]
    release_groups: Vec<ReleaseGroup>,
}

impl MusicBrainzClient {
    /// Creates a new MusicBrainz API client
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for making requests
    /// * `user_agent` - User agent string (format: "AppName/Version (Contact)")
    /// * `rate_limit_delay_ms` - Minimum delay between requests in milliseconds
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = MusicBrainzClient::new(
    ///     http_client,
    ///     "MyMusicApp/1.0 (contact@example.com)".to_string(),
    ///     1000,
    /// );
    /// ```
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        user_agent: String,
        rate_limit_delay_ms: u64,
    ) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Self::with_clock(http_client, user_agent, rate_limit_delay_ms, clock)
    }

    pub fn with_clock(
        http_client: Arc<dyn HttpClient>,
        user_agent: String,
        rate_limit_delay_ms: u64,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            http_client,
            user_agent,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(rate_limit_delay_ms, clock))),
        }
    }

    /// Fetches cover art for an album
    ///
    /// Searches for the release group by artist and album name, then attempts to
    /// download the front cover from the Cover Art Archive.
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist name
    /// * `album` - Album name
    /// * `mbid` - Optional MusicBrainz ID (if known, bypasses search)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Bytes))` - Cover art image data
    /// - `Ok(None)` - No cover art found
    /// - `Err` - API error or network failure
    ///
    /// # Example
    ///
    /// ```ignore
    /// let artwork = client.fetch_cover_art("The Beatles", "Abbey Road", None).await?;
    /// if let Some(image_data) = artwork {
    ///     // Process image_data
    /// }
    /// ```
    pub async fn fetch_cover_art(
        &self,
        artist: &str,
        album: &str,
        mbid: Option<&str>,
    ) -> Result<Option<Bytes>> {
        // If MBID is provided, use it directly
        let release_group_id = if let Some(id) = mbid {
            info!("Using provided MBID {} for '{} - {}'", id, artist, album);
            id.to_string()
        } else {
            // Search for release group
            match self.search_release_group(artist, album).await? {
                Some(id) => {
                    info!("Found release group {} for '{} - {}'", id, artist, album);
                    id
                }
                None => {
                    info!(
                        "No release group found for '{} - {}' on MusicBrainz",
                        artist, album
                    );
                    return Ok(None);
                }
            }
        };

        // Fetch cover art from Cover Art Archive
        self.fetch_cover_art_by_mbid(&release_group_id).await
    }

    /// Searches for a release group by artist and album name
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist name
    /// * `album` - Album name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - MusicBrainz release group ID
    /// - `Ok(None)` - No matching release found
    /// - `Err` - API error
    async fn search_release_group(&self, artist: &str, album: &str) -> Result<Option<String>> {
        // Build search query
        // Use lucene query syntax: artist:"..." AND releasegroup:"..."
        let query = format!(
            "artist:\"{}\" AND releasegroup:\"{}\"",
            Self::escape_query(artist),
            Self::escape_query(album)
        );

        let encoded_query = urlencoding::encode(&query);
        let url = format!(
            "{}/release-group/?query={}&fmt=json&limit={}",
            MUSICBRAINZ_API_BASE, encoded_query, MAX_SEARCH_RESULTS
        );

        debug!("Searching MusicBrainz: {}", url);

        // Apply rate limiting
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let request = HttpRequest::new(HttpMethod::Get, url)
            .header("User-Agent", &self.user_agent)
            .header("Accept", "application/json")
            .timeout(REQUEST_TIMEOUT);

        let response = self.http_client.execute(request).await.map_err(|e| {
            MetadataError::NetworkError(format!("MusicBrainz search failed: {}", e))
        })?;

        // Check status
        if !response.is_success() {
            if response.status == 503 {
                // Service unavailable - rate limited or maintenance
                warn!("MusicBrainz service unavailable (503)");
                return Ok(None);
            }

            return Err(MetadataError::HttpError {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).to_string(),
            });
        }

        // Parse response
        let search_result: SearchResponse =
            serde_json::from_slice(&response.body).map_err(|e| {
                MetadataError::JsonParse(format!("Failed to parse search results: {}", e))
            })?;

        // Return first result (best match)
        // Prefer "Album" type over others
        let best_match = search_result
            .release_groups
            .iter()
            .find(|rg| rg.primary_type.as_deref() == Some("Album"))
            .or_else(|| search_result.release_groups.first());

        Ok(best_match.map(|rg| rg.id.clone()))
    }

    /// Fetches cover art from Cover Art Archive by release group MBID
    ///
    /// # Arguments
    ///
    /// * `mbid` - MusicBrainz release group ID
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Bytes))` - Cover art image data
    /// - `Ok(None)` - No cover art available
    /// - `Err` - API error
    async fn fetch_cover_art_by_mbid(&self, mbid: &str) -> Result<Option<Bytes>> {
        // Cover Art Archive URL for front cover
        let url = format!("{}/release-group/{}/front", COVERART_ARCHIVE_BASE, mbid);

        debug!("Fetching cover art: {}", url);

        // Apply rate limiting (Cover Art Archive shares rate limits with MusicBrainz)
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let request = HttpRequest::new(HttpMethod::Get, url)
            .header("User-Agent", &self.user_agent)
            .timeout(REQUEST_TIMEOUT);

        let response =
            self.http_client.execute(request).await.map_err(|e| {
                MetadataError::NetworkError(format!("Cover art fetch failed: {}", e))
            })?;

        // Check status
        match response.status {
            200 => Ok(Some(response.body)),
            404 => {
                debug!("No cover art available for MBID {}", mbid);
                Ok(None)
            }
            503 => {
                warn!("Cover Art Archive service unavailable (503)");
                Ok(None)
            }
            429 => {
                // Rate limited
                let retry_after = response
                    .headers
                    .get("Retry-After")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60);

                Err(MetadataError::RateLimited {
                    provider: "MusicBrainz".to_string(),
                    retry_after_seconds: retry_after,
                })
            }
            _ => Err(MetadataError::HttpError {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).to_string(),
            }),
        }
    }

    /// Escapes special characters in Lucene query syntax
    fn escape_query(s: &str) -> String {
        // Escape special Lucene characters: + - && || ! ( ) { } [ ] ^ " ~ * ? : \ / .
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('+', "\\+")
            .replace('-', "\\-")
            .replace('!', "\\!")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('{', "\\{")
            .replace('}', "\\}")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('^', "\\^")
            .replace('~', "\\~")
            .replace('*', "\\*")
            .replace('?', "\\?")
            .replace(':', "\\:")
            .replace('/', "\\/")
            .replace('.', "\\.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::time::{Clock, SystemClock};
    use std::sync::Arc;

    #[test]
    fn test_escape_query() {
        assert_eq!(MusicBrainzClient::escape_query("AC/DC"), "AC\\/DC");
        assert_eq!(
            MusicBrainzClient::escape_query("Artist (feat. Other)"),
            "Artist \\(feat\\. Other\\)"
        );
        assert_eq!(
            MusicBrainzClient::escape_query("Album: Title"),
            "Album\\: Title"
        );
    }

    #[test]
    fn test_rate_limiter() {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let mut limiter = RateLimiter::new(100, Arc::clone(&clock));
        assert!(limiter.last_request_ms.is_none());

        // Simulate first request
        limiter.last_request_ms = Some(clock.unix_timestamp_millis());
        assert!(limiter.last_request_ms.is_some());
    }
}
