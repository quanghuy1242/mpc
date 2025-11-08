//! Comprehensive tests for core-playback traits
//!
//! This test suite verifies:
//! - Mock implementations of AudioDecoder and PlaybackAdapter
//! - Trait API compatibility
//! - Error handling
//! - Cross-platform compatibility

use core_playback::{
    AudioCodec, AudioDecoder, AudioFormat, AudioFrameChunk, AudioSource, PlaybackAdapter,
    PlaybackError, ProbeResult, Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Mock AudioDecoder Implementation
// ============================================================================

struct MockAudioDecoder {
    format: AudioFormat,
    total_frames: usize,
    frames_decoded: usize,
    current_position: Duration,
    sample_rate: u32,
    channels: u16,
    should_fail: bool,
    seek_supported: bool,
}

impl MockAudioDecoder {
    fn new(codec: AudioCodec, sample_rate: u32, channels: u16, duration_secs: u64) -> Self {
        let total_frames = (sample_rate as u64 * duration_secs) as usize;
        Self {
            format: AudioFormat::new(codec, sample_rate, channels, Some(16), None),
            total_frames,
            frames_decoded: 0,
            current_position: Duration::from_secs(0),
            sample_rate,
            channels,
            should_fail: false,
            seek_supported: true,
        }
    }

    fn with_seek_support(mut self, supported: bool) -> Self {
        self.seek_supported = supported;
        self
    }

    fn with_failure(mut self, should_fail: bool) -> Self {
        self.should_fail = should_fail;
        self
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl AudioDecoder for MockAudioDecoder {
    async fn probe(&mut self) -> Result<ProbeResult> {
        if self.should_fail {
            return Err(PlaybackError::DecodingError("Mock probe failure".into()));
        }

        let duration_secs = self.total_frames as u64 / self.sample_rate as u64;
        let mut tags = HashMap::new();
        tags.insert("title".to_string(), "Mock Track".to_string());
        tags.insert("artist".to_string(), "Mock Artist".to_string());

        Ok(ProbeResult::new(self.format.clone())
            .with_duration(Some(Duration::from_secs(duration_secs)))
            .with_tags(tags))
    }

    async fn decode_frames(&mut self, max_frames: usize) -> Result<Option<AudioFrameChunk>> {
        if self.should_fail {
            return Err(PlaybackError::DecodingError(
                "Mock decoding failure".into(),
            ));
        }

        if self.frames_decoded >= self.total_frames {
            return Ok(None); // End of stream
        }

        let frames_remaining = self.total_frames - self.frames_decoded;
        let frames_to_decode = max_frames.min(frames_remaining);

        // Generate mock PCM samples (silence)
        let sample_count = frames_to_decode * self.channels as usize;
        let samples = vec![0.0f32; sample_count];

        let chunk = AudioFrameChunk::new(samples, frames_to_decode, self.current_position);

        // Update state
        self.frames_decoded += frames_to_decode;
        let frame_duration =
            Duration::from_secs_f64(frames_to_decode as f64 / self.sample_rate as f64);
        self.current_position += frame_duration;

        Ok(Some(chunk))
    }

    async fn seek(&mut self, position: Duration) -> Result<()> {
        if !self.seek_supported {
            return Err(PlaybackError::SeekNotSupported);
        }

        let position_secs = position.as_secs_f64();
        let max_position_secs = self.total_frames as f64 / self.sample_rate as f64;

        if position_secs > max_position_secs {
            return Err(PlaybackError::SeekOutOfBounds(position));
        }

        let target_frame = (position_secs * self.sample_rate as f64) as usize;
        self.frames_decoded = target_frame;
        self.current_position = position;

        Ok(())
    }
}

// ============================================================================
// Mock PlaybackAdapter Implementation
// ============================================================================

#[derive(Clone)]
struct MockPlaybackAdapter {
    state: Arc<std::sync::Mutex<PlaybackState>>,
}

#[derive(Debug)]
struct PlaybackState {
    is_playing: bool,
    is_paused: bool,
    current_position: Duration,
    volume: f32,
    current_source: Option<AudioSource>,
    current_format: Option<AudioFormat>,
    should_fail: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_paused: false,
            current_position: Duration::from_secs(0),
            volume: 1.0,
            current_source: None,
            current_format: None,
            should_fail: false,
        }
    }
}

impl MockPlaybackAdapter {
    fn new() -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(PlaybackState::default())),
        }
    }

    fn with_failure(self, should_fail: bool) -> Self {
        let mut state = self.state.lock().unwrap();
        state.should_fail = should_fail;
        drop(state);
        self
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PlaybackAdapter for MockPlaybackAdapter {
    async fn play(&self, source: AudioSource, format: AudioFormat) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.should_fail {
            return Err(PlaybackError::PlaybackFailed("Mock playback failure".into()));
        }

        state.current_source = Some(source);
        state.current_format = Some(format);
        state.is_playing = true;
        state.is_paused = false;
        state.current_position = Duration::from_secs(0);

        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if !state.is_playing {
            return Err(PlaybackError::NoTrackLoaded);
        }

        state.is_paused = true;
        state.is_playing = false;
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if !state.is_paused {
            return Err(PlaybackError::PlaybackFailed("Not paused".into()));
        }

        state.is_playing = true;
        state.is_paused = false;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.is_playing = false;
        state.is_paused = false;
        state.current_position = Duration::from_secs(0);
        state.current_source = None;
        state.current_format = None;
        Ok(())
    }

    async fn seek(&self, position: Duration) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.current_source.is_none() {
            return Err(PlaybackError::NoTrackLoaded);
        }

        state.current_position = position;
        Ok(())
    }

    async fn set_volume(&self, volume: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&volume) {
            return Err(PlaybackError::InvalidVolume(volume));
        }

        let mut state = self.state.lock().unwrap();
        state.volume = volume;
        Ok(())
    }

    async fn get_position(&self) -> Result<Duration> {
        let state = self.state.lock().unwrap();
        if state.current_source.is_none() {
            return Err(PlaybackError::NoTrackLoaded);
        }

        Ok(state.current_position)
    }

    async fn is_playing(&self) -> Result<bool> {
        let state = self.state.lock().unwrap();
        Ok(state.is_playing)
    }
}

// ============================================================================
// Tests: AudioCodec
// ============================================================================

#[test]
fn test_audio_codec_classification() {
    // Lossless codecs
    assert!(AudioCodec::Flac.is_lossless());
    assert!(AudioCodec::Wav.is_lossless());
    assert!(AudioCodec::Alac.is_lossless());
    assert!(!AudioCodec::Flac.is_lossy());

    // Lossy codecs
    assert!(AudioCodec::Mp3.is_lossy());
    assert!(AudioCodec::Aac.is_lossy());
    assert!(AudioCodec::Vorbis.is_lossy());
    assert!(AudioCodec::Opus.is_lossy());
    assert!(!AudioCodec::Mp3.is_lossless());

    // Unknown/Other
    assert!(!AudioCodec::Unknown.is_lossless());
    assert!(!AudioCodec::Unknown.is_lossy());
}

// ============================================================================
// Tests: AudioFormat
// ============================================================================

#[test]
fn test_audio_format_creation() {
    let format = AudioFormat::new(AudioCodec::Mp3, 44100, 2, Some(16), Some(320));

    assert_eq!(format.codec, AudioCodec::Mp3);
    assert_eq!(format.sample_rate, 44100);
    assert_eq!(format.channels, 2);
    assert_eq!(format.bits_per_sample, Some(16));
    assert_eq!(format.bitrate, Some(320));
}

#[test]
fn test_audio_format_presets() {
    let cd = AudioFormat::cd_quality();
    assert_eq!(cd.codec, AudioCodec::Wav);
    assert_eq!(cd.sample_rate, 44100);
    assert_eq!(cd.channels, 2);
    assert_eq!(cd.bits_per_sample, Some(16));

    let hi_res = AudioFormat::hi_res();
    assert_eq!(hi_res.codec, AudioCodec::Flac);
    assert_eq!(hi_res.sample_rate, 96000);
    assert_eq!(hi_res.channels, 2);
    assert_eq!(hi_res.bits_per_sample, Some(24));
}

// ============================================================================
// Tests: AudioSource
// ============================================================================

#[test]
fn test_audio_source_local_file() {
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };

    assert!(!source.is_remote());
    assert!(!source.is_cached());
    assert_eq!(source.estimated_size(), None);
}

#[test]
fn test_audio_source_remote_stream() {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer token".to_string());

    let source = AudioSource::RemoteStream {
        url: "https://example.com/stream.mp3".to_string(),
        headers,
    };

    assert!(source.is_remote());
    assert!(!source.is_cached());
    assert_eq!(source.estimated_size(), None);
}

#[test]
fn test_audio_source_cached_chunk() {
    let data = bytes::Bytes::from_static(&[1, 2, 3, 4, 5]);
    let source = AudioSource::CachedChunk {
        data: data.clone(),
        codec_hint: Some(AudioCodec::Mp3),
    };

    assert!(!source.is_remote());
    assert!(source.is_cached());
    assert_eq!(source.estimated_size(), Some(5));
}

// ============================================================================
// Tests: AudioFrameChunk
// ============================================================================

#[test]
fn test_audio_frame_chunk_creation() {
    let samples = vec![0.1, -0.1, 0.2, -0.2];
    let chunk = AudioFrameChunk::new(samples.clone(), 2, Duration::from_secs(0));

    assert_eq!(chunk.samples, samples);
    assert_eq!(chunk.frames, 2);
    assert!(!chunk.is_empty());
}

#[test]
fn test_audio_frame_chunk_empty() {
    let chunk = AudioFrameChunk::new(Vec::new(), 0, Duration::from_secs(0));
    assert!(chunk.is_empty());
}

#[test]
fn test_audio_frame_chunk_duration() {
    let chunk = AudioFrameChunk::new(vec![0.0; 8820], 4410, Duration::from_secs(0));

    let duration = chunk.duration(44100);
    assert_eq!(duration.as_millis(), 100); // 4410 / 44100 = 0.1s
}

// ============================================================================
// Tests: ProbeResult
// ============================================================================

#[test]
fn test_probe_result_builder() {
    let format = AudioFormat::cd_quality();
    let mut tags = HashMap::new();
    tags.insert("title".to_string(), "Test Song".to_string());
    tags.insert("artist".to_string(), "Test Artist".to_string());

    let probe = ProbeResult::new(format.clone())
        .with_duration(Some(Duration::from_secs(240)))
        .with_tags(tags.clone());

    assert_eq!(probe.format, format);
    assert_eq!(probe.duration, Some(Duration::from_secs(240)));
    assert_eq!(probe.tags.get("title"), Some(&"Test Song".to_string()));
    assert_eq!(probe.tags.get("artist"), Some(&"Test Artist".to_string()));
}

// ============================================================================
// Tests: AudioDecoder Trait
// ============================================================================

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_probe() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 180);

    let probe = decoder.probe().await.expect("Probe should succeed");

    assert_eq!(probe.format.codec, AudioCodec::Mp3);
    assert_eq!(probe.format.sample_rate, 44100);
    assert_eq!(probe.format.channels, 2);
    assert_eq!(probe.duration, Some(Duration::from_secs(180)));
    assert!(probe.tags.contains_key("title"));
    assert!(probe.tags.contains_key("artist"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_decode_frames() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 1); // 1 second

    let mut total_frames = 0;
    while let Some(chunk) = decoder
        .decode_frames(4096)
        .await
        .expect("Decode should succeed")
    {
        total_frames += chunk.frames;
        assert!(!chunk.is_empty());
        assert_eq!(chunk.samples.len(), chunk.frames * 2); // Stereo
    }

    // Should decode ~44100 frames for 1 second at 44.1 kHz
    assert_eq!(total_frames, 44100);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_end_of_stream() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 1);

    // Decode all frames
    while decoder
        .decode_frames(44100)
        .await
        .expect("Decode should succeed")
        .is_some()
    {}

    // Next decode should return None (end of stream)
    let result = decoder
        .decode_frames(4096)
        .await
        .expect("Should return Ok(None)");
    assert!(result.is_none());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_seek() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 180);

    // Seek to 30 seconds
    decoder
        .seek(Duration::from_secs(30))
        .await
        .expect("Seek should succeed");

    // Decode next chunk should start from 30s position
    let chunk = decoder
        .decode_frames(4096)
        .await
        .expect("Decode should succeed")
        .expect("Should have chunk");

    assert_eq!(chunk.timestamp, Duration::from_secs(30));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_seek_not_supported() {
    let mut decoder =
        MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 180).with_seek_support(false);

    let result = decoder.seek(Duration::from_secs(30)).await;

    assert!(matches!(result, Err(PlaybackError::SeekNotSupported)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_seek_out_of_bounds() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 180);

    let result = decoder.seek(Duration::from_secs(300)).await; // Beyond duration

    assert!(matches!(result, Err(PlaybackError::SeekOutOfBounds(_))));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_audio_decoder_error_handling() {
    let mut decoder = MockAudioDecoder::new(AudioCodec::Mp3, 44100, 2, 180).with_failure(true);

    let probe_result = decoder.probe().await;
    assert!(matches!(probe_result, Err(PlaybackError::DecodingError(_))));

    let decode_result = decoder.decode_frames(4096).await;
    assert!(matches!(
        decode_result,
        Err(PlaybackError::DecodingError(_))
    ));
}

// ============================================================================
// Tests: PlaybackAdapter Trait
// ============================================================================

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_play() {
    let adapter = MockPlaybackAdapter::new();
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };
    let format = AudioFormat::cd_quality();

    adapter
        .play(source, format)
        .await
        .expect("Play should succeed");

    let is_playing = adapter.is_playing().await.expect("Should succeed");
    assert!(is_playing);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_pause_resume() {
    let adapter = MockPlaybackAdapter::new();
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };
    let format = AudioFormat::cd_quality();

    adapter.play(source, format).await.expect("Play should succeed");
    assert!(adapter.is_playing().await.unwrap());

    adapter.pause().await.expect("Pause should succeed");
    assert!(!adapter.is_playing().await.unwrap());

    adapter.resume().await.expect("Resume should succeed");
    assert!(adapter.is_playing().await.unwrap());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_stop() {
    let adapter = MockPlaybackAdapter::new();
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };
    let format = AudioFormat::cd_quality();

    adapter.play(source, format).await.expect("Play should succeed");
    assert!(adapter.is_playing().await.unwrap());

    adapter.stop().await.expect("Stop should succeed");
    assert!(!adapter.is_playing().await.unwrap());

    // Position should be reset
    let position_result = adapter.get_position().await;
    assert!(matches!(position_result, Err(PlaybackError::NoTrackLoaded)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_seek() {
    let adapter = MockPlaybackAdapter::new();
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };
    let format = AudioFormat::cd_quality();

    adapter.play(source, format).await.expect("Play should succeed");

    let target_position = Duration::from_secs(45);
    adapter
        .seek(target_position)
        .await
        .expect("Seek should succeed");

    let position = adapter
        .get_position()
        .await
        .expect("Get position should succeed");
    assert_eq!(position, target_position);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_volume() {
    let adapter = MockPlaybackAdapter::new();

    // Set valid volume
    adapter
        .set_volume(0.5)
        .await
        .expect("Set volume should succeed");
    adapter
        .set_volume(0.0)
        .await
        .expect("Mute should succeed");
    adapter
        .set_volume(1.0)
        .await
        .expect("Max volume should succeed");

    // Test invalid volumes
    let result = adapter.set_volume(-0.1).await;
    assert!(matches!(result, Err(PlaybackError::InvalidVolume(_))));

    let result = adapter.set_volume(1.5).await;
    assert!(matches!(result, Err(PlaybackError::InvalidVolume(_))));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_no_track_loaded() {
    let adapter = MockPlaybackAdapter::new();

    // Operations should fail when no track is loaded
    let pause_result = adapter.pause().await;
    assert!(matches!(pause_result, Err(PlaybackError::NoTrackLoaded)));

    let seek_result = adapter.seek(Duration::from_secs(10)).await;
    assert!(matches!(seek_result, Err(PlaybackError::NoTrackLoaded)));

    let position_result = adapter.get_position().await;
    assert!(matches!(position_result, Err(PlaybackError::NoTrackLoaded)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
async fn test_playback_adapter_error_handling() {
    let adapter = MockPlaybackAdapter::new().with_failure(true);
    let source = AudioSource::LocalFile {
        path: "/path/to/song.mp3".into(),
    };
    let format = AudioFormat::cd_quality();

    let result = adapter.play(source, format).await;
    assert!(matches!(result, Err(PlaybackError::PlaybackFailed(_))));
}

// ============================================================================
// Tests: Error Classification
// ============================================================================

#[test]
fn test_error_classification() {
    let network_err = PlaybackError::StreamingFailed("Connection lost".into());
    assert!(network_err.is_network_error());
    assert!(network_err.is_transient());
    assert!(!network_err.is_format_error());

    let format_err = PlaybackError::UnsupportedCodec("DTS".into());
    assert!(format_err.is_format_error());
    assert!(!format_err.is_network_error());
    assert!(!format_err.is_transient());

    let transient_err = PlaybackError::BufferUnderrun;
    assert!(transient_err.is_transient());
    assert!(!transient_err.is_format_error());
}
