//! # Sync & Indexing Module
//!
//! Orchestrates synchronization with cloud storage providers.
//!
//! ## Overview
//!
//! This module manages the lifecycle of sync jobs, including:
//! - Listing remote files via `StorageProvider`
//! - Filtering audio files by MIME type and extension
//! - Extracting metadata from downloaded files
//! - Resolving conflicts (renames, duplicates, deletions)
//! - Persisting library entries to the database

pub mod error;

pub use error::{Result, SyncError};
