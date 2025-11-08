// TODO: Comprehensive streaming tests will be added after HTTP client integration is complete
// For now, testing basic ring buffer, config, and statistics functionality

use core_playback::{
    config::{StreamingConfig, StreamingState, StreamingStats},
    ring_buffer::RingBuffer,
    traits::{AudioCodec, AudioFormat},
};

#[test]
fn test_streaming_config_default() {
    let config = StreamingConfig::default();
    assert!(config.buffer_frames > 0);
    assert!(config.min_buffer_frames > 0);
    assert!(config.min_buffer_frames <= config.buffer_frames);
    assert!(config.prefetch_threshold > 0.0 && config.prefetch_threshold < 1.0);
    assert!(config.decode_chunk_frames > 0);
}

#[test]
fn test_streaming_config_presets() {
    // Default preset
    let default = StreamingConfig::default();
    assert_eq!(default.buffer_frames, 88200); // 2 seconds at 44.1kHz

    // Low latency preset
    let low_latency = StreamingConfig::low_latency();
    assert_eq!(low_latency.buffer_frames, 22050); // 0.5 seconds at 44.1kHz
    assert!(low_latency.buffer_frames < default.buffer_frames);

    // High quality preset
    let high_quality = StreamingConfig::high_quality();
    assert_eq!(high_quality.buffer_frames, 220500); // 5 seconds at 44.1kHz
    assert!(high_quality.buffer_frames > default.buffer_frames);
}

#[test]
fn test_streaming_config_buffer_samples() {
    let config = StreamingConfig::default();

    // Stereo
    let samples_stereo = config.buffer_samples(2);
    assert_eq!(samples_stereo, config.buffer_frames * 2);

    // 5.1 surround
    let samples_surround = config.buffer_samples(6);
    assert_eq!(samples_surround, config.buffer_frames * 6);
}

#[test]
fn test_streaming_state_transitions() {
    assert_eq!(StreamingState::Idle, StreamingState::Idle);
    assert_ne!(StreamingState::Idle, StreamingState::Buffering);

    // Test Debug formatting
    let state = StreamingState::Streaming;
    assert_eq!(format!("{:?}", state), "Streaming");
}

#[test]
fn test_streaming_stats_default() {
    let stats = StreamingStats::default();
    assert_eq!(stats.total_frames_buffered, 0);
    assert_eq!(stats.total_frames_consumed, 0);
    assert_eq!(stats.current_buffer_frames, 0);
    assert_eq!(stats.total_bytes_downloaded, 0);
    assert_eq!(stats.http_requests, 0);
    assert_eq!(stats.underrun_count, 0);
    assert_eq!(stats.avg_download_speed, 0.0);
    assert_eq!(stats.avg_decode_time_ms, 0.0);
}

#[test]
fn test_streaming_stats_calculations() {
    let mut stats = StreamingStats::default();
    stats.total_frames_buffered = 44100;
    stats.total_frames_consumed = 22050;
    stats.current_buffer_frames = 22050;
    stats.total_bytes_downloaded = 1024 * 1024; // 1 MB
    stats.http_requests = 10;
    stats.underrun_count = 2;
    stats.avg_download_speed = 128.0 * 1024.0; // 128 KB/s
    stats.avg_decode_time_ms = 5.0;

    assert_eq!(stats.total_frames_buffered, 44100);
    assert_eq!(stats.total_frames_consumed, 22050);
    assert_eq!(stats.current_buffer_frames, 22050);
    assert_eq!(stats.total_bytes_downloaded, 1024 * 1024);
    assert_eq!(stats.http_requests, 10);
    assert_eq!(stats.underrun_count, 2);
    assert_eq!(stats.avg_download_speed, 128.0 * 1024.0);
    assert_eq!(stats.avg_decode_time_ms, 5.0);
}

#[test]
fn test_streaming_stats_buffer_fill() {
    let mut stats = StreamingStats::default();
    stats.current_buffer_frames = 50;

    assert_eq!(stats.buffer_fill_percentage(100), 0.5);
    assert_eq!(stats.buffer_fill_percentage(50), 1.0);
    assert_eq!(stats.buffer_fill_percentage(200), 0.25);
}

#[test]
fn test_streaming_stats_buffer_critical() {
    let mut stats = StreamingStats::default();
    stats.current_buffer_frames = 1000;

    assert!(!stats.is_buffer_critical(500));
    assert!(stats.is_buffer_critical(2000));
}

#[test]
fn test_ring_buffer_creation() {
    let buffer = RingBuffer::new(1000);
    assert_eq!(buffer.capacity(), 1000);
    assert_eq!(buffer.available(), 0);
    assert_eq!(buffer.free_space(), 1000);
    assert!(buffer.is_empty());
    assert!(!buffer.is_full());
}

#[test]
fn test_ring_buffer_write_read() {
    let buffer = RingBuffer::new(1000);

    // Write some samples
    let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let written = buffer.write(&samples);
    assert_eq!(written, 5);
    assert_eq!(buffer.available(), 5);
    assert_eq!(buffer.free_space(), 995);

    // Read them back
    let mut output = vec![0.0; 5];
    let read = buffer.read(&mut output);
    assert_eq!(read, 5);
    assert_eq!(output, samples);
    assert_eq!(buffer.available(), 0);
    assert!(buffer.is_empty());
}

#[test]
fn test_ring_buffer_wrap_around() {
    let buffer = RingBuffer::new(100);

    // Fill the buffer (can only write capacity - 1)
    let samples: Vec<f32> = (0..99).map(|i| i as f32).collect();
    let written = buffer.write(&samples);
    assert_eq!(written, 99);
    assert!(buffer.is_full());

    // Read half
    let mut output = vec![0.0; 50];
    buffer.read(&mut output);
    assert_eq!(buffer.available(), 49);

    // Write more (should wrap around)
    let more_samples = vec![99.0, 100.0, 101.0];
    let written = buffer.write(&more_samples);
    assert_eq!(written, 3);
    assert_eq!(buffer.available(), 52);

    // Read remaining
    let mut output = vec![0.0; 52];
    let read = buffer.read(&mut output);
    assert_eq!(read, 52);
    
    // Check first 49 are from original (50-98), last 3 are new (99-101)
    assert_eq!(output[0], 50.0);
    assert_eq!(output[48], 98.0);
    assert_eq!(output[49], 99.0);
    assert_eq!(output[50], 100.0);
    assert_eq!(output[51], 101.0);
}

#[test]
fn test_ring_buffer_partial_read() {
    let buffer = RingBuffer::new(1000);

    // Write 10 samples
    let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    buffer.write(&samples);

    // Try to read 15 (should only get 10)
    let mut output = vec![0.0; 15];
    let read = buffer.read(&mut output);
    assert_eq!(read, 10);
}

#[test]
fn test_ring_buffer_overwrite() {
    let buffer = RingBuffer::new(10);

    // Write up to capacity - 1 (9 samples)
    let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
    let written = buffer.write(&samples);
    assert_eq!(written, 9);
    assert!(buffer.is_full());

    // Ring buffer maintains 1 empty slot to distinguish full from empty
    // So free_space should be limited
    let free = buffer.free_space();
    assert!(free <= 1);

    // Read some to make space
    let mut output = vec![0.0; 5];
    buffer.read(&mut output);

    // Now should have more space
    assert!(buffer.free_space() >= 5);
    
    // Write more samples
    let more = vec![10.0, 11.0, 12.0];
    let written = buffer.write(&more);
    assert_eq!(written, 3);
}

#[test]
fn test_ring_buffer_fill_level() {
    let buffer = RingBuffer::new(1000);

    assert_eq!(buffer.fill_level(), 0.0);

    buffer.write(&vec![0.0; 500]);
    assert_eq!(buffer.fill_level(), 0.5);

    // Can only write capacity - 1 total, so write 499 more (total 999)
    buffer.write(&vec![0.0; 499]);
    assert!((buffer.fill_level() - 0.999).abs() < 0.001); // Almost 1.0
}

#[test]
fn test_ring_buffer_clear() {
    let buffer = RingBuffer::new(1000);

    buffer.write(&vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(buffer.available(), 5);

    buffer.clear();
    assert_eq!(buffer.available(), 0);
    assert!(buffer.is_empty());
}

#[test]
fn test_audio_format_creation() {
    let format = AudioFormat::new(
        AudioCodec::Mp3,
        44100,
        2,
        Some(16),
        Some(192),
    );

    assert_eq!(format.codec, AudioCodec::Mp3);
    assert_eq!(format.sample_rate, 44100);
    assert_eq!(format.channels, 2);
    assert_eq!(format.bits_per_sample, Some(16));
    assert_eq!(format.bitrate, Some(192));
}

#[test]
fn test_audio_format_presets() {
    let cd = AudioFormat::cd_quality();
    assert_eq!(cd.codec, AudioCodec::Wav);
    assert_eq!(cd.sample_rate, 44100);
    assert_eq!(cd.channels, 2);
    assert_eq!(cd.bits_per_sample, Some(16));

    let hires = AudioFormat::hi_res();
    assert_eq!(hires.codec, AudioCodec::Flac);
    assert_eq!(hires.sample_rate, 96000);
    assert_eq!(hires.channels, 2);
    assert_eq!(hires.bits_per_sample, Some(24));
}

#[test]
fn test_audio_codec_is_lossless() {
    assert!(AudioCodec::Flac.is_lossless());
    assert!(AudioCodec::Wav.is_lossless());
    assert!(AudioCodec::Alac.is_lossless());
    assert!(!AudioCodec::Mp3.is_lossless());
    assert!(!AudioCodec::Aac.is_lossless());
}

#[test]
fn test_audio_codec_is_lossy() {
    assert!(AudioCodec::Mp3.is_lossy());
    assert!(AudioCodec::Aac.is_lossy());
    assert!(AudioCodec::Vorbis.is_lossy());
    assert!(AudioCodec::Opus.is_lossy());
    assert!(!AudioCodec::Flac.is_lossy());
    assert!(!AudioCodec::Wav.is_lossy());
}
