//! WebAssembly bindings for core-playback
//!
//! This module provides JavaScript/TypeScript-friendly bindings for the playback
//! functionality using wasm-bindgen.

use crate::cache::{CacheConfig, EvictionPolicy, OfflineCacheManager};
use crate::traits::{AudioCodec, AudioFormat, ProbeResult};
#[cfg(feature = "core-decoder")]
use crate::decoder::SymphoniaDecoder;
use core_library::models::{CacheStatus, TrackId};
use wasm_bindgen::prelude::*;

// Note: init_panic_hook is already exported by core-library, no need to duplicate it

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

impl From<CacheStatus> for JsCacheStatus {
    fn from(status: CacheStatus) -> Self {
        match status {
            CacheStatus::NotCached => JsCacheStatus::NotCached,
            CacheStatus::Downloading => JsCacheStatus::Downloading,
            CacheStatus::Cached => JsCacheStatus::Cached,
            CacheStatus::Failed => JsCacheStatus::Failed,
            CacheStatus::Stale => JsCacheStatus::Stale,
        }
    }
}

impl From<JsCacheStatus> for CacheStatus {
    fn from(status: JsCacheStatus) -> Self {
        match status {
            JsCacheStatus::NotCached => CacheStatus::NotCached,
            JsCacheStatus::Downloading => CacheStatus::Downloading,
            JsCacheStatus::Cached => CacheStatus::Cached,
            JsCacheStatus::Failed => CacheStatus::Failed,
            JsCacheStatus::Stale => CacheStatus::Stale,
        }
    }
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
// TODO: Advanced Bindings
// =============================================================================
// The following would require significant work to expose properly:
//
// 1. OfflineCacheManager - needs async support and dependency injection
// 2. SymphoniaDecoder - needs AudioSource trait implementation for WASM
// 3. StreamingService - needs ring buffer and platform audio adapter
// 4. RingBuffer - needs proper JS typed array integration
//
// These would be exposed in a future iteration once the platform bridges
// (bridge-wasm) are more complete.

