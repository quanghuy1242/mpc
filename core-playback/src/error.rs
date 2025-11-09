//! # Playback Error Types
//!
//! Comprehensive error types for audio playback operations.

use thiserror::Error;

/// Errors that can occur during playback operations.
#[derive(Error, Debug)]
pub enum PlaybackError {
    // ========================================================================
    // Source Errors
    // ========================================================================
    /// Track was not found in the library or storage provider.
    #[error("Track not found: {0}")]
    TrackNotFound(String),

    /// Failed to open or read audio source.
    #[error("Failed to open audio source: {0}")]
    SourceError(String),

    /// Audio source is unavailable (e.g., network error, file deleted).
    #[error("Audio source unavailable: {0}")]
    SourceUnavailable(String),

    // ========================================================================
    // Format/Codec Errors
    // ========================================================================
    /// Audio format is not recognized or cannot be parsed.
    #[error("Unsupported or invalid audio format: {0}")]
    InvalidFormat(String),

    /// Codec is not supported by the decoder.
    #[error("Unsupported codec: {0}")]
    UnsupportedCodec(String),

    /// Audio format was detected but cannot be decoded.
    #[error("Cannot decode audio format: {0}")]
    FormatNotDecodable(String),

    // ========================================================================
    // Decoding Errors
    // ========================================================================
    /// Error occurred during audio decoding.
    #[error("Decoding error: {0}")]
    DecodingError(String),

    /// Audio stream is corrupted or contains invalid data.
    #[error("Corrupted audio stream: {0}")]
    CorruptedStream(String),

    /// Decoder encountered an internal error.
    #[error("Decoder internal error: {0}")]
    DecoderError(String),

    /// End of stream reached unexpectedly.
    #[error("Unexpected end of stream")]
    UnexpectedEndOfStream,

    // ========================================================================
    // Streaming Errors
    // ========================================================================
    /// Network streaming failed.
    #[error("Streaming failed: {0}")]
    StreamingFailed(String),

    /// Stream buffer underrun occurred.
    #[error("Buffer underrun")]
    BufferUnderrun,

    /// Stream buffer is full, cannot accept more data.
    #[error("Buffer overflow")]
    BufferOverflow,

    // ========================================================================
    // Playback Control Errors
    // ========================================================================
    /// Seeking is not supported for this audio source.
    #[error("Seeking not supported")]
    SeekNotSupported,

    /// Seek position is out of bounds.
    #[error("Seek position out of bounds: {0:?}")]
    SeekOutOfBounds(std::time::Duration),

    /// Playback operation failed.
    #[error("Playback operation failed: {0}")]
    PlaybackFailed(String),

    /// Attempted operation when no track is loaded.
    #[error("No track loaded")]
    NoTrackLoaded,

    /// Invalid volume value (must be in range [0.0, 1.0]).
    #[error("Invalid volume: {0} (must be between 0.0 and 1.0)")]
    InvalidVolume(f32),

    // ========================================================================
    // Cache Errors
    // ========================================================================
    /// Offline cache operation failed.
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Track is not available in offline cache.
    #[error("Track not cached: {0}")]
    NotCached(String),

    /// Cache storage is full.
    #[error("Cache storage full")]
    CacheFull,

    /// Cache encryption/decryption failed.
    #[error("Cache encryption error: {0}")]
    EncryptionError(String),

    // ========================================================================
    // Platform/Adapter Errors
    // ========================================================================
    /// Platform adapter is not initialized.
    #[error("Playback adapter not initialized")]
    AdapterNotInitialized,

    /// Platform audio device is unavailable.
    #[error("Audio device unavailable: {0}")]
    AudioDeviceUnavailable(String),

    /// Platform audio device encountered an error.
    #[error("Audio device error: {0}")]
    AudioDeviceError(String),

    // ========================================================================
    // Authentication Errors
    // ========================================================================
    /// User is not authenticated.
    #[error("Not authenticated")]
    NotAuthenticated,

    /// Insufficient permissions to access audio resource.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    // ========================================================================
    // Generic Errors
    // ========================================================================
    /// I/O error occurred.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Library error from core-library.
    #[cfg(feature = "offline-cache")]
    #[error("Library error: {0}")]
    LibraryError(#[from] core_library::error::LibraryError),

    /// Internal error (should not occur in normal operation).
    #[error("Internal error: {0}")]
    Internal(String),
}

impl PlaybackError {
    /// Returns `true` if this error is transient and the operation can be retried.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            PlaybackError::StreamingFailed(_)
                | PlaybackError::BufferUnderrun
                | PlaybackError::SourceUnavailable(_)
                | PlaybackError::AudioDeviceUnavailable(_)
        )
    }

    /// Returns `true` if this error is due to network issues.
    pub fn is_network_error(&self) -> bool {
        matches!(
            self,
            PlaybackError::StreamingFailed(_) | PlaybackError::SourceUnavailable(_)
        )
    }

    /// Returns `true` if this error is related to audio format/codec issues.
    pub fn is_format_error(&self) -> bool {
        matches!(
            self,
            PlaybackError::InvalidFormat(_)
                | PlaybackError::UnsupportedCodec(_)
                | PlaybackError::FormatNotDecodable(_)
        )
    }
}

/// Result type for playback operations.
pub type Result<T> = std::result::Result<T, PlaybackError>;
