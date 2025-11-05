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

/// Album with metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Album {
    /// Unique identifier
    pub id: String,
    /// Album name
    pub name: String,
    /// Normalized name for searching
    pub normalized_name: String,
    /// Album artist reference
    pub artist_id: Option<String>,
    /// Release year
    pub year: Option<i32>,
    /// Artwork reference
    pub artwork_id: Option<String>,
    /// Cached track count
    pub track_count: i64,
    /// Cached total duration in milliseconds
    pub total_duration_ms: i64,
    /// Timestamps
    pub created_at: i64,
    pub updated_at: i64,
}

impl Album {
    /// Create a new album with normalized name
    pub fn new(name: String, artist_id: Option<String>) -> Self {
        let normalized_name = Self::normalize(&name);
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            normalized_name,
            artist_id,
            year: None,
            artwork_id: None,
            track_count: 0,
            total_duration_ms: 0,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Validate album data
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Album name cannot be empty".to_string());
        }

        if let Some(year) = self.year {
            if !(1900..=2100).contains(&year) {
                return Err(format!("Album year {} is out of valid range", year));
            }
        }

        if self.track_count < 0 {
            return Err("Track count cannot be negative".to_string());
        }

        Ok(())
    }

    /// Normalize a string for searching
    pub fn normalize(s: &str) -> String {
        s.trim().to_lowercase()
    }
}

/// Artist with metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Artist {
    /// Unique identifier
    pub id: String,
    /// Artist name
    pub name: String,
    /// Normalized name for searching
    pub normalized_name: String,
    /// Sort name for alphabetical sorting
    pub sort_name: Option<String>,
    /// Timestamps
    pub created_at: i64,
    pub updated_at: i64,
}

impl Artist {
    /// Create a new artist with normalized name
    pub fn new(name: String) -> Self {
        let normalized_name = Self::normalize(&name);
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            normalized_name,
            sort_name: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Validate artist data
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Artist name cannot be empty".to_string());
        }

        Ok(())
    }

    /// Normalize a string for searching
    pub fn normalize(s: &str) -> String {
        s.trim().to_lowercase()
    }
}

/// Playlist with tracks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Playlist {
    /// Unique identifier
    pub id: String,
    /// Playlist name
    pub name: String,
    /// Normalized name for searching
    pub normalized_name: String,
    /// Description
    pub description: Option<String>,
    /// Owner type (user or system)
    pub owner_type: String,
    /// Sort order (manual, date_added, title, etc.)
    pub sort_order: String,
    /// Cached track count
    pub track_count: i64,
    /// Cached total duration in milliseconds
    pub total_duration_ms: i64,
    /// Optional playlist cover art
    pub artwork_id: Option<String>,
    /// Timestamps
    pub created_at: i64,
    pub updated_at: i64,
}

impl Playlist {
    /// Create a new user playlist
    pub fn new(name: String) -> Self {
        let normalized_name = name.trim().to_lowercase();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            normalized_name,
            description: None,
            owner_type: "user".to_string(),
            sort_order: "manual".to_string(),
            track_count: 0,
            total_duration_ms: 0,
            artwork_id: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Create a system playlist
    pub fn new_system(name: String, sort_order: String) -> Self {
        let normalized_name = name.trim().to_lowercase();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            normalized_name,
            description: None,
            owner_type: "system".to_string(),
            sort_order,
            track_count: 0,
            total_duration_ms: 0,
            artwork_id: None,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Validate playlist data
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Playlist name cannot be empty".to_string());
        }

        if !["user", "system"].contains(&self.owner_type.as_str()) {
            return Err(format!("Invalid owner type: {}", self.owner_type));
        }

        if ![
            "manual",
            "date_added",
            "title",
            "artist",
            "album",
            "duration",
        ]
        .contains(&self.sort_order.as_str())
        {
            return Err(format!("Invalid sort order: {}", self.sort_order));
        }

        Ok(())
    }
}

/// Folder in provider storage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Folder {
    /// Unique identifier
    pub id: String,
    /// Provider this folder belongs to
    pub provider_id: String,
    /// Provider's folder identifier
    pub provider_folder_id: String,
    /// Folder name
    pub name: String,
    /// Normalized name for searching
    pub normalized_name: String,
    /// Parent folder ID (None for root)
    pub parent_id: Option<String>,
    /// Full path from root
    pub path: String,
    /// Timestamps
    pub created_at: i64,
}

impl Folder {
    /// Create a new folder
    pub fn new(provider_id: String, provider_folder_id: String, name: String, parent_id: Option<String>, path: String) -> Self {
        let normalized_name = name.trim().to_lowercase();
        Self {
            id: Uuid::new_v4().to_string(),
            provider_id,
            provider_folder_id,
            name,
            normalized_name,
            parent_id,
            path,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Validate folder data
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Folder name cannot be empty".to_string());
        }

        if self.path.trim().is_empty() {
            return Err("Folder path cannot be empty".to_string());
        }

        Ok(())
    }
}

/// Artwork/cover image
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Artwork {
    /// Unique identifier
    pub id: String,
    /// Content hash for deduplication
    pub hash: String,
    /// Binary image data
    #[serde(skip_serializing)]
    pub binary_blob: Vec<u8>,
    /// MIME type (image/jpeg, image/png, etc.)
    pub mime_type: String,
    /// Image width in pixels
    pub width: i64,
    /// Image height in pixels
    pub height: i64,
    /// File size in bytes
    pub file_size: i64,
    /// Dominant color as hex (e.g., "#FF5733")
    pub dominant_color: Option<String>,
    /// Source of the artwork (embedded, remote, user_uploaded)
    pub source: String,
    /// Timestamps
    pub created_at: i64,
}

impl Artwork {
    /// Create new artwork from image data
    pub fn new(
        hash: String,
        binary_blob: Vec<u8>,
        width: i64,
        height: i64,
        mime_type: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            hash,
            file_size: binary_blob.len() as i64,
            binary_blob,
            mime_type,
            width,
            height,
            dominant_color: None,
            source: "embedded".to_string(),
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Validate artwork data
    pub fn validate(&self) -> Result<(), String> {
        if self.binary_blob.is_empty() {
            return Err("Artwork binary data cannot be empty".to_string());
        }

        if self.width <= 0 || self.height <= 0 {
            return Err("Artwork dimensions must be positive".to_string());
        }

        if !["image/jpeg", "image/png", "image/webp", "image/gif"]
            .contains(&self.mime_type.as_str())
        {
            return Err(format!("Unsupported artwork MIME type: {}", self.mime_type));
        }

        if self.file_size != self.binary_blob.len() as i64 {
            return Err("Artwork size mismatch".to_string());
        }

        Ok(())
    }
}

/// Track lyrics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Lyrics {
    /// Track this lyrics belongs to
    pub track_id: String,
    /// Source of lyrics (lrclib, manual, etc.)
    pub source: String,
    /// Whether lyrics are synced (LRC format) - SQLite stores as 0 or 1
    pub synced: i64,
    /// Lyrics body (plain text or LRC format)
    pub body: String,
    /// Language code (ISO 639-1)
    pub language: Option<String>,
    /// Last time lyrics were checked/updated
    pub last_checked_at: i64,
    /// Timestamps
    pub created_at: i64,
}

impl Lyrics {
    /// Create new lyrics
    pub fn new(track_id: String, source: String, synced: bool, body: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            track_id,
            source,
            synced: if synced { 1 } else { 0 },
            body,
            language: None,
            last_checked_at: now,
            created_at: now,
        }
    }

    /// Validate lyrics data
    pub fn validate(&self) -> Result<(), String> {
        if self.body.trim().is_empty() {
            return Err("Lyrics body cannot be empty".to_string());
        }

        if self.synced != 0 && !self.body.contains('[') {
            return Err("Synced lyrics must be in LRC format".to_string());
        }

        Ok(())
    }

    /// Check if lyrics are in LRC format
    pub fn is_lrc_format(&self) -> bool {
        self.body.contains('[') && self.body.contains(']')
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_album_new() {
        let album = Album::new("Test Album".to_string(), Some("artist-123".to_string()));
        assert_eq!(album.name, "Test Album");
        assert_eq!(album.normalized_name, "test album");
        assert_eq!(album.artist_id, Some("artist-123".to_string()));
        assert_eq!(album.track_count, 0);
        assert!(album.created_at > 0);
        assert!(album.updated_at > 0);
    }

    #[test]
    fn test_album_validation() {
        let mut album = Album::new("Valid Album".to_string(), None);
        assert!(album.validate().is_ok());

        // Empty name
        album.name = "".to_string();
        assert!(album.validate().is_err());

        // Invalid year
        album.name = "Valid".to_string();
        album.year = Some(1800);
        assert!(album.validate().is_err());

        album.year = Some(2200);
        assert!(album.validate().is_err());

        // Negative track count
        album.year = Some(2020);
        album.track_count = -1;
        assert!(album.validate().is_err());
    }

    #[test]
    fn test_album_normalize() {
        assert_eq!(Album::normalize("  Test Album  "), "test album");
        assert_eq!(Album::normalize("UPPERCASE"), "uppercase");
    }

    #[test]
    fn test_artist_new() {
        let artist = Artist::new("Test Artist".to_string());
        assert_eq!(artist.name, "Test Artist");
        assert_eq!(artist.normalized_name, "test artist");
        assert!(artist.sort_name.is_none());
        assert!(artist.created_at > 0);
    }

    #[test]
    fn test_artist_validation() {
        let mut artist = Artist::new("Valid Artist".to_string());
        assert!(artist.validate().is_ok());

        // Empty name
        artist.name = "".to_string();
        assert!(artist.validate().is_err());

        artist.name = "   ".to_string();
        assert!(artist.validate().is_err());
    }

    #[test]
    fn test_playlist_new() {
        let playlist = Playlist::new("My Playlist".to_string());
        assert_eq!(playlist.name, "My Playlist");
        assert_eq!(playlist.normalized_name, "my playlist");
        assert_eq!(playlist.owner_type, "user");
        assert_eq!(playlist.sort_order, "manual");
        assert_eq!(playlist.track_count, 0);
    }

    #[test]
    fn test_playlist_new_system() {
        let playlist = Playlist::new_system("Recently Added".to_string(), "date_added".to_string());
        assert_eq!(playlist.name, "Recently Added");
        assert_eq!(playlist.owner_type, "system");
        assert_eq!(playlist.sort_order, "date_added");
    }

    #[test]
    fn test_playlist_validation() {
        let mut playlist = Playlist::new("Valid Playlist".to_string());
        assert!(playlist.validate().is_ok());

        // Empty name
        playlist.name = "".to_string();
        assert!(playlist.validate().is_err());

        // Invalid owner type
        playlist.name = "Valid".to_string();
        playlist.owner_type = "invalid".to_string();
        assert!(playlist.validate().is_err());

        // Invalid sort order
        playlist.owner_type = "user".to_string();
        playlist.sort_order = "invalid".to_string();
        assert!(playlist.validate().is_err());
    }

    #[test]
    fn test_folder_new() {
        let folder = Folder::new(
            "provider-1".to_string(),
            "folder-music".to_string(),
            "Music".to_string(),
            None,
            "/Music".to_string(),
        );
        assert_eq!(folder.name, "Music");
        assert_eq!(folder.provider_id, "provider-1");
        assert_eq!(folder.provider_folder_id, "folder-music");
        assert_eq!(folder.path, "/Music");
        assert!(folder.parent_id.is_none());
    }

    #[test]
    fn test_folder_validation() {
        let mut folder = Folder::new(
            "provider-1".to_string(),
            "folder-valid".to_string(),
            "Valid".to_string(),
            None,
            "/Valid".to_string(),
        );
        assert!(folder.validate().is_ok());

        // Empty name
        folder.name = "".to_string();
        assert!(folder.validate().is_err());

        // Empty path
        folder.name = "Valid".to_string();
        folder.path = "".to_string();
        assert!(folder.validate().is_err());
    }

    #[test]
    fn test_artwork_new() {
        let data = vec![0xFF, 0xD8, 0xFF]; // JPEG header
        let artwork = Artwork::new(
            "hash123".to_string(),
            data.clone(),
            800,
            600,
            "image/jpeg".to_string(),
        );
        assert_eq!(artwork.hash, "hash123");
        assert_eq!(artwork.width, 800);
        assert_eq!(artwork.height, 600);
        assert_eq!(artwork.mime_type, "image/jpeg");
        assert_eq!(artwork.file_size, data.len() as i64);
    }

    #[test]
    fn test_artwork_validation() {
        let data = vec![0xFF, 0xD8, 0xFF];
        let mut artwork = Artwork::new(
            "hash123".to_string(),
            data.clone(),
            800,
            600,
            "image/jpeg".to_string(),
        );
        assert!(artwork.validate().is_ok());

        // Empty data
        artwork.binary_blob = vec![];
        artwork.file_size = 0;
        assert!(artwork.validate().is_err());

        // Invalid dimensions
        artwork.binary_blob = data.clone();
        artwork.file_size = data.len() as i64;
        artwork.width = 0;
        assert!(artwork.validate().is_err());

        artwork.width = 800;
        artwork.height = -1;
        assert!(artwork.validate().is_err());

        // Invalid MIME type
        artwork.height = 600;
        artwork.mime_type = "text/plain".to_string();
        assert!(artwork.validate().is_err());

        // Size mismatch
        artwork.mime_type = "image/jpeg".to_string();
        artwork.file_size = 999;
        assert!(artwork.validate().is_err());
    }

    #[test]
    fn test_lyrics_new() {
        let lyrics = Lyrics::new(
            "track-123".to_string(),
            "lrclib".to_string(),
            false,
            "These are the lyrics".to_string(),
        );
        assert_eq!(lyrics.track_id, "track-123");
        assert_eq!(lyrics.source, "lrclib");
        assert_eq!(lyrics.synced, 0); // false = 0 in SQLite
        assert_eq!(lyrics.body, "These are the lyrics");
    }

    #[test]
    fn test_lyrics_validation() {
        let mut lyrics = Lyrics::new(
            "track-123".to_string(),
            "manual".to_string(),
            false,
            "Valid lyrics".to_string(),
        );
        assert!(lyrics.validate().is_ok());

        // Empty body
        lyrics.body = "".to_string();
        assert!(lyrics.validate().is_err());

        // Synced but not LRC format
        lyrics.body = "Not LRC format".to_string();
        lyrics.synced = 1;
        assert!(lyrics.validate().is_err());

        // Valid LRC format
        lyrics.body = "[00:12.00]Line 1\n[00:17.20]Line 2".to_string();
        assert!(lyrics.validate().is_ok());
    }

    #[test]
    fn test_lyrics_is_lrc_format() {
        let plain_lyrics = Lyrics::new(
            "track-123".to_string(),
            "manual".to_string(),
            false,
            "Plain text lyrics".to_string(),
        );
        assert!(!plain_lyrics.is_lrc_format());

        let lrc_lyrics = Lyrics::new(
            "track-123".to_string(),
            "lrclib".to_string(),
            true,
            "[00:12.00]Synced lyrics".to_string(),
        );
        assert!(lrc_lyrics.is_lrc_format());
    }

    #[test]
    fn test_id_types_display() {
        let track_id = TrackId::new();
        let album_id = AlbumId::new();
        let artist_id = ArtistId::new();
        let playlist_id = PlaylistId::new();

        // Should display as UUID strings
        assert!(!track_id.to_string().is_empty());
        assert!(!album_id.to_string().is_empty());
        assert!(!artist_id.to_string().is_empty());
        assert!(!playlist_id.to_string().is_empty());
    }

    #[test]
    fn test_id_types_from_string() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";

        let track_id = TrackId::from_string(uuid_str).unwrap();
        assert_eq!(track_id.to_string(), uuid_str);

        let album_id = AlbumId::from_string(uuid_str).unwrap();
        assert_eq!(album_id.to_string(), uuid_str);

        let artist_id = ArtistId::from_string(uuid_str).unwrap();
        assert_eq!(artist_id.to_string(), uuid_str);

        let playlist_id = PlaylistId::from_string(uuid_str).unwrap();
        assert_eq!(playlist_id.to_string(), uuid_str);

        // Invalid UUID should error
        assert!(TrackId::from_string("invalid").is_err());
    }

    #[test]
    fn test_id_types_default() {
        let track_id = TrackId::default();
        let album_id = AlbumId::default();
        let artist_id = ArtistId::default();
        let playlist_id = PlaylistId::default();

        // Default should create new UUIDs
        assert!(!track_id.to_string().is_empty());
        assert!(!album_id.to_string().is_empty());
        assert!(!artist_id.to_string().is_empty());
        assert!(!playlist_id.to_string().is_empty());
    }
}
