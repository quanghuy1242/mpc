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

pub mod error;
pub mod models;

pub use error::{LibraryError, Result};
