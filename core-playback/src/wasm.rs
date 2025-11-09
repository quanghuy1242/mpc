//! WebAssembly bindings for core-playback
//!
//! This module provides JavaScript/TypeScript-friendly bindings for the playback
//! functionality using wasm-bindgen.

#[cfg(feature = "offline-cache")]
use crate::cache::{CacheConfig, EncryptionKey, EvictionPolicy, OfflineCacheManager};
use crate::config::{StreamingConfig, StreamingState, StreamingStats};
use crate::ring_buffer::RingBuffer;
use crate::streaming::{StreamingRequest, StreamingService};
use crate::traits::{AudioCodec, AudioFormat, AudioSource, ProbeResult};
use crate::PlaybackError;
use bridge_traits::http::{HttpClient, HttpMethod, HttpRequest};
use bridge_wasm::JsHttpClient;
use core_async::sync::CancellationToken;
use core_async::task;
use js_sys::{Float32Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "core-decoder")]
use crate::decoder::SymphoniaDecoder;
#[cfg(feature = "core-decoder")]
use crate::traits::AudioDecoder;
use bytes::Bytes;

#[cfg(feature = "offline-cache")]
use bridge_wasm::filesystem::WasmFileSystem;
#[cfg(feature = "offline-cache")]
use core_library::models::{CacheStatus, TrackId};
#[cfg(feature = "offline-cache")]
use core_library::repositories::SqliteTrackRepository;
#[cfg(feature = "offline-cache")]
use core_library::wasm::JsLibrary;
#[cfg(feature = "offline-cache")]
use core_runtime::wasm::JsEventBus;
#[cfg(feature = "offline-cache")]
use provider_google_drive::GoogleDriveConnector;
#[cfg(feature = "offline-cache")]
use serde_wasm_bindgen::to_value;
#[cfg(feature = "offline-cache")]
use bridge_traits::storage::{FileSystemAccess, StorageProvider};

// NOTE: Logging functions (init_panic_hook, enableConsoleLogging, initLogging) 
// are already exported by core-runtime. Don't duplicate them here.

// =============================================================================
// Audio Format Types - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible audio format information
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsAudioFormat {
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
}

#[wasm_bindgen]
impl JsAudioFormat {
    #[wasm_bindgen(js_name = sampleRate)]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    #[wasm_bindgen(js_name = bitsPerSample)]
    pub fn bits_per_sample(&self) -> u8 {
        self.bits_per_sample
    }
}

impl From<AudioFormat> for JsAudioFormat {
    fn from(format: AudioFormat) -> Self {
        Self {
            sample_rate: format.sample_rate,
            channels: format.channels as u8,
            bits_per_sample: format.bits_per_sample.unwrap_or(0) as u8,
        }
    }
}

/// JavaScript-accessible audio codec information
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsAudioCodec {
    codec_name: String,
}

#[wasm_bindgen]
impl JsAudioCodec {
    #[wasm_bindgen(js_name = codecName)]
    pub fn codec_name(&self) -> String {
        self.codec_name.clone()
    }
}

impl From<AudioCodec> for JsAudioCodec {
    fn from(codec: AudioCodec) -> Self {
        let codec_name = match codec {
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Aac => "aac",
            AudioCodec::Flac => "flac",
            AudioCodec::Vorbis => "vorbis",
            AudioCodec::Opus => "opus",
            AudioCodec::Wav => "wav",
            AudioCodec::Alac => "alac",
            AudioCodec::Unknown => "unknown",
            AudioCodec::Other(ref s) => s,
        }.to_string();
        
        Self { codec_name }
    }
}

/// JavaScript-accessible probe result
#[wasm_bindgen]
#[derive(Clone)]
pub struct JsProbeResult {
    format: JsAudioFormat,
    duration_ms: Option<u64>,
}

#[wasm_bindgen]
impl JsProbeResult {
    pub fn format(&self) -> JsAudioFormat {
        self.format.clone()
    }

    #[wasm_bindgen(js_name = durationMs)]
    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }
}

impl From<ProbeResult> for JsProbeResult {
    fn from(result: ProbeResult) -> Self {
        let duration_ms = result.duration.map(|d| d.as_millis() as u64);
        
        Self {
            format: result.format.into(),
            duration_ms,
        }
    }
}

// =============================================================================
// Cache Configuration - Exported to JavaScript
// =============================================================================

#[cfg(feature = "offline-cache")]
/// JavaScript-accessible cache configuration
#[wasm_bindgen]
pub struct JsCacheConfig {
    inner: CacheConfig,
}

#[cfg(feature = "offline-cache")]
#[wasm_bindgen]
impl JsCacheConfig {
    /// Create a new cache config with defaults
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CacheConfig::default(),
        }
    }

    /// Set maximum cache size in megabytes
    #[wasm_bindgen(js_name = setMaxSizeMB)]
    pub fn set_max_size_mb(&mut self, mb: u32) {
        self.inner.max_cache_size_bytes = (mb as u64) * 1024 * 1024;
    }

    /// Set eviction policy: "lru", "lfu", "fifo", or "largest_first"
    #[wasm_bindgen(js_name = setEvictionPolicy)]
    pub fn set_eviction_policy(&mut self, policy: &str) -> Result<(), JsValue> {
        self.inner.eviction_policy = match policy.to_lowercase().as_str() {
            "lru" => EvictionPolicy::LeastRecentlyUsed,
            "lfu" => EvictionPolicy::LeastFrequentlyUsed,
            "fifo" => EvictionPolicy::FirstInFirstOut,
            "largest_first" => EvictionPolicy::LargestFirst,
            _ => return Err(JsValue::from_str(&format!("Invalid eviction policy: {}", policy))),
        };
        Ok(())
    }

    /// Enable or disable encryption
    #[wasm_bindgen(js_name = setEncryption)]
    pub fn set_encryption(&mut self, enabled: bool) {
        self.inner.enable_encryption = enabled;
    }

    /// Set maximum concurrent downloads
    #[wasm_bindgen(js_name = setMaxConcurrentDownloads)]
    pub fn set_max_concurrent_downloads(&mut self, count: u8) {
        self.inner.max_concurrent_downloads = count as usize;
    }

    /// Set cache directory path
    #[wasm_bindgen(js_name = setCacheDirectory)]
    pub fn set_cache_directory(&mut self, path: String) {
        self.inner.cache_directory = path;
    }
}

#[cfg(feature = "offline-cache")]
impl Default for JsCacheConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Cache Status - Exported to JavaScript
// =============================================================================

#[cfg(feature = "offline-cache")]
/// JavaScript-accessible cache status for playback
/// NOTE: Named differently from core-library's JsCacheStatus to avoid conflicts
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsPlaybackCacheStatus {
    NotCached,
    Downloading,
    Cached,
    Failed,
    Stale,
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get the playback module version
#[wasm_bindgen]
pub fn playback_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the playback module name
#[wasm_bindgen]
pub fn playback_name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

/// Get list of supported audio formats
#[wasm_bindgen]
pub fn supported_formats() -> Vec<String> {
    let mut formats = vec!["wav".to_string(), "pcm".to_string()];
    
    #[cfg(feature = "decoder-mp3")]
    formats.push("mp3".to_string());
    
    #[cfg(feature = "decoder-flac")]
    formats.push("flac".to_string());
    
    #[cfg(feature = "decoder-vorbis")]
    formats.push("ogg".to_string());
    formats.push("vorbis".to_string());
    
    #[cfg(feature = "decoder-opus")]
    formats.push("opus".to_string());
    
    #[cfg(feature = "decoder-aac")]
    formats.push("aac".to_string());
    formats.push("m4a".to_string());
    
    #[cfg(feature = "decoder-alac")]
    formats.push("alac".to_string());
    
    formats
}

/// Check if a specific format is supported
#[wasm_bindgen]
pub fn is_format_supported(format: &str) -> bool {
    supported_formats().iter().any(|f| f.eq_ignore_ascii_case(format))
}

// =============================================================================
// Feature Detection
// =============================================================================

/// Check if the offline cache feature is available
#[wasm_bindgen]
pub fn has_offline_cache() -> bool {
    cfg!(feature = "offline-cache")
}

/// Check if HTTP streaming is available
#[wasm_bindgen]
pub fn has_http_streaming() -> bool {
    cfg!(feature = "http-streaming")
}

/// Check if the core decoder is available
#[wasm_bindgen]
pub fn has_decoder() -> bool {
    cfg!(feature = "core-decoder")
}

// =============================================================================
// Audio Decoder - Exported to JavaScript
// =============================================================================

#[cfg(feature = "core-decoder")]
/// JavaScript-accessible audio decoder
#[wasm_bindgen]
pub struct JsAudioDecoder {
    decoder: Option<SymphoniaDecoder>,
    probe_result: Option<JsProbeResult>,
}

#[cfg(feature = "core-decoder")]
#[wasm_bindgen]
impl JsAudioDecoder {
    /// Create a new decoder from raw audio file bytes
    ///
    /// # Arguments
    ///
    /// * `data` - Raw audio file bytes (MP3, AAC, M4A, FLAC, etc.)
    /// * `filename` - Optional filename for format hint (e.g., "song.m4a")
    #[wasm_bindgen(js_name = create)]
    pub async fn create(data: Vec<u8>, filename: Option<String>) -> Result<JsAudioDecoder, JsValue> {
        // Determine codec hint from filename
        let codec_hint = filename.as_ref().and_then(|name| {
            let ext = std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            
            match ext.to_lowercase().as_str() {
                "mp3" => Some(AudioCodec::Mp3),
                "aac" | "m4a" | "mp4" => Some(AudioCodec::Aac),
                "flac" => Some(AudioCodec::Flac),
                "ogg" => Some(AudioCodec::Vorbis),
                "opus" => Some(AudioCodec::Opus),
                "wav" => Some(AudioCodec::Wav),
                _ => None,
            }
        });

        // Create audio source from memory buffer
        let source = AudioSource::CachedChunk {
            data: Bytes::from(data),
            codec_hint,
        };

        // Create decoder
        let mut decoder = SymphoniaDecoder::new(source)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to create decoder: {}", e)))?;

        // Probe the audio format
        let probe = decoder
            .probe()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to probe audio: {}", e)))?;

        let probe_result = JsProbeResult::from(probe);

        Ok(JsAudioDecoder {
            decoder: Some(decoder),
            probe_result: Some(probe_result),
        })
    }

    /// Get audio format information
    ///
    /// Must call this after creating the decoder to get format details
    #[wasm_bindgen(js_name = getFormat)]
    pub fn get_format(&self) -> Result<JsProbeResult, JsValue> {
        self.probe_result
            .clone()
            .ok_or_else(|| JsValue::from_str("Decoder not initialized"))
    }

    /// Decode a chunk of audio frames
    ///
    /// Returns interleaved f32 PCM samples in range [-1.0, 1.0]
    /// Returns null when end of stream is reached
    ///
    /// # Arguments
    ///
    /// * `max_frames` - Maximum number of frames to decode (typically 4096-8192)
    ///
    /// # Returns
    ///
    /// Float32Array of interleaved samples, or null if EOF
    #[wasm_bindgen(js_name = decodeFrames)]
    pub async fn decode_frames(&mut self, max_frames: usize) -> Result<JsValue, JsValue> {
        let decoder = self
            .decoder
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Decoder not initialized"))?;

        match decoder.decode_frames(max_frames).await {
            Ok(Some(chunk)) => {
                // Convert Vec<f32> to Float32Array
                let array = js_sys::Float32Array::from(&chunk.samples[..]);
                Ok(array.into())
            }
            Ok(None) => {
                // End of stream
                Ok(JsValue::NULL)
            }
            Err(e) => Err(JsValue::from_str(&format!("Decode error: {}", e))),
        }
    }

    /// Decode multiple packets in batch to reduce WASM/JS boundary crossings
    ///
    /// This is significantly faster than calling decodeFrames repeatedly because
    /// it decodes multiple codec packets in Rust before crossing the WASM boundary.
    ///
    /// # Arguments
    ///
    /// * `target_frames` - Target number of frames to decode (e.g., 32768 for ~0.75s @ 44.1kHz)
    /// * `max_packets` - Maximum number of codec packets to decode (prevents infinite loops)
    ///
    /// # Returns
    ///
    /// Float32Array of interleaved samples, or null if EOF
    ///
    /// # Example
    ///
    /// ```javascript
    /// // Decode ~1 second of audio in one call (reduces boundary crossings by ~40x for AAC)
    /// const samples = await decoder.decodeBatch(44100, 50);
    /// ```
    #[wasm_bindgen(js_name = decodeBatch)]
    pub async fn decode_batch(&mut self, target_frames: usize, max_packets: usize) -> Result<JsValue, JsValue> {
        let decoder = self
            .decoder
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Decoder not initialized"))?;

        let mut all_samples = Vec::new();
        let mut decoded_frames = 0;
        let mut packets = 0;

        while decoded_frames < target_frames && packets < max_packets {
            match decoder.decode_frames(8192).await {
                Ok(Some(chunk)) => {
                    let frame_count = chunk.frames;
                    all_samples.extend_from_slice(&chunk.samples);
                    decoded_frames += frame_count;
                    packets += 1;
                }
                Ok(None) => {
                    // End of stream
                    break;
                }
                Err(e) => {
                    // If we've decoded some samples, return them; otherwise propagate error
                    if !all_samples.is_empty() {
                        break;
                    }
                    return Err(JsValue::from_str(&format!("Batch decode error: {}", e)));
                }
            }
        }

        if all_samples.is_empty() {
            Ok(JsValue::NULL)
        } else {
            let array = js_sys::Float32Array::from(&all_samples[..]);
            Ok(array.into())
        }
    }

    /// Seek to a specific position in seconds
    ///
    /// # Arguments
    ///
    /// * `position_seconds` - Target position in seconds
    #[wasm_bindgen(js_name = seek)]
    pub async fn seek(&mut self, position_seconds: f64) -> Result<(), JsValue> {
        let decoder = self
            .decoder
            .as_mut()
            .ok_or_else(|| JsValue::from_str("Decoder not initialized"))?;

        let duration = Duration::from_secs_f64(position_seconds);
        decoder
            .seek(duration)
            .await
            .map_err(|e| JsValue::from_str(&format!("Seek error: {}", e)))?;

        Ok(())
    }

    /// Reset decoder to beginning
    #[wasm_bindgen(js_name = reset)]
    pub async fn reset(&mut self) -> Result<(), JsValue> {
        self.seek(0.0).await
    }
}

// =============================================================================
// Streaming Configuration - Exported to JavaScript
// =============================================================================

#[wasm_bindgen]
#[derive(Clone)]
pub struct JsStreamingConfig {
    inner: StreamingConfig,
}

#[wasm_bindgen]
impl JsStreamingConfig {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: StreamingConfig::default(),
        }
    }

    #[wasm_bindgen(js_name = setBufferFrames)]
    pub fn set_buffer_frames(&mut self, frames: usize) {
        self.inner.buffer_frames = frames;
    }

    #[wasm_bindgen(js_name = setMinBufferFrames)]
    pub fn set_min_buffer_frames(&mut self, frames: usize) {
        self.inner.min_buffer_frames = frames;
    }

    #[wasm_bindgen(js_name = setPrefetchThreshold)]
    pub fn set_prefetch_threshold(&mut self, threshold: f32) {
        self.inner.prefetch_threshold = threshold.clamp(0.0, 1.0);
    }

    #[wasm_bindgen(js_name = setDecodeChunkFrames)]
    pub fn set_decode_chunk_frames(&mut self, frames: usize) {
        self.inner.decode_chunk_frames = frames.max(1);
    }

    #[wasm_bindgen(js_name = setHttpChunkBytes)]
    pub fn set_http_chunk_bytes(&mut self, bytes: usize) {
        self.inner.http_chunk_bytes = bytes.max(1);
    }

    #[wasm_bindgen(js_name = setHttpTimeoutMs)]
    pub fn set_http_timeout_ms(&mut self, timeout_ms: u32) {
        self.inner.http_timeout = Duration::from_millis(timeout_ms as u64);
    }

    #[wasm_bindgen(js_name = setDecodeTimeoutMs)]
    pub fn set_decode_timeout_ms(&mut self, timeout_ms: u32) {
        self.inner.decode_timeout = Duration::from_millis(timeout_ms as u64);
    }

    #[wasm_bindgen(js_name = setAdaptiveStreaming)]
    pub fn set_adaptive_streaming(&mut self, enabled: bool) {
        self.inner.enable_adaptive_streaming = enabled;
    }

    #[wasm_bindgen(js_name = validate)]
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }
}

impl JsStreamingConfig {
    fn clone_inner(&self) -> StreamingConfig {
        self.inner.clone()
    }
}

#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub enum JsStreamingState {
    Idle,
    Buffering,
    Streaming,
    Paused,
    Stalled,
    Completed,
    Error,
}

impl From<StreamingState> for JsStreamingState {
    fn from(value: StreamingState) -> Self {
        match value {
            StreamingState::Idle => JsStreamingState::Idle,
            StreamingState::Buffering => JsStreamingState::Buffering,
            StreamingState::Streaming => JsStreamingState::Streaming,
            StreamingState::Paused => JsStreamingState::Paused,
            StreamingState::Stalled => JsStreamingState::Stalled,
            StreamingState::Completed => JsStreamingState::Completed,
            StreamingState::Error => JsStreamingState::Error,
        }
    }
}

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsStreamingStats {
    inner: StreamingStats,
}

#[wasm_bindgen]
impl JsStreamingStats {
    #[wasm_bindgen(js_name = totalFramesBuffered)]
    pub fn total_frames_buffered(&self) -> usize {
        self.inner.total_frames_buffered
    }

    #[wasm_bindgen(js_name = totalFramesConsumed)]
    pub fn total_frames_consumed(&self) -> usize {
        self.inner.total_frames_consumed
    }

    #[wasm_bindgen(js_name = currentBufferFrames)]
    pub fn current_buffer_frames(&self) -> usize {
        self.inner.current_buffer_frames
    }

    #[wasm_bindgen(js_name = totalBytesDownloaded)]
    pub fn total_bytes_downloaded(&self) -> u64 {
        self.inner.total_bytes_downloaded
    }

    #[wasm_bindgen(js_name = httpRequests)]
    pub fn http_requests(&self) -> u64 {
        self.inner.http_requests
    }

    #[wasm_bindgen(js_name = underrunCount)]
    pub fn underrun_count(&self) -> u32 {
        self.inner.underrun_count
    }

    #[wasm_bindgen(js_name = avgDownloadSpeed)]
    pub fn avg_download_speed(&self) -> f64 {
        self.inner.avg_download_speed
    }

    #[wasm_bindgen(js_name = avgDecodeTimeMs)]
    pub fn avg_decode_time_ms(&self) -> f64 {
        self.inner.avg_decode_time_ms
    }
}

impl From<StreamingStats> for JsStreamingStats {
    fn from(stats: StreamingStats) -> Self {
        Self { inner: stats }
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct JsRingBuffer {
    inner: RingBuffer,
    channels: u16,
}

#[wasm_bindgen]
impl JsRingBuffer {
    #[wasm_bindgen(constructor)]
    pub fn new(capacity_frames: usize, channels: u16) -> Result<JsRingBuffer, JsValue> {
        if channels == 0 {
            return Err(JsValue::from_str("Channel count must be greater than zero"));
        }

        let samples = capacity_frames
            .checked_mul(channels as usize)
            .ok_or_else(|| JsValue::from_str("Ring buffer capacity overflow"))?;

        Ok(Self {
            inner: RingBuffer::new(samples),
            channels,
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    #[wasm_bindgen(js_name = capacityFrames)]
    pub fn capacity_frames(&self) -> usize {
        self.inner.capacity() / self.channels as usize
    }

    #[wasm_bindgen(js_name = availableFrames)]
    pub fn available_frames(&self) -> usize {
        self.inner.available() / self.channels as usize
    }

    #[wasm_bindgen(js_name = freeFrames)]
    pub fn free_frames(&self) -> usize {
        self.inner.free_space() / self.channels as usize
    }

    #[wasm_bindgen(js_name = fillRatio)]
    pub fn fill_ratio(&self) -> f64 {
        self.inner.fill_level() as f64
    }

    pub fn clear(&self) {
        self.inner.clear();
    }

    #[wasm_bindgen(js_name = readFrames)]
    pub fn read_frames(&self, max_frames: usize) -> Option<Float32Array> {
        if max_frames == 0 {
            return None;
        }

        let channels = self.channels as usize;
        let mut buffer = vec![0.0f32; max_frames * channels];
        let read_samples = self.inner.read(&mut buffer);

        if read_samples == 0 {
            return None;
        }

        buffer.truncate(read_samples);
        Some(Float32Array::from(buffer.as_slice()))
    }

    #[wasm_bindgen(js_name = writeSamples)]
    pub fn write_samples(&self, samples: &Float32Array) -> usize {
        let mut data = vec![0.0f32; samples.length() as usize];
        samples.copy_to(&mut data[..]);
        self.inner.write(&data)
    }
}

impl JsRingBuffer {
    fn clone_inner(&self) -> RingBuffer {
        self.inner.clone()
    }

    fn from_inner(inner: RingBuffer, channels: u16) -> Self {
        Self { inner, channels }
    }
}

#[derive(Clone)]
enum AudioSourceDescriptor {
    CachedChunk {
        data: Vec<u8>,
        codec_hint: Option<AudioCodec>,
    },
    LocalFile {
        path: PathBuf,
    },
    Remote {
        url: String,
        headers: HashMap<String, String>,
    },
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct JsAudioSource {
    descriptor: AudioSourceDescriptor,
}

#[wasm_bindgen]
impl JsAudioSource {
    #[wasm_bindgen(js_name = fromCachedChunk)]
    pub fn from_cached_chunk(data: Vec<u8>, codec_hint: Option<String>) -> Result<JsAudioSource, JsValue> {
        Ok(Self {
            descriptor: AudioSourceDescriptor::CachedChunk {
                data,
                codec_hint: codec_hint.as_deref().map(codec_from_hint_str),
            },
        })
    }

    #[wasm_bindgen(js_name = fromLocalPath)]
    pub fn from_local_path(path: String) -> JsAudioSource {
        Self {
            descriptor: AudioSourceDescriptor::LocalFile {
                path: PathBuf::from(path),
            },
        }
    }

    #[wasm_bindgen(js_name = fromRemote)]
    pub fn from_remote(url: String, headers: Option<JsValue>) -> Result<JsAudioSource, JsValue> {
        Ok(Self {
            descriptor: AudioSourceDescriptor::Remote {
                url,
                headers: parse_headers(headers)?,
            },
        })
    }
}

impl JsAudioSource {
    async fn resolve(&self, http_client: &JsHttpClient) -> Result<AudioSource, JsValue> {
        match &self.descriptor {
            AudioSourceDescriptor::CachedChunk { data, codec_hint } => Ok(AudioSource::CachedChunk {
                data: Bytes::from(data.clone()),
                codec_hint: codec_hint.clone(),
            }),
            AudioSourceDescriptor::LocalFile { path } => Ok(AudioSource::LocalFile {
                path: path.clone(),
            }),
            AudioSourceDescriptor::Remote { url, headers } => {
                let bytes = fetch_remote_bytes(http_client, url, headers).await?;
                Ok(AudioSource::CachedChunk {
                    data: bytes,
                    codec_hint: None,
                })
            }
        }
    }
}

#[cfg(feature = "core-decoder")]
#[wasm_bindgen]
pub struct JsStreamingSession {
    service: Rc<StreamingService>,
    ring_buffer: RingBuffer,
    config: StreamingConfig,
    cancel_token: CancellationToken,
    join_handle: Rc<RefCell<Option<task::JoinHandle<()>>>>,
    completion: Rc<RefCell<Option<Result<(), PlaybackError>>>>,
    source: AudioSource,
    probe: JsProbeResult,
    channels: u16,
}

#[cfg(feature = "core-decoder")]
#[wasm_bindgen]
impl JsStreamingSession {
    #[wasm_bindgen(js_name = create)]
    pub async fn create(
        source: JsAudioSource,
        ring_buffer: &JsRingBuffer,
        config: &JsStreamingConfig,
        http_client: &JsHttpClient,
    ) -> Result<JsStreamingSession, JsValue> {
        let audio_source = source.resolve(http_client).await?;

        let mut decoder = SymphoniaDecoder::new(audio_source.clone())
            .await
            .map_err(to_js_error)?;
        let probe = decoder.probe().await.map_err(to_js_error)?;
        let probe_js = JsProbeResult::from(probe.clone());

        let http_client_rc: Rc<dyn bridge_traits::http::HttpClient> =
            Rc::new(http_client.clone());

        let service = Rc::new(StreamingService::new(http_client_rc, Box::new(decoder)));

        Ok(Self {
            service,
            ring_buffer: ring_buffer.clone_inner(),
            config: config.clone_inner(),
            cancel_token: CancellationToken::new(),
            join_handle: Rc::new(RefCell::new(None)),
            completion: Rc::new(RefCell::new(None)),
            source: audio_source,
            probe: probe_js,
            channels: probe.format.channels,
        })
    }

    pub fn start(&self) -> Result<(), JsValue> {
        let mut slot = self.join_handle.borrow_mut();
        if slot.is_some() {
            return Err(JsValue::from_str("Streaming already started"));
        }

        let request = StreamingRequest {
            source: self.source.clone(),
            ring_buffer: self.ring_buffer.clone(),
            config: self.config.clone(),
        };

        let service = self.service.clone();
        let token = self.cancel_token.clone();
        let completion = self.completion.clone();

        let handle = task::spawn(async move {
            let result = service.run(request, token).await;
            let mut guard = completion.borrow_mut();
            *guard = Some(result);
        });

        *slot = Some(handle);
        Ok(())
    }

    pub fn pause(&self) {
        self.service.pause();
    }

    pub fn resume(&self) {
        self.service.resume();
    }

    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    #[wasm_bindgen(js_name = awaitCompletion)]
    pub fn await_completion(&self) -> js_sys::Promise {
        let join_handle = self.join_handle.clone();
        let completion = self.completion.clone();

        future_to_promise(async move {
            if let Some(handle) = {
                let mut guard = join_handle.borrow_mut();
                guard.take()
            } {
                if let Err(err) = handle.await {
                    return Err(to_js_error(err));
                }
            }

            if let Some(result) = {
                let mut guard = completion.borrow_mut();
                guard.take()
            } {
                match result {
                    Ok(()) => Ok(JsValue::UNDEFINED),
                    Err(err) => Err(playback_error_to_js(err)),
                }
            } else {
                Ok(JsValue::UNDEFINED)
            }
        })
    }

    pub fn state(&self) -> JsStreamingState {
        JsStreamingState::from(self.service.state())
    }

    pub fn stats(&self) -> JsStreamingStats {
        JsStreamingStats::from(self.service.stats())
    }

    #[wasm_bindgen(js_name = ringBuffer)]
    pub fn ring_buffer(&self) -> JsRingBuffer {
        JsRingBuffer::from_inner(self.ring_buffer.clone(), self.channels)
    }

    #[wasm_bindgen(js_name = format)]
    pub fn format(&self) -> JsProbeResult {
        self.probe.clone()
    }
}

// =============================================================================
// Offline Cache Manager - Exported to JavaScript (WASM)
// =============================================================================

#[cfg(feature = "offline-cache")]
enum StorageProviderDescriptor {
    GoogleDrive { access_token: String },
}

#[cfg(feature = "offline-cache")]
#[wasm_bindgen]
pub struct JsStorageProviderConfig {
    descriptor: StorageProviderDescriptor,
}

#[cfg(feature = "offline-cache")]
#[wasm_bindgen]
impl JsStorageProviderConfig {
    #[wasm_bindgen(js_name = googleDrive)]
    pub fn google_drive(access_token: String) -> JsStorageProviderConfig {
        Self {
            descriptor: StorageProviderDescriptor::GoogleDrive { access_token },
        }
    }
}

#[cfg(feature = "offline-cache")]
impl JsStorageProviderConfig {
    fn build(&self, http_client: Arc<dyn bridge_traits::http::HttpClient>) -> Arc<dyn StorageProvider> {
        match &self.descriptor {
            StorageProviderDescriptor::GoogleDrive { access_token } => {
                Arc::new(GoogleDriveConnector::new(http_client, access_token.clone()))
                    as Arc<dyn StorageProvider>
            }
        }
    }
}

#[cfg(feature = "offline-cache")]
#[wasm_bindgen]
pub struct JsOfflineCacheManager {
    manager: Arc<OfflineCacheManager>,
}

#[cfg(feature = "offline-cache")]
#[wasm_bindgen]
impl JsOfflineCacheManager {
    #[wasm_bindgen(js_name = create)]
    pub async fn create(
        library: &JsLibrary,
        config: &JsCacheConfig,
        namespace: String,
        http_client: &JsHttpClient,
        storage: &JsStorageProviderConfig,
        event_bus: Option<JsEventBus>,
        encryption_key: Option<Vec<u8>>,
    ) -> Result<JsOfflineCacheManager, JsValue> {
        let adapter = library.adapter_handle();

        let track_repo = SqliteTrackRepository::new(adapter.clone());
        let track_repo_arc: Arc<dyn core_library::repositories::TrackRepository> =
            Arc::new(track_repo);

        let filesystem: Arc<dyn FileSystemAccess> = Arc::new(
            WasmFileSystem::new(&namespace)
                .await
                .map_err(|e| to_js_error(format!("Filesystem init failed: {e}")))?,
        );

        let http_arc: Arc<dyn bridge_traits::http::HttpClient> =
            Arc::new(http_client.clone());

        let storage_arc = storage.build(http_arc.clone());

        let mut manager = OfflineCacheManager::new(
            config.clone_inner(),
            adapter,
            track_repo_arc,
            filesystem,
            http_arc,
            storage_arc,
        );

        if let Some(bus) = event_bus {
            manager = manager.with_event_bus(bus.inner().clone());
        }

        if let Some(bytes) = encryption_key {
            let key = EncryptionKey::from_bytes(bytes).map_err(to_js_error)?;
            manager = manager.with_encryption(key);
        }

        Ok(Self {
            manager: Arc::new(manager),
        })
    }

    pub fn initialize(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            manager.initialize().await.map_err(playback_error_to_js)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = downloadTrack)]
    pub fn download_track(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            manager.download_track(id).await.map_err(playback_error_to_js)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = isCached)]
    pub fn is_cached(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            let cached = manager.is_cached(&id).await.map_err(playback_error_to_js)?;
            Ok(JsValue::from_bool(cached))
        })
    }

    #[wasm_bindgen(js_name = cacheStatus)]
    pub fn cache_status(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            let status = manager
                .get_cache_status(&id)
                .await
                .map_err(playback_error_to_js)?;
            Ok(JsValue::from(JsPlaybackCacheStatus::from(status)))
        })
    }

    #[wasm_bindgen(js_name = cacheStats)]
    pub fn cache_stats(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let stats = manager
                .get_cache_stats()
                .await
                .map_err(playback_error_to_js)?;
            to_value(&stats).map_err(to_js_error)
        })
    }

    #[wasm_bindgen(js_name = cacheSize)]
    pub fn cache_size(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let size = manager
                .get_cache_size()
                .await
                .map_err(playback_error_to_js)?;
            Ok(JsValue::from_f64(size as f64))
        })
    }

    #[wasm_bindgen(js_name = activeDownloads)]
    pub fn active_downloads(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let downloads = manager.get_active_downloads().await;
            to_value(&downloads).map_err(to_js_error)
        })
    }

    #[wasm_bindgen(js_name = downloadProgress)]
    pub fn download_progress(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            match manager.get_download_progress(&id).await {
                Some(progress) => to_value(&progress).map_err(to_js_error),
                None => Ok(JsValue::NULL),
            }
        })
    }

    #[wasm_bindgen(js_name = readTrack)]
    pub fn read_track(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            let bytes = manager
                .read_cached_track(&id)
                .await
                .map_err(playback_error_to_js)?;
            Ok(Uint8Array::from(bytes.as_ref()).into())
        })
    }

    #[wasm_bindgen(js_name = listCachedTracks)]
    pub fn list_cached_tracks(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let tracks = manager
                .list_cached_tracks()
                .await
                .map_err(playback_error_to_js)?;
            to_value(&tracks).map_err(to_js_error)
        })
    }

    #[wasm_bindgen(js_name = evictBytes)]
    pub fn evict_bytes(&self, bytes: f64) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let removed = manager
                .evict_tracks(bytes.max(0.0) as u64)
                .await
                .map_err(playback_error_to_js)?;
            Ok(JsValue::from_f64(removed as f64))
        })
    }

    #[wasm_bindgen(js_name = clearCache)]
    pub fn clear_cache(&self) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let cleared = manager
                .clear_cache()
                .await
                .map_err(playback_error_to_js)?;
            Ok(JsValue::from_f64(cleared as f64))
        })
    }

    #[wasm_bindgen(js_name = deleteCachedTrack)]
    pub fn delete_cached_track(&self, track_id: String) -> js_sys::Promise {
        let manager = self.manager.clone();
        future_to_promise(async move {
            let id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            manager
                .delete_cached_track(&id)
                .await
                .map_err(playback_error_to_js)?;
            Ok(JsValue::UNDEFINED)
        })
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}

fn playback_error_to_js(err: PlaybackError) -> JsValue {
    JsValue::from_str(&err.to_string())
}

fn codec_from_hint_str(value: &str) -> AudioCodec {
    match value.to_lowercase().as_str() {
        "mp3" => AudioCodec::Mp3,
        "aac" | "m4a" | "mp4" => AudioCodec::Aac,
        "flac" => AudioCodec::Flac,
        "ogg" | "vorbis" => AudioCodec::Vorbis,
        "opus" => AudioCodec::Opus,
        "wav" => AudioCodec::Wav,
        "alac" => AudioCodec::Alac,
        other => AudioCodec::Other(other.to_string()),
    }
}

fn parse_headers(headers: Option<JsValue>) -> Result<HashMap<String, String>, JsValue> {
    if let Some(raw) = headers {
        if raw.is_null() || raw.is_undefined() {
            return Ok(HashMap::new());
        }

        let object = Object::from(raw);
        let keys = Object::keys(&object);
        let mut map = HashMap::new();
        for index in 0..keys.length() {
            let key = keys
                .get(index)
                .as_string()
                .ok_or_else(|| JsValue::from_str("Header keys must be strings"))?;
            let value = Reflect::get(&object, &JsValue::from_str(&key))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("Header values must be strings"))?;
            map.insert(key, value);
        }
        Ok(map)
    } else {
        Ok(HashMap::new())
    }
}

async fn fetch_remote_bytes(
    http_client: &JsHttpClient,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<Bytes, JsValue> {
    let mut request = HttpRequest::new(HttpMethod::Get, url.to_string());
    request.headers = headers.clone();

    let response = http_client.execute(request).await.map_err(to_js_error)?;

    if !(200..300).contains(&response.status) {
        return Err(JsValue::from_str(&format!(
            "HTTP {} when fetching audio from {}",
            response.status, url
        )));
    }

    Ok(response.body)
}

#[cfg(feature = "offline-cache")]
impl From<CacheStatus> for JsPlaybackCacheStatus {
    fn from(status: CacheStatus) -> Self {
        match status {
            CacheStatus::NotCached => JsPlaybackCacheStatus::NotCached,
            CacheStatus::Downloading => JsPlaybackCacheStatus::Downloading,
            CacheStatus::Cached => JsPlaybackCacheStatus::Cached,
            CacheStatus::Failed => JsPlaybackCacheStatus::Failed,
            CacheStatus::Stale => JsPlaybackCacheStatus::Stale,
        }
    }
}

#[cfg(feature = "offline-cache")]
impl JsCacheConfig {
    pub(crate) fn clone_inner(&self) -> CacheConfig {
        self.inner.clone()
    }
}

