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

mod pagination;
mod track;

pub use pagination::{Page, PageRequest};
pub use track::{SqliteTrackRepository, TrackRepository};
