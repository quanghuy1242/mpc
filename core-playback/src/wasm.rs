//! WebAssembly bindings for core-playback
//!
//! This module provides JavaScript/TypeScript-friendly bindings for the playback
//! functionality using wasm-bindgen.

use crate::cache::{CacheConfig, EvictionPolicy};
use crate::traits::{AudioCodec, AudioFormat, AudioSource, ProbeResult};
use wasm_bindgen::prelude::*;
use std::time::Duration;

#[cfg(feature = "core-decoder")]
use crate::decoder::SymphoniaDecoder;
#[cfg(feature = "core-decoder")]
use crate::traits::AudioDecoder;
#[cfg(feature = "core-decoder")]
use bytes::Bytes;

// Note: init_panic_hook is already exported by core-library, no need to duplicate it

/// Enable Rust logging to browser console
/// Call this once at startup to see tracing logs in DevTools
#[wasm_bindgen(js_name = enableConsoleLogging)]
pub fn enable_console_logging() {
    use core_runtime::logging::{init_logging, LoggingConfig, LogFormat};
    use bridge_traits::time::LogLevel;
    
    let config = LoggingConfig::default()
        .with_format(LogFormat::Compact)
        .with_level(LogLevel::Debug);
    
    let _ = init_logging(config);
    web_sys::console::log_1(&"✅ Rust console logging enabled (tracing-wasm)".into());
}

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

/// JavaScript-accessible cache configuration
#[wasm_bindgen]
pub struct JsCacheConfig {
    inner: CacheConfig,
}

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

impl Default for JsCacheConfig {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Cache Status - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible cache status
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsCacheStatus {
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
// TODO: Advanced Bindings
// =============================================================================
// The following would require significant work to expose properly:
//
// 1. OfflineCacheManager - needs async support and dependency injection
// 3. StreamingService - needs ring buffer and platform audio adapter
// 4. RingBuffer - needs proper JS typed array integration
//
// These would be exposed in a future iteration once the platform bridges
// (bridge-wasm) are more complete.

