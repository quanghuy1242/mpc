//! Artist Enrichment Provider
//!
//! Provides integration with MusicBrainz API for fetching artist biography and metadata.
//!
//! ## API Endpoints
//!
//! - **Artist Search**: `https://musicbrainz.org/ws/2/artist/?query={query}&fmt=json`
//! - **Artist Lookup**: `https://musicbrainz.org/ws/2/artist/{mbid}?inc=annotation&fmt=json`
//!
//! ## Features
//!
//! - Artist search by name with fuzzy matching
//! - Biography/annotation retrieval
//! - Country of origin information
//! - Automatic rate limiting (1 req/sec for MusicBrainz)
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::providers::artist_enrichment::ArtistEnrichmentProvider;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = ArtistEnrichmentProvider::new(
//!     http_client,
//!     "MyMusicApp/1.0 (contact@example.com)".to_string(),
//!     1000, // 1 request per second
//! );
//!
//! let metadata = provider.fetch_artist_metadata("The Beatles").await?;
//! if let Some(bio) = metadata.bio {
//!     println!("Biography: {}", bio);
//! }
//! if let Some(country) = metadata.country {
//!     println!("Country: {}", country);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{MetadataError, Result};
use bridge_traits::http::{HttpClient, HttpMethod, HttpRequest};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info};

/// MusicBrainz API base URL
const MUSICBRAINZ_API_BASE: &str = "https://musicbrainz.org/ws/2";

/// Maximum number of search results to consider
const MAX_SEARCH_RESULTS: usize = 3;

/// Timeout for API requests
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum biography length to be considered valid (in characters)
const MIN_BIO_LENGTH: usize = 50;

/// Artist metadata retrieved from external providers
#[derive(Debug, Clone)]
pub struct ArtistMetadata {
    /// Artist biography/description
    pub bio: Option<String>,
    /// Country of origin (ISO 3166-1 alpha-2 code, e.g., 'US', 'GB', 'JP')
    pub country: Option<String>,
    /// MusicBrainz artist ID (for future reference)
    pub mbid: Option<String>,
}

/// Artist enrichment provider using MusicBrainz API
pub struct ArtistEnrichmentProvider {
    http_client: Arc<dyn HttpClient>,
    user_agent: String,
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

/// MusicBrainz artist search response
#[derive(Debug, Deserialize)]
struct ArtistSearchResponse {
    artists: Vec<ArtistSearchResult>,
}

/// MusicBrainz artist search result
#[derive(Debug, Deserialize)]
struct ArtistSearchResult {
    id: String,
    name: String,
    #[serde(default)]
    score: i32,
    #[serde(rename = "country")]
    #[allow(dead_code)]
    country: Option<String>,
    #[serde(rename = "area")]
    #[allow(dead_code)]
    area: Option<Area>,
}

/// MusicBrainz area information
#[derive(Debug, Deserialize)]
struct Area {
    #[serde(rename = "iso-3166-1-codes")]
    iso_codes: Option<Vec<String>>,
}

/// MusicBrainz artist lookup response
#[derive(Debug, Deserialize)]
struct ArtistLookupResponse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    #[serde(rename = "country")]
    country: Option<String>,
    #[serde(rename = "area")]
    area: Option<Area>,
    #[serde(rename = "annotation")]
    annotation: Option<String>,
}

impl ArtistEnrichmentProvider {
    /// Create a new artist enrichment provider
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client implementation
    /// * `user_agent` - User-Agent header for MusicBrainz API (required by API terms)
    /// * `rate_limit_delay_ms` - Minimum delay between requests in milliseconds (default: 1000)
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        user_agent: String,
        rate_limit_delay_ms: u64,
    ) -> Self {
        Self {
            http_client,
            user_agent,
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(rate_limit_delay_ms))),
        }
    }

    /// Fetch artist metadata (biography, country) from MusicBrainz
    ///
    /// # Arguments
    ///
    /// * `artist_name` - Name of the artist to search for
    ///
    /// # Returns
    ///
    /// Returns `ArtistMetadata` with available information, or an error if the search fails.
    ///
    /// # Errors
    ///
    /// - `MetadataError::HttpError` - Network or HTTP error
    /// - `MetadataError::JsonParseError` - Invalid API response
    /// - `MetadataError::NotFound` - Artist not found
    pub async fn fetch_artist_metadata(&self, artist_name: &str) -> Result<ArtistMetadata> {
        info!("Fetching artist metadata for: {}", artist_name);

        // Step 1: Search for artist to get MBID
        let mbid = self.search_artist(artist_name).await?;

        // Step 2: Lookup artist details with annotation
        let metadata = self.lookup_artist(&mbid).await?;

        info!(
            "Artist metadata retrieved - MBID: {}, Bio length: {}, Country: {:?}",
            mbid,
            metadata.bio.as_ref().map(|b| b.len()).unwrap_or(0),
            metadata.country
        );

        Ok(metadata)
    }

    /// Search for an artist by name and return the best matching MBID
    async fn search_artist(&self, artist_name: &str) -> Result<String> {
        // Build search query (escape special Lucene characters)
        let query = Self::escape_lucene_query(artist_name);
        let url = format!(
            "{}/artist/?query=artist:{}&fmt=json&limit={}",
            MUSICBRAINZ_API_BASE, query, MAX_SEARCH_RESULTS
        );

        debug!("Searching for artist: {}", url);

        // Wait for rate limit
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), self.user_agent.clone());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let request = HttpRequest {
            method: HttpMethod::Get,
            url,
            headers,
            body: None,
            timeout: Some(REQUEST_TIMEOUT),
        };

        let response = self.http_client.execute(request).await.map_err(|e| {
            MetadataError::RemoteApi(format!("MusicBrainz artist search failed: {}", e))
        })?;

        if response.status != 200 {
            return Err(MetadataError::HttpError {
                status: response.status,
                body: "MusicBrainz artist search failed".to_string(),
            });
        }

        // Parse response
        let search_response: ArtistSearchResponse = serde_json::from_slice(&response.body)
            .map_err(|e| {
                MetadataError::JsonParse(format!(
                    "Failed to parse MusicBrainz search response: {}",
                    e
                ))
            })?;

        // Find best match (highest score, prefer exact name match)
        let best_match = search_response
            .artists
            .into_iter()
            .max_by_key(|artist| {
                let score = artist.score;
                let exact_match = artist.name.eq_ignore_ascii_case(artist_name);
                (exact_match, score)
            })
            .ok_or_else(|| {
                MetadataError::RemoteApi(format!("No artist found matching: {}", artist_name))
            })?;

        debug!(
            "Best match: {} (score: {}, MBID: {})",
            best_match.name, best_match.score, best_match.id
        );

        Ok(best_match.id)
    }

    /// Lookup artist details including biography (annotation)
    async fn lookup_artist(&self, mbid: &str) -> Result<ArtistMetadata> {
        let url = format!(
            "{}/artist/{}?inc=annotation&fmt=json",
            MUSICBRAINZ_API_BASE, mbid
        );

        debug!("Looking up artist details: {}", url);

        // Wait for rate limit
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Make request
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), self.user_agent.clone());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let request = HttpRequest {
            method: HttpMethod::Get,
            url,
            headers,
            body: None,
            timeout: Some(REQUEST_TIMEOUT),
        };

        let response = self.http_client.execute(request).await.map_err(|e| {
            MetadataError::RemoteApi(format!("MusicBrainz artist lookup failed: {}", e))
        })?;

        if response.status != 200 {
            return Err(MetadataError::HttpError {
                status: response.status,
                body: "MusicBrainz artist lookup failed".to_string(),
            });
        }

        // Parse response
        let lookup_response: ArtistLookupResponse = serde_json::from_slice(&response.body)
            .map_err(|e| {
                MetadataError::JsonParse(format!(
                    "Failed to parse MusicBrainz lookup response: {}",
                    e
                ))
            })?;

        // Extract country (prefer direct country field, fallback to ISO code from area)
        let country = lookup_response.country.or_else(|| {
            lookup_response
                .area
                .and_then(|area| area.iso_codes)
                .and_then(|codes| codes.first().cloned())
        });

        // Clean and validate biography
        let bio = lookup_response.annotation.and_then(Self::clean_biography);

        Ok(ArtistMetadata {
            bio,
            country,
            mbid: Some(mbid.to_string()),
        })
    }

    /// Escape special Lucene query characters
    ///
    /// MusicBrainz uses Lucene for search, so we need to escape special characters.
    fn escape_lucene_query(query: &str) -> String {
        // Characters that need escaping in Lucene queries
        const SPECIAL_CHARS: &[char] = &[
            '+', '-', '&', '|', '!', '(', ')', '{', '}', '[', ']', '^', '"', '~', '*', '?', ':',
            '\\', '/',
        ];

        query
            .chars()
            .map(|c| {
                if SPECIAL_CHARS.contains(&c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            })
            .collect()
    }

    /// Clean and validate biography text
    ///
    /// - Trims whitespace
    /// - Removes excessive newlines
    /// - Rejects very short biographies
    /// - Truncates to reasonable length
    fn clean_biography(text: String) -> Option<String> {
        // Remove excessive whitespace and newlines
        let cleaned = text
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        // Validate length
        if cleaned.len() < MIN_BIO_LENGTH {
            debug!("Biography too short ({} chars), rejecting", cleaned.len());
            return None;
        }

        // Truncate to 5000 characters if too long
        if cleaned.len() > 5000 {
            let truncated = cleaned.chars().take(5000).collect::<String>();
            Some(format!("{}...", truncated.trim()))
        } else {
            Some(cleaned)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_lucene_query() {
        assert_eq!(
            ArtistEnrichmentProvider::escape_lucene_query("AC/DC"),
            "AC\\/DC"
        );
        assert_eq!(
            ArtistEnrichmentProvider::escape_lucene_query("Artist (Name)"),
            "Artist \\(Name\\)"
        );
        assert_eq!(
            ArtistEnrichmentProvider::escape_lucene_query("Normal Name"),
            "Normal Name"
        );
        assert_eq!(
            ArtistEnrichmentProvider::escape_lucene_query("Artist+Plus"),
            "Artist\\+Plus"
        );
    }

    #[test]
    fn test_clean_biography() {
        // Valid biography
        let valid_bio = "A".repeat(100);
        assert!(ArtistEnrichmentProvider::clean_biography(valid_bio.clone()).is_some());

        // Too short
        let short_bio = "Short".to_string();
        assert!(ArtistEnrichmentProvider::clean_biography(short_bio).is_none());

        // With excessive whitespace
        let messy_bio = format!("{}  \n\n  \n  {}  \n  ", "A".repeat(60), "B".repeat(60));
        let cleaned = ArtistEnrichmentProvider::clean_biography(messy_bio);
        assert!(cleaned.is_some());
        let cleaned = cleaned.unwrap();
        assert!(!cleaned.contains("  ")); // No double spaces
        assert!(!cleaned.contains("\n\n\n")); // No triple newlines

        // Too long (>5000 chars)
        let long_bio = "A".repeat(6000);
        let cleaned = ArtistEnrichmentProvider::clean_biography(long_bio);
        assert!(cleaned.is_some());
        let cleaned = cleaned.unwrap();
        assert!(cleaned.len() <= 5003); // 5000 + "..."
        assert!(cleaned.ends_with("..."));
    }
}
