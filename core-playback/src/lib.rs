//! # Playback & Streaming Module
//!
//! Provides streaming API and audio decoding for track playback.
//!
//! ## Overview
//!
//! This module handles:
//! - Streaming API for local and remote tracks
//! - Audio decoding using symphonia (optional, feature-gated)
//! - Adaptive buffering and prefetch
//! - Offline cache management with optional encryption

pub mod error;
pub mod traits;

pub use error::{PlaybackError, Result};
