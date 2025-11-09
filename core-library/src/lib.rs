//! # Library Management Module
//!
//! Owns the canonical music library database and provides repository patterns
//! for data access.
//!
//! ## Overview
//!
//! This module manages:
//! - SQLite database schema and migrations
//! - Repository patterns for tracks, albums, artists, playlists
//! - Query APIs with filtering, sorting, and pagination
//! - Full-text search using FTS5
//!
//! ## Database Abstraction
//!
//! The library uses the `DatabaseAdapter` trait from `bridge-traits` to abstract
//! database operations. This allows the library to work across different platforms:
//! - Native: SQLite via sqlx with native driver
//! - WebAssembly: SQLite via sql.js
//!
//! ### Usage Example
//!
//! ```ignore
//! use core_library::adapters::SqliteAdapter;
//! use bridge_traits::database::DatabaseConfig;
//!
//! // Create and initialize the adapter
//! let config = DatabaseConfig::new("music.db");
//! let mut adapter = SqliteAdapter::new(config).await?;
//! adapter.initialize().await?;
//! let repo = SqliteTrackRepository::new(Arc::new(adapter));
//! ```

pub mod adapters;
#[cfg(not(target_arch = "wasm32"))]
pub mod db;
pub mod error;
pub mod models;
pub mod query;
pub mod repositories;

// WASM bindings
#[cfg(target_arch = "wasm32")]
pub mod wasm;

// Re-export database adapter
#[cfg(not(target_arch = "wasm32"))]
pub use adapters::SqliteAdapter;

#[cfg(not(target_arch = "wasm32"))]
pub use db::{create_pool, create_test_pool, DatabaseConfig};
pub use error::{LibraryError, Result};
pub use models::{AlbumId, ArtistId, PlaylistId, Track, TrackId};
pub use query::{
    AlbumFilter, AlbumListItem, AlbumSearchItem, AlbumSort, ArtistSearchItem, LibraryQueryService,
    PlaylistSearchItem, SearchResults, TrackDetails, TrackFilter, TrackListItem, TrackSort,
};
pub use repositories::{Page, PageRequest, SqliteTrackRepository, TrackRepository};
