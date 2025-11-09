//! # Repository Pattern Implementation
//!
//! This module provides repository traits and implementations for data access.
//! Each entity has a corresponding repository with CRUD operations, querying,
//! and pagination support.
//!
//! ## Architecture
//!
//! - Traits define the interface for each repository
//! - SQLite implementations use sqlx for async database access
//! - All operations return `Result<T>` for error handling
//! - Pagination is supported via the `Page<T>` wrapper
//!
//! ## Available Repositories
//!
//! - `TrackRepository` - Music tracks with metadata and audio properties
//! - `AlbumRepository` - Albums with artist relationships
//! - `ArtistRepository` - Music artists with biographical info
//! - `PlaylistRepository` - User and system playlists with track management
//! - `FolderRepository` - Cloud storage folder hierarchy
//! - `ArtworkRepository` - Album artwork with deduplication support
//! - `LyricsRepository` - Track lyrics (plain text and synced LRC format)

// Platform-conditional Arc type (Rc for WASM, Arc for native)
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::sync::Arc as PlatformArc;
#[cfg(target_arch = "wasm32")]
pub(crate) use std::rc::Rc as PlatformArc;

pub mod album;
pub mod artist;
pub mod artwork;
pub mod cache;
pub mod folder;
pub mod lyrics;
pub mod pagination;
pub mod playlist;
pub mod track;

pub use album::{AlbumRepository, SqliteAlbumRepository};
pub use artist::{ArtistRepository, SqliteArtistRepository};
pub use artwork::{ArtworkRepository, SqliteArtworkRepository};
pub use cache::{CacheMetadataRepository, SqliteCacheMetadataRepository};
pub use folder::{FolderRepository, SqliteFolderRepository};
pub use lyrics::{LyricsRepository, SqliteLyricsRepository};
pub use pagination::{Page, PageRequest};
pub use playlist::{PlaylistRepository, SqlitePlaylistRepository};
pub use track::{SqliteTrackRepository, TrackRepository};
