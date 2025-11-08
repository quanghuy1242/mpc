//! # Core Playback Module
//!
//! Provides comprehensive audio playback and streaming functionality for the music platform.
//!
//! ## Overview
//!
//! This crate provides the core abstractions and implementations for:
//!
//! - **Audio Decoding**: Convert encoded audio (MP3, AAC, FLAC, etc.) to PCM samples
//! - **Playback Control**: Platform-agnostic playback adapter trait
//! - **Streaming Service**: Producer-consumer architecture for efficient audio streaming
//! - **Offline Cache**: Optional encrypted cache for offline playback
//!
//! ## Architecture
//!
//! The playback system uses a **producer-consumer model**:
//!
//! ```text
//! ┌─────────────────────┐
//! │  StreamingService   │  (Background Thread - Producer)
//! │   AudioDecoder      │
//! └──────────┬──────────┘
//!            │ PCM Samples
//!            ▼
//! ┌─────────────────────┐
//! │    Ring Buffer      │  (Shared, Thread-Safe)
//! └──────────┬──────────┘
//!            │ PCM Samples
//!            ▼
//! ┌─────────────────────┐
//! │  PlaybackAdapter    │  (Audio Thread - Consumer)
//! │  Platform Audio     │
//! └─────────────────────┘
//! ```
//!
//! ## Features
//!
//! - `core-decoder` (default): Include Symphonia-based audio decoder
//! - `offline-cache`: Enable encrypted offline cache with AES-GCM
//!
//! ## Cross-Platform Support
//!
//! This crate is designed to work on both native and WASM targets:
//!
//! - **Native**: Full multi-threaded operation with `Send + Sync` traits
//! - **WASM**: Single-threaded operation with `?Send` async traits
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use core_playback::{AudioDecoder, AudioSource, AudioCodec, AudioFormat};
//! use std::time::Duration;
//!
//! async fn example_playback(mut decoder: impl AudioDecoder) {
//!     // Probe audio format
//!     let probe = decoder.probe().await.expect("Failed to probe");
//!     println!("Format: {:?}", probe.format);
//!     println!("Duration: {:?}", probe.duration);
//!     
//!     // Decode and play audio
//!     while let Some(chunk) = decoder.decode_frames(4096).await.expect("Decode error") {
//!         // Feed PCM samples to playback adapter or ring buffer
//!         println!("Decoded {} frames at {:?}", chunk.frames, chunk.timestamp);
//!     }
//! }
//! ```

pub mod cache;
pub mod config;
#[cfg(feature = "core-decoder")]
pub mod decoder;
pub mod error;
pub mod ring_buffer;
pub mod streaming;
pub mod traits;

// WASM bindings
#[cfg(target_arch = "wasm32")]
pub mod wasm;

// Re-export commonly used types
pub use config::{StreamingConfig, StreamingState, StreamingStats};
#[cfg(feature = "core-decoder")]
pub use decoder::{FormatDetector, SampleConverter, SymphoniaDecoder};
pub use error::{PlaybackError, Result};
pub use ring_buffer::RingBuffer;
pub use streaming::{StreamingRequest, StreamingService};
pub use traits::{
    AudioCodec, AudioDecoder, AudioFormat, AudioFrameChunk, AudioSource, PlaybackAdapter,
    ProbeResult,
};
