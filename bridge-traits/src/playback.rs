//! Playback bridge traits and supporting audio types.
//!
//! These abstractions allow the core playback module to interact with
//! platform-specific audio engines and decoder backends while preserving a
//! consistent, async-first API surface. Host applications are expected to
//! provide concrete implementations that satisfy their platform constraints
//! (desktop, mobile, web).

use crate::{
    error::{BridgeError, Result},
    platform::{PlatformSend, PlatformSendSync},
};
use bytes::Bytes;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Supported audio codec identifiers.
///
/// This enum is intentionally extensible; use [`AudioCodec::Other`] for codecs
/// not explicitly listed here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AudioCodec {
    Mp3,
    Aac,
    Flac,
    Vorbis,
    Opus,
    Wav,
    Alac,
    /// Codec is unknown or not yet mapped to a dedicated variant.
    Unknown,
    /// Vendor- or platform-specific codec.
    Other(String),
}

/// Stream metadata describing the decoded PCM format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFormat {
    /// Codec identifier associated with the source.
    pub codec: AudioCodec,
    /// Sample rate in hertz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u16,
    /// Bits per sample, when known.
    pub bits_per_sample: Option<u16>,
    /// Average bitrate in kbps, when reported by the decoder.
    pub bitrate: Option<u32>,
}

impl AudioFormat {
    /// Create a new audio format description.
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
}

/// High-level audio source descriptor provided to playback adapters.
#[derive(Debug, Clone)]
pub enum AudioSource {
    /// Local file accessible to the host runtime.
    LocalFile { path: PathBuf },
    /// Remote HTTP(S) stream to be fetched by the host.
    RemoteStream {
        url: String,
        headers: HashMap<String, String>,
    },
    /// In-memory audio buffer supplied by the caller.
    MemoryBuffer { data: Bytes },
}

impl AudioSource {
    /// Determine whether the source represents remote content.
    pub fn is_remote(&self) -> bool {
        matches!(self, AudioSource::RemoteStream { .. })
    }
}

/// Additional playback options supplied alongside a request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackOptions {
    /// Initial playback position (defaults to start of stream).
    pub start_position: Duration,
    /// Initial volume (0.0 = muted, 1.0 = unity gain).
    pub initial_volume: f32,
    /// Whether playback should loop automatically.
    pub looping: bool,
    /// Hint for adapters to pre-buffer audio this many milliseconds ahead.
    pub prebuffer_duration: Duration,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            start_position: Duration::from_secs(0),
            initial_volume: 1.0,
            looping: false,
            prebuffer_duration: Duration::from_millis(500),
        }
    }
}

/// Unique identifier for playback sessions managed by a host adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaybackSessionId(Uuid);

impl PlaybackSessionId {
    /// Generate a new session identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Construct an identifier from an existing UUID.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for PlaybackSessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Playback lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Preparing,
    Playing,
    Paused,
    Stopped,
    Completed,
    Error { message: String },
}

/// Metadata associated with a playback request. Implementations may use this to
/// enrich platform media sessions or notification center entries.
#[derive(Debug, Clone, Default)]
pub struct PlaybackMetadata {
    /// Optional opaque track identifier.
    pub track_id: Option<String>,
    /// Display title for the track.
    pub title: Option<String>,
    /// Display artist string.
    pub artist: Option<String>,
    /// Album or collection name.
    pub album: Option<String>,
    /// Arbitrary extra fields (e.g., artwork URI, genre).
    pub extra: HashMap<String, String>,
}

/// Request describing the desired playback session a host adapter should provision.
#[derive(Debug, Clone)]
pub struct PlaybackRequest {
    /// Source to feed into the adapter.
    pub source: AudioSource,
    /// Audio format the downstream decoder will emit.
    pub format: AudioFormat,
    /// Playback options such as initial volume or start position.
    pub options: PlaybackOptions,
    /// Optional metadata surfaced to the host.
    pub metadata: PlaybackMetadata,
}

impl PlaybackRequest {
    /// Construct a new playback request with the provided source and format.
    pub fn new(source: AudioSource, format: AudioFormat) -> Self {
        Self {
            source,
            format,
            options: PlaybackOptions::default(),
            metadata: PlaybackMetadata::default(),
        }
    }

    /// Attach playback options to the request.
    pub fn with_options(mut self, options: PlaybackOptions) -> Self {
        self.options = options;
        self
    }

    /// Attach metadata to the request.
    pub fn with_metadata(mut self, metadata: PlaybackMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Chunk of decoded PCM frames produced by an [`AudioDecoder`].
#[derive(Debug, Clone)]
pub struct AudioFrameChunk {
    /// Interleaved PCM samples in the range `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    /// Number of frames represented by `samples` (frame = sample per channel).
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

    /// Returns `true` if the chunk contains no sample data.
    pub fn is_empty(&self) -> bool {
        self.frames == 0 || self.samples.is_empty()
    }
}

/// Result returned by [`AudioDecoder::probe`], containing format and stream metadata.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Resolved output format for decoded PCM.
    pub format: AudioFormat,
    /// Total duration of the stream, if known.
    pub duration: Option<Duration>,
    /// Key-value metadata such as title, album, artist, etc.
    pub tags: HashMap<String, String>,
}

impl ProbeResult {
    /// Create a new probe result with the provided format.
    pub fn new(format: AudioFormat) -> Self {
        Self {
            format,
            duration: None,
            tags: HashMap::new(),
        }
    }

    /// Attach stream duration.
    pub fn with_duration(mut self, duration: Option<Duration>) -> Self {
        self.duration = duration;
        self
    }

    /// Attach tags.
    pub fn with_tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Trait for platform-specific playback adapters that drive native audio engines.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PlaybackAdapter: PlatformSendSync {
    /// Prepare a playback session. Implementations may allocate native resources,
    /// configure routes, or queue buffers. Returns a session identifier that
    /// subsequent control calls reference.
    async fn prepare(&self, request: PlaybackRequest) -> Result<PlaybackSessionId>;

    /// Begin or resume playback for the provided session.
    async fn play(&self, session: PlaybackSessionId) -> Result<()>;

    /// Pause playback without releasing the session.
    async fn pause(&self, session: PlaybackSessionId) -> Result<()>;

    /// Stop playback and reset position to the start of the stream.
    async fn stop(&self, session: PlaybackSessionId) -> Result<()>;

    /// Seek to an absolute position within the stream.
    async fn seek(&self, session: PlaybackSessionId, position: Duration) -> Result<()>;

    /// Adjust playback volume. Volume is normalized to `0.0..=1.0`.
    async fn set_volume(&self, session: PlaybackSessionId, volume: f32) -> Result<()>;

    /// Query the current playback position.
    async fn get_position(&self, session: PlaybackSessionId) -> Result<Duration>;

    /// Fetch the adapter's current understanding of the session state.
    async fn state(&self, session: PlaybackSessionId) -> Result<PlaybackState>;

    /// Release resources associated with a playback session.
    async fn unload(&self, session: PlaybackSessionId) -> Result<()>;
}

/// Trait for decoder implementations capable of producing PCM frames from an audio source.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait AudioDecoder: PlatformSend {
    /// Inspect the underlying stream and return format metadata.
    async fn probe(&mut self) -> Result<ProbeResult>;

    /// Decode up to `max_frames` frames from the current position. Returns
    /// `Ok(None)` when the end of stream has been reached.
    async fn decode_frames(&mut self, max_frames: usize) -> Result<Option<AudioFrameChunk>>;

    /// Seek to the requested absolute position, when supported by the format.
    async fn seek(&mut self, position: Duration) -> Result<()>;
}

/// Convenience result type alias for playback operations.
pub type PlaybackResult<T> = std::result::Result<T, BridgeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_options_default_values() {
        let opts = PlaybackOptions::default();
        assert_eq!(opts.start_position, Duration::from_secs(0));
        assert_eq!(opts.initial_volume, 1.0);
        assert!(!opts.looping);
        assert_eq!(opts.prebuffer_duration, Duration::from_millis(500));
    }

    #[test]
    fn session_id_is_unique() {
        let a = PlaybackSessionId::new();
        let b = PlaybackSessionId::new();
        assert_ne!(a, b);
        assert_eq!(a, PlaybackSessionId::from_uuid(*a.as_uuid()));
    }

    #[test]
    fn audio_frame_chunk_empty() {
        let chunk = AudioFrameChunk::new(Vec::new(), 0, Duration::from_secs(0));
        assert!(chunk.is_empty());
    }
}
