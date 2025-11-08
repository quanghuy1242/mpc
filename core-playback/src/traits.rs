//! # Core Playback Traits
//!
//! This module defines the core abstractions for audio playback within the music platform.
//! These traits are designed for the **core logic layer** and differ from the bridge-traits
//! definitions which expose platform-specific adapters to the host.
//!
//! ## Architecture
//!
//! The playback system uses a **producer-consumer model**:
//!
//! - **Producer (AudioDecoder)**: Runs in a background thread/task, decodes audio data
//!   and places PCM samples into a shared buffer. This is typically run via the
//!   `StreamingService` coordinated by host platform's `BackgroundExecutor`.
//!
//! - **Consumer (PlaybackAdapter)**: Called from a high-priority audio thread, reads
//!   PCM samples from the shared buffer and feeds them to the platform's audio engine.
//!
//! ## Threading Model
//!
//! - On **native** platforms: `AudioDecoder` and `PlaybackAdapter` must be `Send + Sync`
//!   to support multi-threaded operation.
//! - On **WASM**: Single-threaded execution, traits use `?Send` from `async_trait`.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use core_playback::{AudioDecoder, AudioSource, AudioFormat, AudioCodec};
//! use std::time::Duration;
//!
//! async fn example_usage(mut decoder: impl AudioDecoder) {
//!     // Probe the audio stream
//!     let probe = decoder.probe().await.expect("Failed to probe");
//!     println!("Format: {:?}", probe.format);
//!     
//!     // Decode frames
//!     while let Some(chunk) = decoder.decode_frames(4096).await.expect("Decode error") {
//!         // Process PCM samples
//!         println!("Decoded {} frames", chunk.frames);
//!     }
//!     
//!     // Seek to position
//!     decoder.seek(Duration::from_secs(30)).await.expect("Seek failed");
//! }
//! ```

use crate::error::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// Audio Format Types
// ============================================================================

/// Supported audio codecs.
///
/// This enum covers all major audio formats used in music streaming and local
/// playback. Use [`AudioCodec::Other`] for platform-specific or proprietary codecs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    /// MPEG-1 Audio Layer 3
    Mp3,
    /// Advanced Audio Coding (AAC/M4A)
    Aac,
    /// Free Lossless Audio Codec
    Flac,
    /// Ogg Vorbis
    Vorbis,
    /// Opus (low-latency codec)
    Opus,
    /// Waveform Audio File Format
    Wav,
    /// Apple Lossless Audio Codec
    Alac,
    /// Codec not recognized or未知
    Unknown,
    /// Custom or proprietary codec
    Other(String),
}

impl AudioCodec {
    /// Returns `true` if this is a lossless codec.
    pub fn is_lossless(&self) -> bool {
        matches!(self, AudioCodec::Flac | AudioCodec::Wav | AudioCodec::Alac)
    }

    /// Returns `true` if this codec is lossy.
    pub fn is_lossy(&self) -> bool {
        matches!(
            self,
            AudioCodec::Mp3
                | AudioCodec::Aac
                | AudioCodec::Vorbis
                | AudioCodec::Opus
        )
    }
}

/// Audio format metadata describing decoded PCM output.
///
/// This struct provides all the information needed to correctly interpret
/// decoded audio samples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    /// Source codec (before decoding)
    pub codec: AudioCodec,
    /// Sample rate in Hz (e.g., 44100, 48000)
    pub sample_rate: u32,
    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    pub channels: u16,
    /// Bits per sample in the source format (e.g., 16, 24)
    pub bits_per_sample: Option<u16>,
    /// Average bitrate in kbps (for lossy codecs)
    pub bitrate: Option<u32>,
}

impl AudioFormat {
    /// Create a new audio format descriptor.
    pub fn new(
        codec: AudioCodec,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: Option<u16>,
        bitrate: Option<u32>,
    ) -> Self {
        Self {
            codec,
            sample_rate,
            channels,
            bits_per_sample,
            bitrate,
        }
    }

    /// Standard CD quality (44.1 kHz, 16-bit stereo)
    pub fn cd_quality() -> Self {
        Self {
            codec: AudioCodec::Wav,
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: Some(16),
            bitrate: None,
        }
    }

    /// High-resolution audio (96 kHz, 24-bit stereo)
    pub fn hi_res() -> Self {
        Self {
            codec: AudioCodec::Flac,
            sample_rate: 96000,
            channels: 2,
            bits_per_sample: Some(24),
            bitrate: None,
        }
    }
}

// ============================================================================
// Audio Source Types
// ============================================================================

/// Source of audio data for decoding and playback.
///
/// This enum abstracts different audio sources:
/// - Local files from the filesystem
/// - Remote streams from HTTP(S) URLs
/// - In-memory cached chunks
#[derive(Debug, Clone)]
pub enum AudioSource {
    /// Audio file stored locally on the filesystem.
    LocalFile {
        /// Absolute path to the audio file
        path: PathBuf,
    },

    /// Audio stream from a remote HTTP(S) endpoint.
    RemoteStream {
        /// Full URL to the audio resource
        url: String,
        /// HTTP headers to include in the request (e.g., Authorization)
        headers: HashMap<String, String>,
    },

    /// Pre-fetched audio data stored in memory.
    ///
    /// This is typically used for:
    /// - Small audio files that fit in memory
    /// - Cached chunks from offline storage
    /// - Pre-buffered segments for gapless playback
    CachedChunk {
        /// Raw audio data (encoded format, not PCM)
        data: Bytes,
        /// Optional hint about the source codec
        codec_hint: Option<AudioCodec>,
    },
}

impl AudioSource {
    /// Returns `true` if this source requires network access.
    pub fn is_remote(&self) -> bool {
        matches!(self, AudioSource::RemoteStream { .. })
    }

    /// Returns `true` if the audio data is already in memory.
    pub fn is_cached(&self) -> bool {
        matches!(self, AudioSource::CachedChunk { .. })
    }

    /// Returns the estimated size in bytes, if known.
    pub fn estimated_size(&self) -> Option<usize> {
        match self {
            AudioSource::CachedChunk { data, .. } => Some(data.len()),
            _ => None,
        }
    }
}

// ============================================================================
// Decoded Audio Data
// ============================================================================

/// A chunk of decoded PCM audio frames.
///
/// This struct represents the output of [`AudioDecoder::decode_frames()`].
/// Samples are normalized to the range `[-1.0, 1.0]` and are interleaved
/// for multi-channel audio (e.g., stereo is LRLRLR...).
#[derive(Debug, Clone)]
pub struct AudioFrameChunk {
    /// Interleaved PCM samples normalized to [-1.0, 1.0].
    ///
    /// For stereo audio, samples are ordered: [L0, R0, L1, R1, L2, R2, ...]
    pub samples: Vec<f32>,

    /// Number of frames represented by this chunk.
    ///
    /// One frame = one sample per channel.
    /// For stereo: `frames = samples.len() / 2`
    pub frames: usize,

    /// Presentation timestamp for the first frame in this chunk.
    pub timestamp: Duration,
}

impl AudioFrameChunk {
    /// Create a new audio frame chunk.
    pub fn new(samples: Vec<f32>, frames: usize, timestamp: Duration) -> Self {
        Self {
            samples,
            frames,
            timestamp,
        }
    }

    /// Returns `true` if the chunk contains no audio data.
    pub fn is_empty(&self) -> bool {
        self.frames == 0 || self.samples.is_empty()
    }

    /// Returns the duration of this chunk based on sample rate.
    pub fn duration(&self, sample_rate: u32) -> Duration {
        if sample_rate == 0 {
            return Duration::from_secs(0);
        }
        let seconds = self.frames as f64 / sample_rate as f64;
        Duration::from_secs_f64(seconds)
    }
}

/// Result of probing an audio stream.
///
/// Contains format metadata and optional tags extracted from the audio container.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Decoded audio format
    pub format: AudioFormat,
    /// Total duration of the audio stream, if known
    pub duration: Option<Duration>,
    /// Metadata tags (e.g., title, artist, album)
    pub tags: HashMap<String, String>,
}

impl ProbeResult {
    /// Create a new probe result with the given format.
    pub fn new(format: AudioFormat) -> Self {
        Self {
            format,
            duration: None,
            tags: HashMap::new(),
        }
    }

    /// Set the stream duration.
    pub fn with_duration(mut self, duration: Option<Duration>) -> Self {
        self.duration = duration;
        self
    }

    /// Set metadata tags.
    pub fn with_tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags = tags;
        self
    }
}

// ============================================================================
// Core Traits
// ============================================================================

/// Trait for audio decoders that convert encoded audio into PCM samples.
///
/// This trait is designed to run in a **producer thread** as part of the
/// streaming service. The decoder reads encoded audio data and outputs
/// interleaved f32 PCM samples in the range [-1.0, 1.0].
///
/// ## Threading Model
///
/// - **Native**: Must be `Send` to move between threads
/// - **WASM**: Single-threaded, uses `?Send` from async_trait
///
/// ## Implementation Notes
///
/// - Implementations should buffer decoded frames internally for efficiency
/// - `decode_frames()` should return chunks of the requested size when possible
/// - Seeking may not be supported by all formats (return error if unsupported)
/// - End of stream is indicated by returning `Ok(None)`
///
/// ## Example
///
/// ```rust,no_run
/// # use core_playback::{AudioDecoder, AudioSource, AudioFormat, AudioCodec, ProbeResult};
/// # use std::time::Duration;
/// # struct MyDecoder;
/// # #[async_trait::async_trait]
/// # impl AudioDecoder for MyDecoder {
/// #     async fn probe(&mut self) -> core_playback::Result<ProbeResult> { unimplemented!() }
/// #     async fn decode_frames(&mut self, max_frames: usize) -> core_playback::Result<Option<core_playback::AudioFrameChunk>> { unimplemented!() }
/// #     async fn seek(&mut self, position: Duration) -> core_playback::Result<()> { unimplemented!() }
/// # }
/// async fn decode_audio(mut decoder: impl AudioDecoder) {
///     let probe = decoder.probe().await.unwrap();
///     println!("Codec: {:?}", probe.format.codec);
///     
///     while let Some(chunk) = decoder.decode_frames(4096).await.unwrap() {
///         // Feed samples to playback adapter or ring buffer
///         println!("Got {} frames at {:?}", chunk.frames, chunk.timestamp);
///     }
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait AudioDecoder {
    /// Probe the audio stream and return format metadata.
    ///
    /// This should be called once before decoding begins. It inspects the
    /// audio container/stream to determine:
    /// - Codec type
    /// - Sample rate and channel count
    /// - Duration (if available)
    /// - Embedded metadata tags
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The audio format is not recognized
    /// - The stream is corrupted
    /// - Required codec is not supported
    async fn probe(&mut self) -> Result<ProbeResult>;

    /// Decode up to `max_frames` audio frames from the current position.
    ///
    /// Returns a chunk of decoded PCM samples as interleaved f32 values in
    /// the range [-1.0, 1.0]. Returns `Ok(None)` when the end of stream is reached.
    ///
    /// # Arguments
    ///
    /// * `max_frames` - Maximum number of frames to decode. Actual returned
    ///   frames may be less due to internal buffering or end of stream.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(chunk))` - Successfully decoded audio data
    /// - `Ok(None)` - End of stream reached
    /// - `Err(...)` - Decoding error occurred
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The stream contains corrupted data
    /// - A codec error occurs
    /// - The source is no longer available
    async fn decode_frames(&mut self, max_frames: usize) -> Result<Option<AudioFrameChunk>>;

    /// Seek to an absolute position in the audio stream.
    ///
    /// After seeking, the next call to `decode_frames()` will return audio
    /// from the requested position.
    ///
    /// # Arguments
    ///
    /// * `position` - Absolute timestamp to seek to
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Seeking is not supported by this format
    /// - The requested position is out of bounds
    /// - A codec error occurs during seek
    async fn seek(&mut self, position: Duration) -> Result<()>;
}

/// Trait for platform-specific playback adapters.
///
/// This trait abstracts the platform's audio engine and is designed to be
/// called from a **high-priority consumer thread**. The adapter reads PCM
/// samples from a shared buffer and feeds them to the native audio system.
///
/// ## Threading Model
///
/// - **Native**: Must be `Send + Sync` for multi-threaded operation
/// - **WASM**: Single-threaded, uses `?Send` from async_trait
///
/// ## Design Considerations
///
/// - Playback control methods (play, pause, seek) should be fast and non-blocking
/// - Implementations should handle buffer underruns gracefully
/// - Volume changes should apply smoothly without clicks/pops
/// - Position queries should be accurate and updated frequently
///
/// ## Example
///
/// ```rust,no_run
/// # use core_playback::{PlaybackAdapter, AudioSource, AudioFormat, AudioCodec};
/// # use std::time::Duration;
/// # struct MyAdapter;
/// # #[async_trait::async_trait]
/// # impl PlaybackAdapter for MyAdapter {
/// #     async fn play(&self, source: AudioSource, format: AudioFormat) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn pause(&self) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn resume(&self) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn stop(&self) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn seek(&self, position: Duration) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn set_volume(&self, volume: f32) -> core_playback::Result<()> { unimplemented!() }
/// #     async fn get_position(&self) -> core_playback::Result<Duration> { unimplemented!() }
/// #     async fn is_playing(&self) -> core_playback::Result<bool> { unimplemented!() }
/// # }
/// async fn playback_control(adapter: impl PlaybackAdapter) {
///     let source = AudioSource::LocalFile {
///         path: "/path/to/song.mp3".into(),
///     };
///     let format = AudioFormat::cd_quality();
///     
///     adapter.play(source, format).await.unwrap();
///     adapter.set_volume(0.8).await.unwrap();
///     
///     let pos = adapter.get_position().await.unwrap();
///     println!("Current position: {:?}", pos);
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait PlaybackAdapter {
    /// Begin playback of an audio source.
    ///
    /// This initializes the playback session and starts playing audio from
    /// the beginning (or `start_position` if specified in options).
    ///
    /// # Arguments
    ///
    /// * `source` - The audio source to play
    /// * `format` - Expected audio format (for configuration)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The source cannot be opened
    /// - The format is not supported
    /// - The audio device is unavailable
    async fn play(&self, source: AudioSource, format: AudioFormat) -> Result<()>;

    /// Pause playback without releasing resources.
    ///
    /// The playback position is preserved. Use `resume()` to continue playback.
    async fn pause(&self) -> Result<()>;

    /// Resume playback from the paused position.
    async fn resume(&self) -> Result<()>;

    /// Stop playback and release resources.
    ///
    /// After stopping, `play()` must be called to start a new playback session.
    async fn stop(&self) -> Result<()>;

    /// Seek to an absolute position in the current track.
    ///
    /// # Arguments
    ///
    /// * `position` - Absolute timestamp to seek to
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No track is currently loaded
    /// - The position is out of bounds
    /// - Seeking is not supported
    async fn seek(&self, position: Duration) -> Result<()>;

    /// Set playback volume.
    ///
    /// # Arguments
    ///
    /// * `volume` - Volume level in range [0.0, 1.0] where 0.0 is muted and 1.0 is maximum
    ///
    /// # Errors
    ///
    /// Returns an error if the volume is out of range or the adapter is not initialized.
    async fn set_volume(&self, volume: f32) -> Result<()>;

    /// Get the current playback position.
    ///
    /// Returns the elapsed time from the start of the current track.
    ///
    /// # Errors
    ///
    /// Returns an error if no track is currently loaded.
    async fn get_position(&self) -> Result<Duration>;

    /// Check if playback is currently active.
    ///
    /// Returns `true` if audio is currently playing (not paused or stopped).
    async fn is_playing(&self) -> Result<bool>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_codec_classification() {
        assert!(AudioCodec::Flac.is_lossless());
        assert!(AudioCodec::Wav.is_lossless());
        assert!(AudioCodec::Alac.is_lossless());

        assert!(AudioCodec::Mp3.is_lossy());
        assert!(AudioCodec::Aac.is_lossy());
        assert!(AudioCodec::Vorbis.is_lossy());
    }

    #[test]
    fn audio_format_presets() {
        let cd = AudioFormat::cd_quality();
        assert_eq!(cd.sample_rate, 44100);
        assert_eq!(cd.channels, 2);
        assert_eq!(cd.bits_per_sample, Some(16));

        let hi_res = AudioFormat::hi_res();
        assert_eq!(hi_res.sample_rate, 96000);
        assert_eq!(hi_res.channels, 2);
        assert_eq!(hi_res.bits_per_sample, Some(24));
    }

    #[test]
    fn audio_source_classification() {
        let local = AudioSource::LocalFile {
            path: "/path/to/file.mp3".into(),
        };
        assert!(!local.is_remote());
        assert!(!local.is_cached());

        let remote = AudioSource::RemoteStream {
            url: "https://example.com/stream".to_string(),
            headers: HashMap::new(),
        };
        assert!(remote.is_remote());
        assert!(!remote.is_cached());

        let cached = AudioSource::CachedChunk {
            data: Bytes::from_static(&[1, 2, 3, 4]),
            codec_hint: Some(AudioCodec::Mp3),
        };
        assert!(!cached.is_remote());
        assert!(cached.is_cached());
        assert_eq!(cached.estimated_size(), Some(4));
    }

    #[test]
    fn audio_frame_chunk_duration() {
        let chunk = AudioFrameChunk::new(
            vec![0.0; 8820], // 4410 frames * 2 channels
            4410,
            Duration::from_secs(0),
        );

        let duration = chunk.duration(44100);
        assert_eq!(duration.as_millis(), 100); // 4410 frames / 44100 Hz = 0.1s

        assert!(!chunk.is_empty());
    }

    #[test]
    fn probe_result_builder() {
        let format = AudioFormat::cd_quality();
        let mut tags = HashMap::new();
        tags.insert("title".to_string(), "Test Song".to_string());

        let probe = ProbeResult::new(format.clone())
            .with_duration(Some(Duration::from_secs(180)))
            .with_tags(tags.clone());

        assert_eq!(probe.format, format);
        assert_eq!(probe.duration, Some(Duration::from_secs(180)));
        assert_eq!(probe.tags.get("title"), Some(&"Test Song".to_string()));
    }
}
