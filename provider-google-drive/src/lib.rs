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

pub mod error;

pub use error::{GoogleDriveError, Result};
