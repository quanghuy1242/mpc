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

pub mod db;
pub mod error;
pub mod models;
pub mod repositories;

pub use db::{create_pool, create_test_pool, DatabaseConfig};
pub use error::{LibraryError, Result};
pub use models::{AlbumId, ArtistId, PlaylistId, Track, TrackId};
pub use repositories::{Page, PageRequest, SqliteTrackRepository, TrackRepository};
