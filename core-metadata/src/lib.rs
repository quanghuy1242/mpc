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

pub mod error;

pub use error::{MetadataError, Result};
