//! Domain models for the music library
//!
//! This module contains rich domain models with validation and database mapping.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use uuid::Uuid;

// =============================================================================
// ID Types
// =============================================================================

/// Unique identifier for a track
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct TrackId(pub Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for TrackId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an album
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct AlbumId(pub Uuid);

impl AlbumId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for AlbumId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AlbumId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an artist
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct ArtistId(pub Uuid);

impl ArtistId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for ArtistId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ArtistId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a playlist
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct PlaylistId(pub Uuid);

impl PlaylistId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for PlaylistId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlaylistId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// =============================================================================
// Domain Models
// =============================================================================

/// Music track with complete metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Track {
    /// Unique identifier
    pub id: String,
    /// Provider this track is from
    pub provider_id: String,
    /// Provider's file identifier
    pub provider_file_id: String,
    /// Content hash for deduplication
    pub hash: Option<String>,

    // Metadata
    /// Track title
    pub title: String,
    /// Normalized title for searching
    pub normalized_title: String,
    /// Album reference
    pub album_id: Option<String>,
    /// Track artist
    pub artist_id: Option<String>,
    /// Album artist (for compilations)
    pub album_artist_id: Option<String>,
    /// Track position on album
    pub track_number: Option<i32>,
    /// Disc number for multi-disc albums
    pub disc_number: i32,
    /// Music genre
    pub genre: Option<String>,
    /// Release year
    pub year: Option<i32>,

    // Audio properties
    /// Duration in milliseconds
    pub duration_ms: i64,
    /// Bitrate in kbps
    pub bitrate: Option<i32>,
    /// Sample rate in Hz
    pub sample_rate: Option<i32>,
    /// Number of audio channels
    pub channels: Option<i32>,
    /// File format (mp3, flac, m4a, etc.)
    pub format: String,

    // File metadata
    /// File size in bytes
    pub file_size: Option<i64>,
    /// MIME type from provider
    pub mime_type: Option<String>,

    // Enrichment status
    /// Artwork reference
    pub artwork_id: Option<String>,
    /// Lyrics fetch status
    pub lyrics_status: String,

    // Timestamps
    /// When first added
    pub created_at: i64,
    /// Last update time
    pub updated_at: i64,
    /// Last modified time from provider
    pub provider_modified_at: Option<i64>,
}

impl Track {
    /// Validate track data
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Track title cannot be empty".to_string());
        }

        if self.duration_ms <= 0 {
            return Err("Track duration must be positive".to_string());
        }

        if let Some(year) = self.year {
            if !(1900..=2100).contains(&year) {
                return Err(format!("Track year {} is out of valid range", year));
            }
        }

        if let Some(track_number) = self.track_number {
            if track_number <= 0 {
                return Err("Track number must be positive".to_string());
            }
        }

        if self.disc_number <= 0 {
            return Err("Disc number must be positive".to_string());
        }

        Ok(())
    }

    /// Normalize a string for searching (lowercase, trimmed)
    pub fn normalize(s: &str) -> String {
        s.trim().to_lowercase()
    }
}
