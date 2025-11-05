//! Last.fm API Client
//!
//! Provides integration with the Last.fm API for fetching album artwork.
//!
//! ## API Endpoints
//!
//! - **Album Info**: `https://ws.audioscrobbler.com/2.0/?method=album.getinfo&api_key={key}&artist={artist}&album={album}&format=json`
//!
//! ## Rate Limiting
//!
//! Last.fm API rate limits:
//! - Free tier: Varies, generally permissive (several requests per second)
//! - We apply conservative 1 request/second to be respectful
//!
//! ## API Key Requirement
//!
//! Last.fm requires an API key for all requests.
//! Obtain one at: https://www.last.fm/api/account/create
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::providers::lastfm::LastFmClient;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = LastFmClient::new(
//!     http_client,
//!     "your_api_key".to_string(),
//!     1000, // 1 request per second
//! );
//!
//! let artwork = client.fetch_artwork("The Beatles", "Abbey Road").await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{MetadataError, Result};
use bridge_traits::http::{HttpClient, HttpMethod, HttpRequest};
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Last.fm API base URL
const LASTFM_API_BASE: &str = "https://ws.audioscrobbler.com/2.0/";

/// Timeout for API requests
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Last.fm API client
///
/// Handles fetching album information and artwork from Last.fm.
/// Implements automatic rate limiting to be respectful to the API.
pub struct LastFmClient {
    http_client: Arc<dyn HttpClient>,
    api_key: String,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

/// Simple rate limiter to enforce delay between requests
struct RateLimiter {
    last_request: Option<Instant>,
    min_delay: Duration,
}

impl RateLimiter {
    fn new(delay_ms: u64) -> Self {
        Self {
            last_request: None,
            min_delay: Duration::from_millis(delay_ms),
        }
    }

    async fn wait_if_needed(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_delay {
                let wait_time = self.min_delay - elapsed;
                debug!("Rate limiting: waiting {:?}", wait_time);
                tokio::time::sleep(wait_time).await;
            }
        }
        self.last_request = Some(Instant::now());
    }
}

/// Last.fm album image
#[derive(Debug, Clone, Deserialize)]
struct AlbumImage {
    #[serde(rename = "#text")]
    url: String,
    size: String,
}

/// Last.fm album info
#[derive(Debug, Deserialize)]
struct AlbumInfo {
    #[serde(default)]
    image: Vec<AlbumImage>,
}

/// Last.fm API response wrapper
#[derive(Debug, Deserialize)]
struct AlbumResponse {
    album: Option<AlbumInfo>,
}

/// Last.fm error response
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: i32,
    message: String,
}

impl LastFmClient {
    /// Creates a new Last.fm API client
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for making requests
    /// * `api_key` - Last.fm API key
    /// * `rate_limit_delay_ms` - Minimum delay between requests in milliseconds
    ///
    /// # Example
    ///
    /// ```ignore
    /// let client = LastFmClient::new(
    ///     http_client,
    ///     "your_api_key".to_string(),
    ///     1000,
    /// );
    /// ```
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        api_key: String,
        rate_limit_delay_ms: u64,
    ) -> Self {
        Self {
            http_client,
            api_key,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(rate_limit_delay_ms))),
        }
    }

    /// Fetches artwork for an album
    ///
    /// Queries the Last.fm album.getInfo API to retrieve album information,
    /// then downloads the highest quality artwork image available.
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist name
    /// * `album` - Album name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Bytes))` - Artwork image data
    /// - `Ok(None)` - No artwork found
    /// - `Err` - API error or network failure
    ///
    /// # Example
    ///
    /// ```ignore
    /// let artwork = client.fetch_artwork("The Beatles", "Abbey Road").await?;
    /// if let Some(image_data) = artwork {
    ///     // Process image_data
    /// }
    /// ```
    pub async fn fetch_artwork(&self, artist: &str, album: &str) -> Result<Option<Bytes>> {
        // Get album info with image URLs
        let image_url = match self.get_album_info(artist, album).await? {
            Some(url) => {
                info!("Found artwork URL for '{} - {}' on Last.fm", artist, album);
                url
            }
            None => {
                info!(
                    "No artwork found for '{} - {}' on Last.fm",
                    artist, album
                );
                return Ok(None);
            }
        };

        // Download the image
        self.download_image(&image_url).await
    }

    /// Gets album information from Last.fm
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist name
    /// * `album` - Album name
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - URL of the highest quality artwork
    /// - `Ok(None)` - No album found or no artwork available
    /// - `Err` - API error
    async fn get_album_info(&self, artist: &str, album: &str) -> Result<Option<String>> {
        // Build API URL
        let url = format!(
            "{}?method=album.getinfo&api_key={}&artist={}&album={}&format=json",
            LASTFM_API_BASE,
            urlencoding::encode(&self.api_key),
            urlencoding::encode(artist),
            urlencoding::encode(album)
        );

        debug!("Querying Last.fm: album.getinfo for '{} - {}'", artist, album);

        // Apply rate limiting
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let request = HttpRequest::new(HttpMethod::Get, url)
            .header("User-Agent", "MusicPlatformCore/1.0")
            .header("Accept", "application/json")
            .timeout(REQUEST_TIMEOUT);

        let response = self
            .http_client
            .execute(request)
            .await
            .map_err(|e| MetadataError::NetworkError(format!("Last.fm request failed: {}", e)))?;

        // Check status
        if !response.is_success() {
            if response.status == 429 {
                // Rate limited
                let retry_after = response
                    .headers
                    .get("Retry-After")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60);

                return Err(MetadataError::RateLimited {
                    provider: "Last.fm".to_string(),
                    retry_after_seconds: retry_after,
                });
            }

            return Err(MetadataError::HttpError {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).to_string(),
            });
        }

        // Check for API error in response body
        if let Ok(error_resp) = serde_json::from_slice::<ErrorResponse>(&response.body) {
            match error_resp.error {
                6 => {
                    // Album not found
                    debug!("Album not found on Last.fm: '{} - {}'", artist, album);
                    return Ok(None);
                }
                _ => {
                    return Err(MetadataError::RemoteApi(format!(
                        "Last.fm API error {}: {}",
                        error_resp.error, error_resp.message
                    )));
                }
            }
        }

        // Parse successful response
        let album_response: AlbumResponse = serde_json::from_slice(&response.body)
            .map_err(|e| MetadataError::JsonParse(format!("Failed to parse Last.fm response: {}", e)))?;

        // Extract artwork URL
        // Prefer sizes in order: mega > extralarge > large > medium > small
        let artwork_url = album_response
            .album
            .and_then(|album| {
                album
                    .image
                    .iter()
                    .find(|img| img.size == "mega")
                    .or_else(|| album.image.iter().find(|img| img.size == "extralarge"))
                    .or_else(|| album.image.iter().find(|img| img.size == "large"))
                    .or_else(|| album.image.iter().find(|img| img.size == "medium"))
                    .or_else(|| album.image.first())
                    .map(|img| img.url.clone())
            })
            .filter(|url| !url.is_empty());

        Ok(artwork_url)
    }

    /// Downloads an image from a URL
    ///
    /// # Arguments
    ///
    /// * `url` - Image URL
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Bytes))` - Image data
    /// - `Ok(None)` - Download failed or empty response
    /// - `Err` - Network error
    async fn download_image(&self, url: &str) -> Result<Option<Bytes>> {
        debug!("Downloading image from: {}", url);

        // Apply rate limiting (even for CDN requests to be respectful)
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let request = HttpRequest::new(HttpMethod::Get, url.to_string())
            .header("User-Agent", "MusicPlatformCore/1.0")
            .timeout(REQUEST_TIMEOUT);

        let response = self
            .http_client
            .execute(request)
            .await
            .map_err(|e| MetadataError::NetworkError(format!("Image download failed: {}", e)))?;

        // Check status
        if !response.is_success() {
            warn!("Failed to download image from {}: status {}", url, response.status);
            return Ok(None);
        }

        // Return image data if non-empty
        if response.body.is_empty() {
            Ok(None)
        } else {
            Ok(Some(response.body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(100);
        assert!(limiter.last_request.is_none());

        // Simulate first request
        limiter.last_request = Some(Instant::now());
        assert!(limiter.last_request.is_some());
    }
}
