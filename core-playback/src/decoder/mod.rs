//! # Audio Decoder Module
//!
//! Production-ready audio decoding using the Symphonia library.
//!
//! ## Overview
//!
//! This module provides `SymphoniaDecoder`, a comprehensive implementation of the
//! `AudioDecoder` trait that supports all major audio formats through the Symphonia
//! pure-Rust audio library.
//!
//! ## Supported Formats
//!
//! | Format | Codec | Feature Flag | License | WASM Support |
//! |--------|-------|--------------|---------|--------------|
//! | MP3 | MPEG-1/2 Audio Layer III | `decoder-mp3` | Patents expired | ✅ |
//! | FLAC | Free Lossless Audio Codec | `decoder-flac` | BSD-3 | ✅ |
//! | Vorbis | Ogg Vorbis | `decoder-vorbis` | BSD-3 | ✅ |
//! | Opus | Opus in Ogg | `decoder-opus` | BSD-3 | ✅ |
//! | AAC | Advanced Audio Coding | `decoder-aac` | Patent-encumbered | ✅ |
//! | WAV | Waveform Audio | `decoder-wav` | Public domain | ✅ |
//! | ALAC | Apple Lossless | `decoder-alac` | Apache 2.0 | ✅ |
//!
//! ## Architecture
//!
//! The decoder uses Symphonia's three-layer architecture:
//!
//! 1. **MediaSource**: Abstracts file/stream I/O (file, HTTP, memory buffer)
//! 2. **FormatReader**: Demultiplexes containers, reads packets
//! 3. **Decoder**: Decodes packets to PCM samples
//!
//! ```text
//! AudioSource → MediaSourceStream → FormatReader → Decoder → AudioFrameChunk
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use core_playback::{AudioDecoder, AudioSource, SymphoniaDecoder};
//! use std::path::PathBuf;
//!
//! # async fn example() -> core_playback::Result<()> {
//! let source = AudioSource::LocalFile {
//!     path: PathBuf::from("/path/to/song.mp3"),
//! };
//!
//! let mut decoder = SymphoniaDecoder::new(source).await?;
//!
//! // Probe format
//! let probe = decoder.probe().await?;
//! println!("Format: {:?}, Duration: {:?}", probe.format.codec, probe.duration);
//!
//! // Decode audio
//! while let Some(chunk) = decoder.decode_frames(4096).await? {
//!     // Process PCM samples (f32, interleaved, [-1.0, 1.0])
//!     println!("Decoded {} frames", chunk.frames);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Performance
//!
//! - **Native**: ~5-10% CPU for 320kbps MP3 decoding on modern hardware
//! - **WASM**: ~15-25% overhead compared to native (still real-time capable)
//! - **Memory**: ~1-2MB per decoder instance (includes buffers)
//!
//! ## Threading Model
//!
//! - **Native**: Decoder is `Send` and can run in background threads
//! - **WASM**: Single-threaded, runs in Web Worker or main thread
//! - Designed for producer-consumer pattern with ring buffer

#[cfg(feature = "core-decoder")]
mod format_detector;

#[cfg(feature = "core-decoder")]
mod sample_converter;

#[cfg(feature = "core-decoder")]
mod symphonia;

#[cfg(feature = "core-decoder")]
pub use self::symphonia::SymphoniaDecoder;

#[cfg(feature = "core-decoder")]
pub use format_detector::FormatDetector;

#[cfg(feature = "core-decoder")]
pub use sample_converter::SampleConverter;

// Re-export when decoder is not available
#[cfg(not(feature = "core-decoder"))]
compile_error!(
    "Audio decoder feature is not enabled. Enable one of: \
     'decoder-mp3', 'decoder-flac', 'decoder-vorbis', 'decoder-opus', \
     'decoder-aac', 'decoder-wav', 'decoder-alac', or 'decoder-all'"
);
