//! # OneDrive Provider
//!
//! Implements `StorageProvider` trait for Microsoft Graph API (OneDrive).
//!
//! ## Overview
//!
//! This module provides:
//! - MSAL authentication with OneDrive
//! - File listing with audio filtering by extension
//! - Streaming downloads with range request support
//! - Delta sync using Microsoft Graph delta queries
//! - Throttling per Graph API guidelines

pub mod error;

pub use error::{OneDriveError, Result};
