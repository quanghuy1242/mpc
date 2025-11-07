//! Audio Tag Extraction and Metadata Processing
//!
//! This module provides functionality for extracting metadata from audio files
//! using the `lofty` crate. It supports ID3v2, Vorbis Comments, MP4 tags, and FLAC.
//!
//! ## Overview
//!
//! - Extracts comprehensive metadata (title, artist, album, year, etc.)
//! - Normalizes metadata (trim whitespace, title case, standardize track numbers)
//! - Extracts embedded artwork
//! - Calculates SHA-256 content hash for deduplication
//! - Handles corrupted files gracefully with partial metadata
//!
//! ## Usage
//!
//! ```ignore
//! use core_metadata::extractor::MetadataExtractor;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let extractor = MetadataExtractor::new();
//! let metadata = extractor.extract_from_file(Path::new("song.mp3")).await?;
//!
//! println!("Title: {}", metadata.title.unwrap_or_default());
//! println!("Artist: {}", metadata.artist.unwrap_or_default());
//! println!("Duration: {}ms", metadata.duration_ms);
//! # Ok(())
//! # }
//! ```

use bytes::Bytes;
use core_async::fs;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::{debug, warn};

use crate::error::{MetadataError, Result};

/// Extracted metadata from an audio file
#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    // Core metadata
    /// Track title (normalized)
    pub title: Option<String>,
    /// Primary artist (normalized)
    pub artist: Option<String>,
    /// Album name (normalized)
    pub album: Option<String>,
    /// Album artist (for compilations, normalized)
    pub album_artist: Option<String>,
    /// Release year
    pub year: Option<i32>,
    /// Track number on album
    pub track_number: Option<u32>,
    /// Total tracks on album
    pub total_tracks: Option<u32>,
    /// Disc number for multi-disc albums
    pub disc_number: Option<u32>,
    /// Total discs
    pub total_discs: Option<u32>,
    /// Genre classification
    pub genre: Option<String>,
    /// Composer/songwriter
    pub composer: Option<String>,
    /// Album cover comment/description
    pub comment: Option<String>,

    // Audio properties
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Bitrate in bits per second
    pub bitrate: Option<u32>,
    /// Sample rate in Hz
    pub sample_rate: Option<u32>,
    /// Number of audio channels
    pub channels: Option<u8>,
    /// Audio format/codec (e.g., "MP3", "FLAC", "AAC")
    pub format: String,
    /// File size in bytes
    pub file_size: u64,
    /// MIME type
    pub mime_type: String,

    // Deduplication and integrity
    /// SHA-256 hash of file contents
    pub content_hash: String,

    // Embedded artwork
    /// Extracted artwork images
    pub artwork: Vec<ExtractedArtwork>,

    // Parsing metadata
    /// Whether parsing encountered errors
    pub has_errors: bool,
    /// Partial success - some tags extracted despite errors
    pub partial_metadata: bool,
}

/// Extracted artwork/cover image
#[derive(Debug, Clone)]
pub struct ExtractedArtwork {
    /// Image data (JPEG, PNG, etc.)
    pub data: Bytes,
    /// MIME type (e.g., "image/jpeg", "image/png")
    pub mime_type: String,
    /// Picture type (Cover Front, Back, etc.)
    pub picture_type: ArtworkType,
    /// Image description
    pub description: Option<String>,
    /// Image width in pixels (if available)
    pub width: Option<u32>,
    /// Image height in pixels (if available)
    pub height: Option<u32>,
}

/// Type of artwork/cover image
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkType {
    /// Front cover
    CoverFront,
    /// Back cover
    CoverBack,
    /// Artist/performer
    Artist,
    /// Other/unspecified
    Other,
}

impl From<lofty::picture::PictureType> for ArtworkType {
    fn from(picture_type: lofty::picture::PictureType) -> Self {
        use lofty::picture::PictureType as PT;
        match picture_type {
            PT::CoverFront => ArtworkType::CoverFront,
            PT::CoverBack => ArtworkType::CoverBack,
            PT::Artist | PT::Conductor | PT::LeadArtist | PT::Band => ArtworkType::Artist,
            _ => ArtworkType::Other,
        }
    }
}

/// Audio metadata extractor
///
/// Extracts metadata from audio files using the `lofty` crate.
/// Supports ID3v2, Vorbis Comments, MP4 tags, FLAC, and other common formats.
pub struct MetadataExtractor {
    /// Parse options for lofty
    parse_options: ParseOptions,
}

impl MetadataExtractor {
    /// Create a new metadata extractor with default settings
    pub fn new() -> Self {
        Self {
            parse_options: ParseOptions::new(),
        }
    }

    /// Create extractor with custom parse options
    pub fn with_options(parse_options: ParseOptions) -> Self {
        Self { parse_options }
    }

    /// Extract metadata from an audio file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    ///
    /// Returns `Ok(ExtractedMetadata)` with as much metadata as could be extracted.
    /// If parsing encounters errors, `partial_metadata` will be true and `has_errors`
    /// will indicate issues.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File cannot be opened or read
    /// - File format is completely unsupported
    /// - Critical parsing errors prevent any metadata extraction
    ///
    /// # Performance
    ///
    /// Target: <50ms per track for typical audio files
    ///
    /// # Example
    ///
    /// ```ignore
    /// let extractor = MetadataExtractor::new();
    /// let metadata = extractor.extract_from_file(Path::new("song.flac")).await?;
    /// assert!(metadata.duration_ms > 0);
    /// ```
    pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedMetadata> {
        debug!("Extracting metadata from: {}", path.display());

        // Read file and calculate hash
        let file_data = fs::read(path)
            .await
            .map_err(|e| MetadataError::ExtractionFailed(format!("Failed to read file: {}", e)))?;

        let file_size = file_data.len() as u64;
        let content_hash = self.calculate_hash(&file_data);

        // Probe the file to determine format
        let tagged_file = Probe::new(std::io::Cursor::new(&file_data))
            .options(self.parse_options)
            .guess_file_type()
            .map_err(|e| MetadataError::ExtractionFailed(format!("Failed to probe file: {}", e)))?
            .read()
            .map_err(|e| MetadataError::ExtractionFailed(format!("Failed to parse file: {}", e)))?;

        let file_type = tagged_file.file_type();
        let properties = tagged_file.properties();

        // Extract format information
        let format = format!("{:?}", file_type);
        let mime_type = Self::file_type_to_mime_type(file_type);

        // Extract audio properties
        let duration_ms = properties.duration().as_millis() as u64;
        let bitrate = properties.audio_bitrate();
        let sample_rate = properties.sample_rate();
        let channels = properties.channels();

        // Try to extract tags from primary tag, falling back to first available tag
        let mut has_errors = false;
        let mut partial_metadata = false;

        let tag = if let Some(primary_tag) = tagged_file.primary_tag() {
            Some(primary_tag)
        } else {
            tagged_file.first_tag()
        };

        // Extract text metadata
        let (
            title,
            artist,
            album,
            album_artist,
            year,
            track_number,
            total_tracks,
            disc_number,
            total_discs,
            genre,
            composer,
            comment,
        ) = if let Some(tag) = tag {
            (
                tag.title().map(|s| Self::normalize_text(s.as_ref())),
                tag.artist().map(|s| Self::normalize_text(s.as_ref())),
                tag.album().map(|s| Self::normalize_text(s.as_ref())),
                tag.get_string(&ItemKey::AlbumArtist)
                    .map(Self::normalize_text),
                tag.year().map(|y| y as i32),
                tag.track(),
                tag.track_total(),
                tag.disk(),
                tag.disk_total(),
                tag.genre().map(|s| Self::normalize_text(s.as_ref())),
                tag.get_string(&ItemKey::Composer).map(Self::normalize_text),
                tag.comment().map(|s| Self::normalize_text(s.as_ref())),
            )
        } else {
            warn!(
                "No tags found in file: {}. Using filename as title.",
                path.display()
            );
            partial_metadata = true;
            has_errors = true;

            // Fallback: use filename as title
            let filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();

            (
                Some(Self::normalize_text(&filename)),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
        };

        // Extract artwork
        let artwork = if let Some(tag) = tag {
            Self::extract_artwork(tag)
        } else {
            Vec::new()
        };

        Ok(ExtractedMetadata {
            title,
            artist,
            album,
            album_artist,
            year,
            track_number,
            total_tracks,
            disc_number,
            total_discs,
            genre,
            composer,
            comment,
            duration_ms,
            bitrate,
            sample_rate,
            channels,
            format,
            file_size,
            mime_type,
            content_hash,
            artwork,
            has_errors,
            partial_metadata,
        })
    }

    /// Calculate SHA-256 hash of file contents for deduplication
    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    /// Normalize text metadata
    ///
    /// - Trims leading/trailing whitespace
    /// - Normalizes consecutive whitespace to single space
    /// - Removes null bytes and control characters
    fn normalize_text(text: &str) -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .collect()
    }

    /// Extract all artwork/pictures from tag
    fn extract_artwork(tag: &lofty::tag::Tag) -> Vec<ExtractedArtwork> {
        tag.pictures()
            .iter()
            .filter_map(|pic| {
                let data = Bytes::copy_from_slice(pic.data());

                // Get MIME type, defaulting to octet-stream if None
                let mime_type = pic
                    .mime_type()
                    .map(Self::mime_type_to_string)
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                // Skip if data is empty or MIME type is invalid
                if data.is_empty() || mime_type == "application/octet-stream" {
                    return None;
                }

                Some(ExtractedArtwork {
                    data,
                    mime_type,
                    picture_type: ArtworkType::from(pic.pic_type()),
                    description: pic.description().map(|s| s.to_string()),
                    width: None,  // lofty doesn't provide dimensions
                    height: None, // lofty doesn't provide dimensions
                })
            })
            .collect()
    }

    /// Convert lofty MimeType to string
    fn mime_type_to_string(mime_type: &MimeType) -> String {
        match mime_type {
            MimeType::Png => "image/png".to_string(),
            MimeType::Jpeg => "image/jpeg".to_string(),
            MimeType::Tiff => "image/tiff".to_string(),
            MimeType::Bmp => "image/bmp".to_string(),
            MimeType::Gif => "image/gif".to_string(),
            _ => "application/octet-stream".to_string(),
        }
    }

    /// Convert lofty FileType to MIME type string
    fn file_type_to_mime_type(file_type: lofty::file::FileType) -> String {
        use lofty::file::FileType;
        match file_type {
            FileType::Aac => "audio/aac",
            FileType::Aiff => "audio/aiff",
            FileType::Ape => "audio/ape",
            FileType::Flac => "audio/flac",
            FileType::Mpeg => "audio/mpeg",
            FileType::Mp4 => "audio/mp4",
            FileType::Mpc => "audio/musepack",
            FileType::Opus => "audio/opus",
            FileType::Vorbis => "audio/vorbis",
            FileType::Speex => "audio/speex",
            FileType::Wav => "audio/wav",
            FileType::WavPack => "audio/wavpack",
            FileType::Custom(_) | _ => "application/octet-stream",
        }
        .to_string()
    }
}

impl Default for MetadataExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text() {
        assert_eq!(
            MetadataExtractor::normalize_text("  Hello   World  "),
            "Hello World"
        );
        assert_eq!(
            MetadataExtractor::normalize_text("Title\nWith\tWhitespace"),
            "Title With Whitespace"
        );
        assert_eq!(
            MetadataExtractor::normalize_text("Clean Text"),
            "Clean Text"
        );
    }

    #[test]
    fn test_calculate_hash() {
        let extractor = MetadataExtractor::new();
        let data = b"test data";
        let hash = extractor.calculate_hash(data);

        // SHA-256 hash should be 64 hex characters
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Same data should produce same hash
        let hash2 = extractor.calculate_hash(data);
        assert_eq!(hash, hash2);

        // Different data should produce different hash
        let hash3 = extractor.calculate_hash(b"different data");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_artwork_type_conversion() {
        use lofty::picture::PictureType;

        assert_eq!(
            ArtworkType::from(PictureType::CoverFront),
            ArtworkType::CoverFront
        );
        assert_eq!(
            ArtworkType::from(PictureType::CoverBack),
            ArtworkType::CoverBack
        );
        assert_eq!(ArtworkType::from(PictureType::Artist), ArtworkType::Artist);
        assert_eq!(ArtworkType::from(PictureType::Other), ArtworkType::Other);
    }

    #[test]
    fn test_mime_type_to_string() {
        assert_eq!(
            MetadataExtractor::mime_type_to_string(&MimeType::Png),
            "image/png"
        );
        assert_eq!(
            MetadataExtractor::mime_type_to_string(&MimeType::Jpeg),
            "image/jpeg"
        );
        assert_eq!(
            MetadataExtractor::mime_type_to_string(&MimeType::Gif),
            "image/gif"
        );
    }

    #[test]
    fn test_file_type_to_mime_type() {
        use lofty::file::FileType;

        assert_eq!(
            MetadataExtractor::file_type_to_mime_type(FileType::Mpeg),
            "audio/mpeg"
        );
        assert_eq!(
            MetadataExtractor::file_type_to_mime_type(FileType::Flac),
            "audio/flac"
        );
        assert_eq!(
            MetadataExtractor::file_type_to_mime_type(FileType::Mp4),
            "audio/mp4"
        );
        assert_eq!(
            MetadataExtractor::file_type_to_mime_type(FileType::Opus),
            "audio/opus"
        );
    }

    #[test]
    fn test_metadata_extractor_new() {
        let extractor = MetadataExtractor::new();
        assert!(format!("{:?}", extractor.parse_options).contains("ParseOptions"));
    }

    #[test]
    fn test_metadata_extractor_default() {
        let extractor1 = MetadataExtractor::new();
        let extractor2 = MetadataExtractor::default();

        // Both should have same parse options structure
        assert_eq!(
            format!("{:?}", extractor1.parse_options),
            format!("{:?}", extractor2.parse_options)
        );
    }

    // Integration tests with actual audio files would go in tests/ directory
}
