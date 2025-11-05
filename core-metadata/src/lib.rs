//! # Metadata & Lyrics Module
//!
//! Extracts metadata from audio files and enriches library with artwork and lyrics.
//!
//! ## Overview
//!
//! This module handles:
//! - Audio tag extraction (ID3, Vorbis, MP4, FLAC)
//! - Embedded artwork extraction
//! - Remote artwork fetching (optional, feature-gated)
//! - Lyrics fetching from external providers (optional, feature-gated)
//! - Content hashing for deduplication
//! - Background enrichment jobs for batch processing

pub mod artwork;
pub mod enrichment_job;
pub mod error;
pub mod extractor;
pub mod lyrics;

pub use artwork::{ArtworkService, ArtworkSize, ProcessedArtwork};
pub use enrichment_job::{EnrichmentConfig, EnrichmentJob, EnrichmentProgress, EnrichmentResult};
pub use error::{MetadataError, Result};
pub use extractor::{ArtworkType, ExtractedArtwork, ExtractedMetadata, MetadataExtractor};
pub use lyrics::{LyricsProvider, LyricsResult, LyricsSearchQuery, LyricsService, LyricsSource};
