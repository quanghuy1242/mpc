//! # Audio Streaming Service
//!
//! Producer logic for the audio streaming pipeline. Runs as an async task coordinated
//! by the host's `BackgroundExecutor`.
//!
//! ## Architecture
//!
//! The `StreamingService` operates as a pure async function - it does NOT spawn threads.
//! The host platform is responsible for running the service's main loop in an appropriate
//! execution context (background thread, Web Worker, etc.).
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │       StreamingService (Producer)       │
//! │                                         │
//! │  1. Download chunks (HttpClient)        │
//! │  2. Decode to PCM (AudioDecoder)        │
//! │  3. Write to RingBuffer                 │
//! └────────────┬────────────────────────────┘
//!              │ PCM Samples
//!              ▼
//! ┌─────────────────────────────────────────┐
//! │           RingBuffer (Shared)           │
//! └────────────┬────────────────────────────┘
//!              │ PCM Samples
//!              ▼
//! ┌─────────────────────────────────────────┐
//! │   PlaybackAdapter (Consumer Thread)     │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use core_playback::streaming::{StreamingService, StreamingRequest};
//! use core_playback::{AudioSource, RingBuffer, StreamingConfig};
//! use core_async::sync::CancellationToken;
//!
//! async fn start_streaming(
//!     http_client: Arc<dyn HttpClient>,
//!     decoder: Box<dyn AudioDecoder>,
//! ) {
//!     let ring_buffer = RingBuffer::new(176400); // 2s stereo at 44.1kHz
//!     let config = StreamingConfig::default();
//!     let cancel_token = CancellationToken::new();
//!     
//!     let request = StreamingRequest {
//!         source: AudioSource::RemoteStream {
//!             url: "https://example.com/song.mp3".into(),
//!             headers: Default::default(),
//!         },
//!         ring_buffer: ring_buffer.clone(),
//!         config,
//!     };
//!     
//!     let service = StreamingService::new(http_client, decoder);
//!     
//!     // Run the streaming loop (host decides execution context)
//!     service.run(request, cancel_token).await.ok();
//! }
//! ```

use crate::config::{StreamingConfig, StreamingState, StreamingStats};
use crate::error::{PlaybackError, Result};
use crate::ring_buffer::RingBuffer;
use crate::traits::{AudioDecoder, AudioSource};
use bridge_traits::http::HttpClient;
use core_async::sync::CancellationToken;
use core_async::time::sleep;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

// ============================================================================
// Request Types
// ============================================================================

/// Request to start streaming audio.
pub struct StreamingRequest {
    /// Audio source to stream from.
    pub source: AudioSource,
    /// Ring buffer to write decoded PCM samples into.
    pub ring_buffer: RingBuffer,
    /// Streaming configuration.
    pub config: StreamingConfig,
}

// ============================================================================
// StreamingService (Native)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub struct StreamingService {
    _http_client: Arc<dyn HttpClient>,
    decoder: parking_lot::Mutex<Box<dyn AudioDecoder>>,
    state: parking_lot::Mutex<StreamingState>,
    stats: parking_lot::Mutex<StreamingStats>,
}

#[cfg(not(target_arch = "wasm32"))]
impl StreamingService {
    /// Create a new streaming service.
    ///
    /// # Arguments
    ///
    /// * `http_client` - HTTP client for downloading audio data
    /// * `decoder` - Audio decoder implementation
    pub fn new(http_client: Arc<dyn HttpClient>, decoder: Box<dyn AudioDecoder>) -> Self {
        Self {
            _http_client: http_client,
            decoder: parking_lot::Mutex::new(decoder),
            state: parking_lot::Mutex::new(StreamingState::Idle),
            stats: parking_lot::Mutex::new(StreamingStats::default()),
        }
    }

    /// Get the current streaming state.
    pub fn state(&self) -> StreamingState {
        *self.state.lock()
    }

    /// Get streaming statistics.
    pub fn stats(&self) -> StreamingStats {
        self.stats.lock().clone()
    }

    /// Run the streaming service.
    ///
    /// This is the main entry point. It will:
    /// 1. Probe the audio format
    /// 2. Enter buffering state
    /// 3. Download and decode audio chunks
    /// 4. Write PCM samples to the ring buffer
    /// 5. Handle buffering and network conditions adaptively
    ///
    /// # Cancellation
    ///
    /// The service will stop when `cancel_token` is triggered.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Audio format cannot be probed
    /// - Network download fails
    /// - Decoding fails
    /// - Configuration is invalid
    #[instrument(skip(self, request, cancel_token))]
    pub async fn run(
        &self,
        request: StreamingRequest,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        // Validate configuration
        request.config.validate().map_err(|e| {
            PlaybackError::Internal(format!("Invalid streaming config: {}", e))
        })?;

        info!("Starting streaming service");
        *self.state.lock() = StreamingState::Buffering;
        *self.stats.lock() = StreamingStats::default();

        // Probe audio format
        let format = {
            let mut decoder = self.decoder.lock();
            let probe_result = decoder.probe().await?;
            debug!(
                "Probed audio format: codec={:?}, sample_rate={}, channels={}",
                probe_result.format.codec, probe_result.format.sample_rate, probe_result.format.channels
            );
            probe_result.format
        };

        // Calculate buffer requirements
        let channels = format.channels;
        let _sample_rate = format.sample_rate;
        let buffer_capacity_samples = request.config.buffer_samples(channels);

        // Verify ring buffer capacity
        if request.ring_buffer.capacity() < buffer_capacity_samples {
            warn!(
                "Ring buffer capacity ({}) is smaller than recommended ({})",
                request.ring_buffer.capacity(),
                buffer_capacity_samples
            );
        }

        // Main streaming loop
        let mut decode_times = Vec::new();
        let start_time = Instant::now();

        loop {
            // Check cancellation
            if cancel_token.is_cancelled() {
                info!("Streaming cancelled");
                *self.state.lock() = StreamingState::Idle;
                return Ok(());
            }

            // Check buffer level and decide action
            let buffer_level = request.ring_buffer.available();
            let buffer_capacity = request.ring_buffer.capacity();
            let fill_ratio = buffer_level as f32 / buffer_capacity as f32;

            // Update stats
            {
                let mut stats = self.stats.lock();
                stats.current_buffer_frames = buffer_level / channels as usize;
            }

            // State transitions
            let current_state = self.state();
            match current_state {
                StreamingState::Buffering => {
                    if buffer_level >= request.config.min_buffer_samples(channels) {
                        info!("Initial buffering complete, transitioning to streaming");
                        *self.state.lock() = StreamingState::Streaming;
                    }
                }
                StreamingState::Streaming => {
                    // Check for underrun
                    if fill_ratio < request.config.prefetch_threshold {
                        debug!(
                            "Buffer level low ({:.1}%), prefetching aggressively",
                            fill_ratio * 100.0
                        );
                    }
                }
                StreamingState::Completed | StreamingState::Error => {
                    break;
                }
                _ => {}
            }

            // Decode next chunk if buffer has space
            if request.ring_buffer.free_space() >= request.config.decode_chunk_frames * channels as usize {
                let decode_start = Instant::now();

                let chunk_result = {
                    let mut decoder = self.decoder.lock();
                    decoder.decode_frames(request.config.decode_chunk_frames).await
                };

                match chunk_result {
                    Ok(Some(chunk)) => {
                        let decode_elapsed = decode_start.elapsed();
                        decode_times.push(decode_elapsed.as_secs_f64() * 1000.0);

                        // Write to ring buffer
                        let written = request.ring_buffer.write(&chunk.samples);
                        debug!(
                            "Decoded {} frames, wrote {} samples to buffer (fill: {:.1}%)",
                            chunk.frames,
                            written,
                            fill_ratio * 100.0
                        );

                        // Update stats
                        {
                            let mut stats = self.stats.lock();
                            stats.total_frames_buffered += chunk.frames;
                            stats.avg_decode_time_ms = decode_times.iter().sum::<f64>() / decode_times.len() as f64;
                        }
                    }
                    Ok(None) => {
                        // End of stream
                        info!("End of stream reached");
                        *self.state.lock() = StreamingState::Completed;
                        break;
                    }
                    Err(e) => {
                        error!("Decoding error: {}", e);
                        *self.state.lock() = StreamingState::Error;
                        return Err(e);
                    }
                }
            } else {
                // Buffer is full, wait a bit
                sleep(Duration::from_millis(10)).await;
            }

            // Adaptive sleep based on buffer level
            if fill_ratio > 0.8 {
                // Buffer is well-filled, can afford to sleep longer
                sleep(Duration::from_millis(50)).await;
            } else if fill_ratio < 0.3 {
                // Buffer is low, decode aggressively (no sleep)
            } else {
                // Normal operation
                sleep(Duration::from_millis(20)).await;
            }
        }

        let elapsed = start_time.elapsed();
        info!(
            "Streaming completed in {:.2}s, {} frames buffered",
            elapsed.as_secs_f64(),
            self.stats.lock().total_frames_buffered
        );

        Ok(())
    }

    /// Pause streaming (stops decoding but maintains buffer).
    pub fn pause(&self) {
        let current = self.state();
        if current == StreamingState::Streaming {
            *self.state.lock() = StreamingState::Paused;
            info!("Streaming paused");
        }
    }

    /// Resume streaming from paused state.
    pub fn resume(&self) {
        let current = self.state();
        if current == StreamingState::Paused {
            *self.state.lock() = StreamingState::Streaming;
            info!("Streaming resumed");
        }
    }
}

// ============================================================================
// StreamingService (WASM)
// ============================================================================

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
pub struct StreamingService {
    _http_client: Rc<dyn HttpClient>,
    decoder: RefCell<Box<dyn AudioDecoder>>,
    state: RefCell<StreamingState>,
    stats: RefCell<StreamingStats>,
}

#[cfg(target_arch = "wasm32")]
impl StreamingService {
    /// Create a new streaming service.
    pub fn new(http_client: Rc<dyn HttpClient>, decoder: Box<dyn AudioDecoder>) -> Self {
        Self {
            _http_client: http_client,
            decoder: RefCell::new(decoder),
            state: RefCell::new(StreamingState::Idle),
            stats: RefCell::new(StreamingStats::default()),
        }
    }

    /// Get the current streaming state.
    pub fn state(&self) -> StreamingState {
        *self.state.borrow()
    }

    /// Get streaming statistics.
    pub fn stats(&self) -> StreamingStats {
        self.stats.borrow().clone()
    }

    /// Run the streaming service.
    #[instrument(skip(self, request, cancel_token))]
    pub async fn run(
        &self,
        request: StreamingRequest,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        // Validate configuration
        request.config.validate().map_err(|e| {
            PlaybackError::Internal(format!("Invalid streaming config: {}", e))
        })?;

        info!("Starting streaming service (WASM)");
        *self.state.borrow_mut() = StreamingState::Buffering;
        *self.stats.borrow_mut() = StreamingStats::default();

        // Probe audio format
        let format = {
            let mut decoder = self.decoder.borrow_mut();
            let probe_result = decoder.probe().await?;
            debug!(
                "Probed audio format: codec={:?}, sample_rate={}, channels={}",
                probe_result.format.codec, probe_result.format.sample_rate, probe_result.format.channels
            );
            probe_result.format
        };

        let channels = format.channels;
        let _sample_rate = format.sample_rate;
        let buffer_capacity_samples = request.config.buffer_samples(channels);

        if request.ring_buffer.capacity() < buffer_capacity_samples {
            warn!(
                "Ring buffer capacity ({}) is smaller than recommended ({})",
                request.ring_buffer.capacity(),
                buffer_capacity_samples
            );
        }

        // Main streaming loop
        let mut decode_times = Vec::new();
        let start_time = Instant::now();

        loop {
            if cancel_token.is_cancelled() {
                info!("Streaming cancelled");
                *self.state.borrow_mut() = StreamingState::Idle;
                return Ok(());
            }

            let buffer_level = request.ring_buffer.available();
            let buffer_capacity = request.ring_buffer.capacity();
            let fill_ratio = buffer_level as f32 / buffer_capacity as f32;

            // Update stats
            {
                let mut stats = self.stats.borrow_mut();
                stats.current_buffer_frames = buffer_level / channels as usize;
            }

            // State transitions
            let current_state = self.state();
            match current_state {
                StreamingState::Buffering => {
                    if buffer_level >= request.config.min_buffer_samples(channels) {
                        info!("Initial buffering complete, transitioning to streaming");
                        *self.state.borrow_mut() = StreamingState::Streaming;
                    }
                }
                StreamingState::Streaming => {
                    if fill_ratio < request.config.prefetch_threshold {
                        debug!(
                            "Buffer level low ({:.1}%), prefetching aggressively",
                            fill_ratio * 100.0
                        );
                    }
                }
                StreamingState::Completed | StreamingState::Error => {
                    break;
                }
                _ => {}
            }

            // Decode next chunk
            if request.ring_buffer.free_space() >= request.config.decode_chunk_frames * channels as usize {
                let decode_start = Instant::now();

                let chunk_result = {
                    let mut decoder = self.decoder.borrow_mut();
                    decoder.decode_frames(request.config.decode_chunk_frames).await
                };

                match chunk_result {
                    Ok(Some(chunk)) => {
                        let decode_elapsed = decode_start.elapsed();
                        decode_times.push(decode_elapsed.as_secs_f64() * 1000.0);

                        let written = request.ring_buffer.write(&chunk.samples);
                        debug!(
                            "Decoded {} frames, wrote {} samples to buffer (fill: {:.1}%)",
                            chunk.frames,
                            written,
                            fill_ratio * 100.0
                        );

                        {
                            let mut stats = self.stats.borrow_mut();
                            stats.total_frames_buffered += chunk.frames;
                            stats.avg_decode_time_ms = decode_times.iter().sum::<f64>() / decode_times.len() as f64;
                        }
                    }
                    Ok(None) => {
                        info!("End of stream reached");
                        *self.state.borrow_mut() = StreamingState::Completed;
                        break;
                    }
                    Err(e) => {
                        error!("Decoding error: {}", e);
                        *self.state.borrow_mut() = StreamingState::Error;
                        return Err(e);
                    }
                }
            } else {
                sleep(Duration::from_millis(10)).await;
            }

            // Adaptive sleep
            if fill_ratio > 0.8 {
                sleep(Duration::from_millis(50)).await;
            } else if fill_ratio < 0.3 {
                // No sleep
            } else {
                sleep(Duration::from_millis(20)).await;
            }
        }

        let elapsed = start_time.elapsed();
        info!(
            "Streaming completed in {:.2}s, {} frames buffered",
            elapsed.as_secs_f64(),
            self.stats.borrow().total_frames_buffered
        );

        Ok(())
    }

    /// Pause streaming.
    pub fn pause(&self) {
        let current = self.state();
        if current == StreamingState::Streaming {
            *self.state.borrow_mut() = StreamingState::Paused;
            info!("Streaming paused");
        }
    }

    /// Resume streaming.
    pub fn resume(&self) {
        let current = self.state();
        if current == StreamingState::Paused {
            *self.state.borrow_mut() = StreamingState::Streaming;
            info!("Streaming resumed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_request_creation() {
        let ring_buffer = RingBuffer::new(176400);
        let config = StreamingConfig::default();

        let request = StreamingRequest {
            source: AudioSource::LocalFile {
                path: "/path/to/file.mp3".into(),
            },
            ring_buffer,
            config,
        };

        // Verify configuration
        assert_eq!(request.config.buffer_frames, 88200);
    }
}
