//! # Google Drive Provider
//!
//! Implements `StorageProvider` trait for Google Drive API v3.
//!
//! ## Overview
//!
//! This module provides:
//! - OAuth 2.0 authentication with Google Drive
//! - File listing with audio filtering by MIME type
//! - Streaming downloads with range request support
//! - Incremental sync using change tokens
//! - Rate limiting and exponential backoff
//!
//! ## Example
//!
//! ```ignore
//! use provider_google_drive::GoogleDriveConnector;
//! use bridge_traits::storage::StorageProvider;
//! use std::sync::Arc;
//!
//! # async fn example(http_client: Arc<dyn HttpClient>) -> Result<()> {
//! let connector = GoogleDriveConnector::new(http_client, "access_token".to_string());
//!
//! // List all files
//! let (files, next_cursor) = connector.list_media(None).await?;
//! println!("Found {} files", files.len());
//!
//! // Download a file
//! let data = connector.download(&files[0].id, None).await?;
//! println!("Downloaded {} bytes", data.len());
//!
//! // Get incremental changes
//! if let Some(cursor) = next_cursor {
//!     let (changes, new_cursor) = connector.get_changes(Some(cursor)).await?;
//!     println!("Found {} changes", changes.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Authentication
//!
//! The connector requires a valid OAuth 2.0 access token with the `drive.readonly` scope.
//! Token management is handled by the `core-auth` module.
//!
//! ## Rate Limiting
//!
//! Google Drive API has the following limits:
//! - 12,000 queries per minute per user
//! - 1,000 queries per 100 seconds per user
//!
//! The connector implements exponential backoff for 429 (rate limit) and 5xx (server error) responses.

pub mod connector;
pub mod error;
pub mod types;

pub use connector::GoogleDriveConnector;
pub use error::{GoogleDriveError, Result};
pub use types::{ChangesListResponse, DriveFile, FilesListResponse, StartPageTokenResponse};
