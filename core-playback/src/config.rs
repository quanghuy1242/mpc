//! # Streaming Configuration
//!
//! Configuration types for the audio streaming service.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Streaming service configuration.
///
/// Controls buffer sizes, prefetching behavior, chunk sizes, and timeout settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Target buffer size in frames (one frame = samples for all channels).
    ///
    /// Example: For 2 seconds of 44.1kHz stereo: `88200 * 2 = 176400` samples,
    /// or `88200` frames.
    ///
    /// Default: 2 seconds of CD-quality audio (88200 frames).
    #[serde(default = "default_buffer_frames")]
    pub buffer_frames: usize,

    /// Minimum buffer level (in frames) before playback can start.
    ///
    /// Default: 0.5 seconds (22050 frames at 44.1kHz).
    #[serde(default = "default_min_buffer_frames")]
    pub min_buffer_frames: usize,

    /// Buffer level (fraction, 0.0-1.0) that triggers prefetching more data.
    ///
    /// When buffer falls below this level, the service will download and decode
    /// more aggressively.
    ///
    /// Default: 0.3 (30% full).
    #[serde(default = "default_prefetch_threshold")]
    pub prefetch_threshold: f32,

    /// Number of frames to decode per cycle.
    ///
    /// Larger values reduce overhead but increase latency.
    ///
    /// Default: 4096 frames (~93ms at 44.1kHz).
    #[serde(default = "default_decode_chunk_frames")]
    pub decode_chunk_frames: usize,

    /// HTTP chunk size for streaming downloads (in bytes).
    ///
    /// Default: 256 KB.
    #[serde(default = "default_http_chunk_bytes")]
    pub http_chunk_bytes: usize,

    /// Maximum duration to wait for HTTP response.
    ///
    /// Default: 30 seconds.
    #[serde(default = "default_http_timeout")]
    pub http_timeout: Duration,

    /// Maximum duration to wait for decoder to produce frames.
    ///
    /// Default: 10 seconds.
    #[serde(default = "default_decode_timeout")]
    pub decode_timeout: Duration,

    /// Whether to enable adaptive bitrate streaming.
    ///
    /// If enabled, the service will monitor network conditions and
    /// adjust streaming quality dynamically.
    ///
    /// Default: true.
    #[serde(default = "default_enable_adaptive_streaming")]
    pub enable_adaptive_streaming: bool,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            buffer_frames: default_buffer_frames(),
            min_buffer_frames: default_min_buffer_frames(),
            prefetch_threshold: default_prefetch_threshold(),
            decode_chunk_frames: default_decode_chunk_frames(),
            http_chunk_bytes: default_http_chunk_bytes(),
            http_timeout: default_http_timeout(),
            decode_timeout: default_decode_timeout(),
            enable_adaptive_streaming: default_enable_adaptive_streaming(),
        }
    }
}

impl StreamingConfig {
    /// Create a configuration optimized for low latency.
    ///
    /// - Smaller buffer (0.5s)
    /// - Aggressive prefetching (50% threshold)
    /// - Smaller decode chunks
    pub fn low_latency() -> Self {
        Self {
            buffer_frames: 22050,        // 0.5s at 44.1kHz
            min_buffer_frames: 11025,    // 0.25s
            prefetch_threshold: 0.5,     // 50%
            decode_chunk_frames: 2048,   // ~46ms
            http_chunk_bytes: 128 * 1024, // 128 KB
            ..Default::default()
        }
    }

    /// Create a configuration optimized for high quality/stability.
    ///
    /// - Larger buffer (5s)
    /// - Conservative prefetching (20% threshold)
    /// - Larger decode chunks for efficiency
    pub fn high_quality() -> Self {
        Self {
            buffer_frames: 220500,       // 5s at 44.1kHz
            min_buffer_frames: 88200,    // 2s
            prefetch_threshold: 0.2,     // 20%
            decode_chunk_frames: 8192,   // ~186ms
            http_chunk_bytes: 512 * 1024, // 512 KB
            ..Default::default()
        }
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<(), String> {
        if self.buffer_frames == 0 {
            return Err("buffer_frames must be > 0".to_string());
        }

        if self.min_buffer_frames > self.buffer_frames {
            return Err("min_buffer_frames cannot exceed buffer_frames".to_string());
        }

        if !(0.0..=1.0).contains(&self.prefetch_threshold) {
            return Err("prefetch_threshold must be between 0.0 and 1.0".to_string());
        }

        if self.decode_chunk_frames == 0 {
            return Err("decode_chunk_frames must be > 0".to_string());
        }

        if self.http_chunk_bytes == 0 {
            return Err("http_chunk_bytes must be > 0".to_string());
        }

        Ok(())
    }

    /// Calculate buffer size in samples for a given channel count.
    pub fn buffer_samples(&self, channels: u16) -> usize {
        self.buffer_frames * channels as usize
    }

    /// Calculate minimum buffer size in samples for a given channel count.
    pub fn min_buffer_samples(&self, channels: u16) -> usize {
        self.min_buffer_frames * channels as usize
    }
}

// ============================================================================
// Default Functions (for serde)
// ============================================================================

fn default_buffer_frames() -> usize {
    88200 // 2 seconds at 44.1kHz
}

fn default_min_buffer_frames() -> usize {
    22050 // 0.5 seconds at 44.1kHz
}

fn default_prefetch_threshold() -> f32 {
    0.3 // 30%
}

fn default_decode_chunk_frames() -> usize {
    4096 // ~93ms at 44.1kHz
}

fn default_http_chunk_bytes() -> usize {
    256 * 1024 // 256 KB
}

fn default_http_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_decode_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_enable_adaptive_streaming() -> bool {
    true
}

// ============================================================================
// Streaming State
// ============================================================================

/// Current state of the streaming service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingState {
    /// Service is idle (not streaming).
    Idle,
    /// Initial buffering before playback can start.
    Buffering,
    /// Actively streaming and decoding.
    Streaming,
    /// Paused (buffer maintained but no active download/decode).
    Paused,
    /// Stalled due to network issues or buffer underrun.
    Stalled,
    /// Completed streaming (end of track reached).
    Completed,
    /// Error occurred, service stopped.
    Error,
}

impl StreamingState {
    /// Returns `true` if the service is in an active state.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Buffering | Self::Streaming | Self::Paused)
    }

    /// Returns `true` if the service is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Error)
    }
}

/// Statistics about streaming performance.
#[derive(Debug, Clone, Default)]
pub struct StreamingStats {
    /// Total number of frames buffered.
    pub total_frames_buffered: usize,
    /// Total number of frames consumed by playback.
    pub total_frames_consumed: usize,
    /// Current buffer level in frames.
    pub current_buffer_frames: usize,
    /// Total bytes downloaded from network.
    pub total_bytes_downloaded: u64,
    /// Total number of HTTP requests made.
    pub http_requests: u64,
    /// Number of buffer underruns encountered.
    pub underrun_count: u32,
    /// Average download speed in bytes per second.
    pub avg_download_speed: f64,
    /// Average decode time per chunk in milliseconds.
    pub avg_decode_time_ms: f64,
}

impl StreamingStats {
    /// Calculate buffer fill percentage (0.0 to 1.0).
    pub fn buffer_fill_percentage(&self, capacity: usize) -> f32 {
        if capacity == 0 {
            return 0.0;
        }
        (self.current_buffer_frames as f32 / capacity as f32).min(1.0)
    }

    /// Returns `true` if buffer is critically low.
    pub fn is_buffer_critical(&self, min_frames: usize) -> bool {
        self.current_buffer_frames < min_frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StreamingConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.buffer_frames, 88200);
        assert_eq!(config.prefetch_threshold, 0.3);
    }

    #[test]
    fn test_low_latency_config() {
        let config = StreamingConfig::low_latency();
        assert!(config.validate().is_ok());
        assert!(config.buffer_frames < StreamingConfig::default().buffer_frames);
        assert!(config.prefetch_threshold > StreamingConfig::default().prefetch_threshold);
    }

    #[test]
    fn test_high_quality_config() {
        let config = StreamingConfig::high_quality();
        assert!(config.validate().is_ok());
        assert!(config.buffer_frames > StreamingConfig::default().buffer_frames);
        assert!(config.prefetch_threshold < StreamingConfig::default().prefetch_threshold);
    }

    #[test]
    fn test_config_validation() {
        let mut config = StreamingConfig::default();

        // Valid config
        assert!(config.validate().is_ok());

        // Invalid: zero buffer
        config.buffer_frames = 0;
        assert!(config.validate().is_err());
        config.buffer_frames = 88200;

        // Invalid: min > buffer
        config.min_buffer_frames = 100000;
        assert!(config.validate().is_err());
        config.min_buffer_frames = 22050;

        // Invalid: threshold out of range
        config.prefetch_threshold = 1.5;
        assert!(config.validate().is_err());
        config.prefetch_threshold = 0.3;
    }

    #[test]
    fn test_buffer_samples_calculation() {
        let config = StreamingConfig::default();

        // Mono
        assert_eq!(config.buffer_samples(1), 88200);

        // Stereo
        assert_eq!(config.buffer_samples(2), 176400);

        // 5.1 surround
        assert_eq!(config.buffer_samples(6), 529200);
    }

    #[test]
    fn test_streaming_state() {
        assert!(StreamingState::Buffering.is_active());
        assert!(StreamingState::Streaming.is_active());
        assert!(!StreamingState::Completed.is_active());

        assert!(StreamingState::Completed.is_terminal());
        assert!(StreamingState::Error.is_terminal());
        assert!(!StreamingState::Streaming.is_terminal());
    }

    #[test]
    fn test_streaming_stats() {
        let mut stats = StreamingStats::default();
        stats.current_buffer_frames = 50000;

        assert!((stats.buffer_fill_percentage(100000) - 0.5).abs() < 0.01);
        assert!(!stats.is_buffer_critical(40000));
        assert!(stats.is_buffer_critical(60000));
    }
}
