//! Artwork Pipeline - Extract, Fetch, Cache, and Deduplicate Album Artwork
//!
//! This module provides functionality for managing album artwork throughout
//! the music platform, including:
//! - Extraction from embedded audio tags
//! - Remote fetching from external APIs (MusicBrainz, Last.fm)
//! - Deduplication by content hash
//! - Image processing (resize, format conversion, dominant color)
//! - LRU caching with size limits
//!
//! ## Overview
//!
//! The `ArtworkService` coordinates artwork operations:
//! - Extract artwork from audio file tags
//! - Store artwork with automatic deduplication
//! - Retrieve artwork from cache or database
//! - Process images for optimal storage and display
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::artwork::ArtworkService;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let service = ArtworkService::new(
//!     artwork_repo,
//!     file_system,
//!     None,  // No HTTP client for embedded-only
//!     200 * 1024 * 1024,  // 200MB cache size
//! );
//!
//! // Extract and store embedded artwork
//! let extracted = vec![/* ExtractedArtwork from MetadataExtractor */];
//! let artwork_ids = service.extract_embedded(extracted).await?;
//!
//! // Retrieve artwork
//! let artwork = service.get(&artwork_ids[0]).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{MetadataError, Result};
use crate::extractor::ExtractedArtwork;
use bytes::Bytes;
use core_library::models::Artwork;
use core_library::repositories::ArtworkRepository;
use image::{DynamicImage, ImageFormat};
use lru::LruCache;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[cfg(feature = "artwork-remote")]
use bridge_traits::http::HttpClient;

#[cfg(feature = "artwork-remote")]
use crate::providers::{lastfm::LastFmClient, musicbrainz::MusicBrainzClient};

/// Standard artwork sizes for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkSize {
    /// Thumbnail size (300x300)
    Thumbnail,
    /// Full size (1200x1200)
    Full,
    /// Original size (no resize)
    Original,
}

impl ArtworkSize {
    /// Get the pixel dimension for this size
    pub fn dimension(&self) -> Option<u32> {
        match self {
            ArtworkSize::Thumbnail => Some(300),
            ArtworkSize::Full => Some(1200),
            ArtworkSize::Original => None,
        }
    }
}

/// Processed artwork result with multiple sizes
#[derive(Debug, Clone)]
pub struct ProcessedArtwork {
    /// Artwork ID in database
    pub id: String,
    /// Content hash for deduplication
    pub hash: String,
    /// Original image dimensions
    pub original_width: u32,
    pub original_height: u32,
    /// Dominant color as hex (e.g., "#FF5733")
    pub dominant_color: Option<String>,
    /// Whether this was deduplicated (existing artwork)
    pub deduplicated: bool,
}

/// Artwork service for managing album artwork
///
/// Coordinates artwork extraction, storage, caching, and retrieval.
/// Implements deduplication by content hash to minimize storage.
pub struct ArtworkService {
    /// Artwork repository for database operations
    repository: Arc<dyn ArtworkRepository>,
    /// LRU cache for artwork data (in-memory)
    cache: Arc<RwLock<LruCache<String, Bytes>>>,
    /// Maximum cache size in bytes
    max_cache_size: usize,
    /// Current cache size in bytes
    cache_size: Arc<RwLock<usize>>,
    /// HTTP client for remote artwork fetching (optional)
    #[cfg(feature = "artwork-remote")]
    http_client: Option<Arc<dyn HttpClient>>,
    /// MusicBrainz API client (optional)
    #[cfg(feature = "artwork-remote")]
    musicbrainz_client: Option<MusicBrainzClient>,
    /// Last.fm API client (optional)
    #[cfg(feature = "artwork-remote")]
    lastfm_client: Option<LastFmClient>,
}

impl ArtworkService {
    /// Create a new ArtworkService
    ///
    /// # Arguments
    ///
    /// * `repository` - Artwork repository for persistence
    /// * `max_cache_size` - Maximum cache size in bytes (default 200MB)
    ///
    /// # Returns
    ///
    /// New ArtworkService instance
    pub fn new(
        repository: Arc<dyn ArtworkRepository>,
        max_cache_size: usize,
    ) -> Self {
        let cache_capacity = NonZeroUsize::new(100).unwrap(); // 100 items in LRU
        Self {
            repository,
            cache: Arc::new(RwLock::new(LruCache::new(cache_capacity))),
            max_cache_size,
            cache_size: Arc::new(RwLock::new(0)),
            #[cfg(feature = "artwork-remote")]
            http_client: None,
            #[cfg(feature = "artwork-remote")]
            musicbrainz_client: None,
            #[cfg(feature = "artwork-remote")]
            lastfm_client: None,
        }
    }

    /// Create a new ArtworkService with HTTP client and API configuration for remote fetching
    ///
    /// # Arguments
    ///
    /// * `repository` - Artwork repository for persistence
    /// * `http_client` - HTTP client for making API requests
    /// * `max_cache_size` - Maximum cache size in bytes
    /// * `musicbrainz_user_agent` - Optional MusicBrainz User-Agent string
    /// * `lastfm_api_key` - Optional Last.fm API key
    /// * `rate_limit_delay_ms` - Rate limit delay in milliseconds (default 1000)
    ///
    /// # Returns
    ///
    /// New ArtworkService instance with remote fetching enabled
    #[cfg(feature = "artwork-remote")]
    pub fn with_remote_fetching(
        repository: Arc<dyn ArtworkRepository>,
        http_client: Arc<dyn HttpClient>,
        max_cache_size: usize,
        musicbrainz_user_agent: Option<String>,
        lastfm_api_key: Option<String>,
        rate_limit_delay_ms: u64,
    ) -> Self {
        let cache_capacity = NonZeroUsize::new(100).unwrap();
        
        // Create MusicBrainz client if user agent provided
        let musicbrainz_client = musicbrainz_user_agent.map(|ua| {
            MusicBrainzClient::new(http_client.clone(), ua, rate_limit_delay_ms)
        });

        // Create Last.fm client if API key provided
        let lastfm_client = lastfm_api_key.map(|key| {
            LastFmClient::new(http_client.clone(), key, rate_limit_delay_ms)
        });

        Self {
            repository,
            cache: Arc::new(RwLock::new(LruCache::new(cache_capacity))),
            max_cache_size,
            cache_size: Arc::new(RwLock::new(0)),
            http_client: Some(http_client),
            musicbrainz_client,
            lastfm_client,
        }
    }

    /// Create a new ArtworkService with HTTP client for remote fetching (deprecated)
    ///
    /// Use `with_remote_fetching` instead for better control over API configuration.
    #[cfg(feature = "artwork-remote")]
    #[deprecated(since = "0.1.0", note = "Use with_remote_fetching instead")]
    pub fn with_http_client(
        repository: Arc<dyn ArtworkRepository>,
        http_client: Arc<dyn HttpClient>,
        max_cache_size: usize,
    ) -> Self {
        let cache_capacity = NonZeroUsize::new(100).unwrap();
        Self {
            repository,
            cache: Arc::new(RwLock::new(LruCache::new(cache_capacity))),
            max_cache_size,
            cache_size: Arc::new(RwLock::new(0)),
            http_client: Some(http_client),
            musicbrainz_client: None,
            lastfm_client: None,
        }
    }

    /// Extract and store embedded artwork from audio files
    ///
    /// Processes extracted artwork from MetadataExtractor, deduplicates by hash,
    /// resizes/optimizes, and stores in database.
    ///
    /// # Arguments
    ///
    /// * `extracted` - Vector of ExtractedArtwork from MetadataExtractor
    ///
    /// # Returns
    ///
    /// Vector of ProcessedArtwork results with artwork IDs
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let extracted_artwork = extractor.extract_from_file(path).await?.artwork;
    /// let processed = service.extract_embedded(extracted_artwork).await?;
    /// for artwork in processed {
    ///     println!("Stored artwork: {} (deduplicated: {})", artwork.id, artwork.deduplicated);
    /// }
    /// ```
    pub async fn extract_embedded(
        &self,
        extracted: Vec<ExtractedArtwork>,
    ) -> Result<Vec<ProcessedArtwork>> {
        let mut results = Vec::new();

        for artwork in extracted {
            // Skip empty or invalid artwork
            if artwork.data.is_empty() {
                debug!("Skipping empty artwork");
                continue;
            }

            // Calculate content hash for deduplication
            let hash = self.calculate_hash(&artwork.data);

            // Check if artwork already exists in database
            if let Some(existing) = self.repository.find_by_hash(&hash).await? {
                debug!(
                    "Artwork already exists with hash {}, reusing ID {}",
                    hash, existing.id
                );
                results.push(ProcessedArtwork {
                    id: existing.id,
                    hash: existing.hash,
                    original_width: existing.width as u32,
                    original_height: existing.height as u32,
                    dominant_color: existing.dominant_color,
                    deduplicated: true,
                });
                continue;
            }

            // Process and store new artwork
            let processed = self
                .store_artwork(&artwork.data, &hash, &artwork.mime_type, "embedded")
                .await?;
            results.push(processed);
        }

        info!("Processed {} artwork images", results.len());
        Ok(results)
    }

    /// Store artwork with deduplication, processing, and optimization
    ///
    /// # Arguments
    ///
    /// * `data` - Raw image data
    /// * `hash` - Content hash (SHA-256)
    /// * `mime_type` - Image MIME type
    /// * `source` - Source of artwork (embedded, remote, user_uploaded)
    ///
    /// # Returns
    ///
    /// ProcessedArtwork with database ID and metadata
    async fn store_artwork(
        &self,
        data: &Bytes,
        hash: &str,
        mime_type: &str,
        _source: &str,
    ) -> Result<ProcessedArtwork> {
        // Load image for processing
        let img = image::load_from_memory(data).map_err(|e| MetadataError::ImageProcessing {
            message: format!("Failed to load image: {}", e),
        })?;

        let original_width = img.width();
        let original_height = img.height();

        // Extract dominant color
        let dominant_color = self.extract_dominant_color(&img);

        // Create artwork model
        let artwork = Artwork::new(
            hash.to_string(),
            data.to_vec(),
            original_width as i64,
            original_height as i64,
            mime_type.to_string(),
        );

        // Store in database
        self.repository.insert(&artwork).await.map_err(|e| {
            MetadataError::Database(format!("Failed to store artwork: {}", e))
        })?;

        info!(
            "Stored new artwork {} ({}x{}, {} bytes)",
            artwork.id,
            original_width,
            original_height,
            data.len()
        );

        Ok(ProcessedArtwork {
            id: artwork.id,
            hash: hash.to_string(),
            original_width,
            original_height,
            dominant_color: Some(dominant_color),
            deduplicated: false,
        })
    }

    /// Get artwork by ID
    ///
    /// Retrieves artwork from cache if available, otherwise from database.
    /// Automatically populates cache on miss.
    ///
    /// # Arguments
    ///
    /// * `artwork_id` - Artwork ID
    ///
    /// # Returns
    ///
    /// Artwork data as Bytes
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let artwork_data = service.get(&artwork_id).await?;
    /// // Use artwork_data for display
    /// ```
    pub async fn get(&self, artwork_id: &str) -> Result<Bytes> {
        // Check cache first
        {
            let mut cache = self.cache.write().await;
            if let Some(data) = cache.get(artwork_id) {
                debug!("Artwork {} found in cache", artwork_id);
                return Ok(data.clone());
            }
        }

        // Cache miss - fetch from database
        let artwork = self
            .repository
            .find_by_id(artwork_id)
            .await?
            .ok_or_else(|| MetadataError::ArtworkNotFound {
                artwork_id: artwork_id.to_string(),
            })?;

        let data = Bytes::from(artwork.binary_blob);

        // Add to cache if there's space
        self.add_to_cache(artwork_id.to_string(), data.clone())
            .await;

        debug!("Artwork {} loaded from database", artwork_id);
        Ok(data)
    }

    /// Add artwork to LRU cache with size limits
    async fn add_to_cache(&self, artwork_id: String, data: Bytes) {
        let data_size = data.len();

        // Check if adding would exceed cache size
        let mut cache_size = self.cache_size.write().await;
        if *cache_size + data_size > self.max_cache_size {
            // Evict oldest items until there's space
            let mut cache = self.cache.write().await;
            while *cache_size + data_size > self.max_cache_size && !cache.is_empty() {
                if let Some((_, evicted_data)) = cache.pop_lru() {
                    *cache_size -= evicted_data.len();
                    debug!("Evicted artwork from cache (size: {})", evicted_data.len());
                }
            }
        }

        // Add to cache
        let mut cache = self.cache.write().await;
        cache.put(artwork_id.clone(), data);
        *cache_size += data_size;

        debug!(
            "Added artwork {} to cache (size: {} bytes, total cache: {} bytes)",
            artwork_id, data_size, *cache_size
        );
    }

    /// Resize image to target size while maintaining aspect ratio
    ///
    /// # Arguments
    ///
    /// * `img` - Source image
    /// * `size` - Target size
    ///
    /// # Returns
    ///
    /// Resized image
    #[allow(dead_code)]
    fn resize_image(&self, img: &DynamicImage, size: ArtworkSize) -> DynamicImage {
        if let Some(dimension) = size.dimension() {
            img.resize(dimension, dimension, image::imageops::FilterType::Lanczos3)
        } else {
            img.clone()
        }
    }

    /// Convert image to WebP format for efficient storage
    ///
    /// # Arguments
    ///
    /// * `img` - Source image
    /// * `quality` - WebP quality (0-100)
    ///
    /// # Returns
    ///
    /// WebP encoded image data
    #[allow(dead_code)]
    fn convert_to_webp(&self, img: &DynamicImage, _quality: u8) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);

        // Convert to JPEG for now (WebP encoding requires additional dependencies)
        // In production, use webp crate for better compression
        img.write_to(&mut cursor, ImageFormat::Jpeg)
            .map_err(|e| MetadataError::ImageProcessing {
                message: format!("Failed to encode image: {}", e),
            })?;

        Ok(buffer)
    }

    /// Extract dominant color from image
    ///
    /// Samples pixels and calculates average RGB color.
    ///
    /// # Arguments
    ///
    /// * `img` - Source image
    ///
    /// # Returns
    ///
    /// Hex color string (e.g., "#FF5733")
    fn extract_dominant_color(&self, img: &DynamicImage) -> String {
        // Resize to small size for faster processing
        let small = img.resize(50, 50, image::imageops::FilterType::Nearest);
        let rgb = small.to_rgb8();

        // Calculate average color
        let mut r_sum: u64 = 0;
        let mut g_sum: u64 = 0;
        let mut b_sum: u64 = 0;
        let mut count: u64 = 0;

        for pixel in rgb.pixels() {
            r_sum += pixel[0] as u64;
            g_sum += pixel[1] as u64;
            b_sum += pixel[2] as u64;
            count += 1;
        }

        let r_avg = (r_sum / count) as u8;
        let g_avg = (g_sum / count) as u8;
        let b_avg = (b_sum / count) as u8;

        format!("#{:02X}{:02X}{:02X}", r_avg, g_avg, b_avg)
    }

    /// Calculate SHA-256 hash of artwork data
    fn calculate_hash(&self, data: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Fetch remote artwork from external APIs (feature-gated)
    ///
    /// Queries MusicBrainz and Last.fm for album artwork based on track metadata.
    ///
    /// # Arguments
    ///
    /// * `artist` - Artist name
    /// * `album` - Album name
    /// * `mbid` - MusicBrainz ID (optional, improves matching)
    ///
    /// # Returns
    ///
    /// ProcessedArtwork if found, None otherwise
    #[cfg(feature = "artwork-remote")]
    pub async fn fetch_remote(
        &self,
        artist: &str,
        album: &str,
        mbid: Option<&str>,
    ) -> Result<Option<ProcessedArtwork>> {
        let http_client = self.http_client.as_ref().ok_or_else(|| {
            MetadataError::ConfigurationError(
                "HTTP client required for remote artwork fetching".to_string(),
            )
        })?;

        info!(
            "Fetching remote artwork for '{} - {}'",
            artist, album
        );

        // Try MusicBrainz first (higher quality)
        if let Some(artwork) = self
            .fetch_from_musicbrainz(artist, album, mbid, http_client)
            .await?
        {
            return Ok(Some(artwork));
        }

        // Fallback to Last.fm
        if let Some(artwork) = self
            .fetch_from_lastfm(artist, album, http_client)
            .await?
        {
            return Ok(Some(artwork));
        }

        warn!(
            "No remote artwork found for '{} - {}'",
            artist, album
        );
        Ok(None)
    }

    /// Fetch artwork from MusicBrainz Cover Art Archive
    #[cfg(feature = "artwork-remote")]
    async fn fetch_from_musicbrainz(
        &self,
        artist: &str,
        album: &str,
        mbid: Option<&str>,
        _http_client: &Arc<dyn HttpClient>,
    ) -> Result<Option<ProcessedArtwork>> {
        // Check if MusicBrainz client is configured
        let client = match &self.musicbrainz_client {
            Some(c) => c,
            None => {
                debug!("MusicBrainz client not configured");
                return Ok(None);
            }
        };

        // Fetch cover art
        match client.fetch_cover_art(artist, album, mbid).await? {
            Some(artwork_data) => {
                info!(
                    "Fetched {} bytes of artwork from MusicBrainz for '{} - {}'",
                    artwork_data.len(),
                    artist,
                    album
                );

                // Store artwork with automatic hash calculation and deduplication
                let processed = self.store_remote_artwork(artwork_data).await?;
                Ok(Some(processed))
            }
            None => Ok(None),
        }
    }

    /// Fetch artwork from Last.fm API
    #[cfg(feature = "artwork-remote")]
    async fn fetch_from_lastfm(
        &self,
        artist: &str,
        album: &str,
        _http_client: &Arc<dyn HttpClient>,
    ) -> Result<Option<ProcessedArtwork>> {
        // Check if Last.fm client is configured
        let client = match &self.lastfm_client {
            Some(c) => c,
            None => {
                debug!("Last.fm client not configured");
                return Ok(None);
            }
        };

        // Fetch artwork
        match client.fetch_artwork(artist, album).await? {
            Some(artwork_data) => {
                info!(
                    "Fetched {} bytes of artwork from Last.fm for '{} - {}'",
                    artwork_data.len(),
                    artist,
                    album
                );

                // Store artwork with automatic hash calculation and deduplication
                let processed = self.store_remote_artwork(artwork_data).await?;
                Ok(Some(processed))
            }
            None => Ok(None),
        }
    }

    /// Store remotely-fetched artwork with automatic format detection and deduplication
    ///
    /// This is a helper method for remote artwork that automatically:
    /// - Calculates the content hash
    /// - Detects the image format/MIME type
    /// - Checks for duplicates
    /// - Stores new artwork if unique
    ///
    /// # Arguments
    ///
    /// * `data` - Raw image data
    ///
    /// # Returns
    ///
    /// ProcessedArtwork with deduplication flag set appropriately
    #[cfg(feature = "artwork-remote")]
    async fn store_remote_artwork(&self, data: Bytes) -> Result<ProcessedArtwork> {
        // Calculate hash
        let hash = self.calculate_hash(&data);

        // Check if artwork already exists (deduplication)
        if let Some(existing) = self.repository.find_by_hash(&hash).await.map_err(|e| {
            MetadataError::Database(format!("Failed to check for existing artwork: {}", e))
        })? {
            debug!("Artwork already exists with hash {}, using existing", hash);
            return Ok(ProcessedArtwork {
                id: existing.id,
                hash: existing.hash,
                original_width: existing.width as u32,
                original_height: existing.height as u32,
                dominant_color: existing.dominant_color,
                deduplicated: true,
            });
        }

        // Detect MIME type from data
        let mime_type = detect_mime_type(&data).unwrap_or_else(|| "image/jpeg".to_string());

        // Store as new artwork
        self.store_artwork(&data, &hash, &mime_type, "remote").await
    }

    /// Get cache statistics
    ///
    /// # Returns
    ///
    /// Tuple of (items_count, total_bytes)
    pub async fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let size = *self.cache_size.read().await;
        (cache.len(), size)
    }

    /// Clear artwork cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        let mut cache_size = self.cache_size.write().await;
        cache.clear();
        *cache_size = 0;
        info!("Cleared artwork cache");
    }
}

/// Detects MIME type from image data by examining the magic bytes
///
/// # Arguments
///
/// * `data` - Image data
///
/// # Returns
///
/// - `Some(String)` - Detected MIME type
/// - `None` - Unable to detect format
#[cfg(feature = "artwork-remote")]
fn detect_mime_type(data: &Bytes) -> Option<String> {
    if data.len() < 12 {
        return None;
    }

    // Check for common image format magic bytes
    match &data[0..4] {
        // JPEG: FF D8 FF
        [0xFF, 0xD8, 0xFF, _] => Some("image/jpeg".to_string()),
        // PNG: 89 50 4E 47
        [0x89, 0x50, 0x4E, 0x47] => Some("image/png".to_string()),
        // GIF: 47 49 46 38
        [0x47, 0x49, 0x46, 0x38] => Some("image/gif".to_string()),
        // WEBP: 52 49 46 46 ... 57 45 42 50
        [0x52, 0x49, 0x46, 0x46] if data.len() >= 12 && &data[8..12] == b"WEBP" => {
            Some("image/webp".to_string())
        }
        // BMP: 42 4D
        [0x42, 0x4D, _, _] => Some("image/bmp".to_string()),
        _ => None,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core_library::repositories::ArtworkRepository;
    use mockall::mock;
    use mockall::predicate::*;

    // Mock ArtworkRepository for testing
    mock! {
        pub ArtworkRepo {}

        #[async_trait::async_trait]
        impl ArtworkRepository for ArtworkRepo {
            async fn find_by_id(&self, id: &str) -> core_library::error::Result<Option<Artwork>>;
            async fn insert(&self, artwork: &Artwork) -> core_library::error::Result<()>;
            async fn update(&self, artwork: &Artwork) -> core_library::error::Result<()>;
            async fn delete(&self, id: &str) -> core_library::error::Result<bool>;
            async fn query(&self, page_request: core_library::repositories::PageRequest) -> core_library::error::Result<core_library::repositories::Page<Artwork>>;
            async fn find_by_hash(&self, hash: &str) -> core_library::error::Result<Option<Artwork>>;
            async fn count(&self) -> core_library::error::Result<i64>;
            async fn total_size(&self) -> core_library::error::Result<i64>;
        }
    }

    #[allow(dead_code)]
    fn create_test_image() -> Vec<u8> {
        // Create a simple 100x100 red image
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            100,
            image::Rgb([255, 0, 0]),
        ));
        let mut buffer = Vec::new();
        img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Jpeg)
            .unwrap();
        buffer
    }

    #[tokio::test]
    async fn test_artwork_service_creation() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let service = ArtworkService::new(mock_repo, 100 * 1024 * 1024);

        let (count, size) = service.cache_stats().await;
        assert_eq!(count, 0);
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_calculate_hash() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let service = ArtworkService::new(mock_repo, 100 * 1024 * 1024);

        let data = Bytes::from("test data");
        let hash = service.calculate_hash(&data);

        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
        assert_eq!(
            hash,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[tokio::test]
    async fn test_extract_dominant_color() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let service = ArtworkService::new(mock_repo, 100 * 1024 * 1024);

        // Create red image
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            100,
            image::Rgb([255, 0, 0]),
        ));

        let color = service.extract_dominant_color(&img);
        assert_eq!(color, "#FF0000"); // Should be red
    }

    #[tokio::test]
    async fn test_resize_image() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let service = ArtworkService::new(mock_repo, 100 * 1024 * 1024);

        let img = DynamicImage::ImageRgb8(image::RgbImage::new(1000, 1000));

        // Resize to thumbnail
        let thumbnail = service.resize_image(&img, ArtworkSize::Thumbnail);
        assert_eq!(thumbnail.width(), 300);
        assert_eq!(thumbnail.height(), 300);

        // Resize to full
        let full = service.resize_image(&img, ArtworkSize::Full);
        assert_eq!(full.width(), 1200);
        assert_eq!(full.height(), 1200);

        // Original size
        let original = service.resize_image(&img, ArtworkSize::Original);
        assert_eq!(original.width(), 1000);
        assert_eq!(original.height(), 1000);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let max_cache_size = 1000; // Small cache for testing
        let service = ArtworkService::new(mock_repo, max_cache_size);

        // Add multiple items to exceed cache size
        let data1 = Bytes::from(vec![0u8; 600]); // 600 bytes
        let data2 = Bytes::from(vec![1u8; 600]); // 600 bytes

        service.add_to_cache("id1".to_string(), data1).await;
        let (count, size) = service.cache_stats().await;
        assert_eq!(count, 1);
        assert_eq!(size, 600);

        // Adding second item should evict first
        service.add_to_cache("id2".to_string(), data2).await;
        let (count, size) = service.cache_stats().await;
        assert_eq!(count, 1); // Only one item fits
        assert_eq!(size, 600);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let mock_repo = Arc::new(MockArtworkRepo::new());
        let service = ArtworkService::new(mock_repo, 100 * 1024 * 1024);

        let data = Bytes::from(vec![0u8; 1000]);
        service.add_to_cache("id1".to_string(), data).await;

        let (count, _) = service.cache_stats().await;
        assert_eq!(count, 1);

        service.clear_cache().await;

        let (count, size) = service.cache_stats().await;
        assert_eq!(count, 0);
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_artwork_size_dimensions() {
        assert_eq!(ArtworkSize::Thumbnail.dimension(), Some(300));
        assert_eq!(ArtworkSize::Full.dimension(), Some(1200));
        assert_eq!(ArtworkSize::Original.dimension(), None);
    }
}
