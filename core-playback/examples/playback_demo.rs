//! # Playback Traits Usage Example
//!
//! This example demonstrates how to use the AudioDecoder and PlaybackAdapter traits
//! to implement a simple audio playback flow.
//!
//! Run with: `cargo run --example playback_demo --package core-playback`

use core_playback::{
    AudioCodec, AudioDecoder, AudioFormat, AudioFrameChunk, AudioSource, PlaybackAdapter,
    PlaybackError, ProbeResult, Result,
};
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// Simple In-Memory Audio Decoder (for demonstration)
// ============================================================================

struct SimpleAudioDecoder {
    samples: Vec<f32>,
    position: usize,
    sample_rate: u32,
    channels: u16,
}

impl SimpleAudioDecoder {
    /// Create a decoder with synthetic audio (sine wave)
    fn new(duration_secs: f64, frequency: f64) -> Self {
        let sample_rate = 44100u32;
        let channels = 2u16;
        let total_frames = (sample_rate as f64 * duration_secs) as usize;

        // Generate a simple sine wave
        let mut samples = Vec::with_capacity(total_frames * channels as usize);
        for i in 0..total_frames {
            let t = i as f64 / sample_rate as f64;
            let sample = (2.0 * std::f64::consts::PI * frequency * t).sin() as f32 * 0.3;

            // Stereo: duplicate sample for both channels
            samples.push(sample);
            samples.push(sample);
        }

        Self {
            samples,
            position: 0,
            sample_rate,
            channels,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl AudioDecoder for SimpleAudioDecoder {
    async fn probe(&mut self) -> Result<ProbeResult> {
        let format = AudioFormat::new(AudioCodec::Wav, self.sample_rate, self.channels, Some(16), None);

        let total_frames = self.samples.len() / self.channels as usize;
        let duration_secs = total_frames as f64 / self.sample_rate as f64;

        let mut tags = HashMap::new();
        tags.insert("title".to_string(), "Sine Wave Demo".to_string());
        tags.insert("artist".to_string(), "Core Playback".to_string());

        Ok(ProbeResult::new(format)
            .with_duration(Some(Duration::from_secs_f64(duration_secs)))
            .with_tags(tags))
    }

    async fn decode_frames(&mut self, max_frames: usize) -> Result<Option<AudioFrameChunk>> {
        let total_samples = self.samples.len();
        if self.position >= total_samples {
            return Ok(None); // End of stream
        }

        let samples_remaining = total_samples - self.position;
        let frames_to_decode = max_frames.min(samples_remaining / self.channels as usize);
        let samples_to_decode = frames_to_decode * self.channels as usize;

        let chunk_samples = self.samples[self.position..self.position + samples_to_decode].to_vec();
        let current_frame = self.position / self.channels as usize;
        let timestamp = Duration::from_secs_f64(current_frame as f64 / self.sample_rate as f64);

        self.position += samples_to_decode;

        Ok(Some(AudioFrameChunk::new(
            chunk_samples,
            frames_to_decode,
            timestamp,
        )))
    }

    async fn seek(&mut self, position: Duration) -> Result<()> {
        let target_frame = (position.as_secs_f64() * self.sample_rate as f64) as usize;
        let target_sample = target_frame * self.channels as usize;

        if target_sample >= self.samples.len() {
            return Err(PlaybackError::SeekOutOfBounds(position));
        }

        self.position = target_sample;
        Ok(())
    }
}

// ============================================================================
// Simple Console Playback Adapter (for demonstration)
// ============================================================================

struct ConsolePlaybackAdapter {
    is_playing: std::sync::Arc<std::sync::Mutex<bool>>,
    position: std::sync::Arc<std::sync::Mutex<Duration>>,
    volume: std::sync::Arc<std::sync::Mutex<f32>>,
}

impl ConsolePlaybackAdapter {
    fn new() -> Self {
        Self {
            is_playing: std::sync::Arc::new(std::sync::Mutex::new(false)),
            position: std::sync::Arc::new(std::sync::Mutex::new(Duration::from_secs(0))),
            volume: std::sync::Arc::new(std::sync::Mutex::new(1.0)),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PlaybackAdapter for ConsolePlaybackAdapter {
    async fn play(&self, source: AudioSource, format: AudioFormat) -> Result<()> {
        println!("▶️  Playing audio:");
        println!("   Source: {:?}", source);
        println!("   Format: {:?}", format);

        *self.is_playing.lock().unwrap() = true;
        *self.position.lock().unwrap() = Duration::from_secs(0);

        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        println!("⏸️  Paused");
        *self.is_playing.lock().unwrap() = false;
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        println!("▶️  Resumed");
        *self.is_playing.lock().unwrap() = true;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        println!("⏹️  Stopped");
        *self.is_playing.lock().unwrap() = false;
        *self.position.lock().unwrap() = Duration::from_secs(0);
        Ok(())
    }

    async fn seek(&self, position: Duration) -> Result<()> {
        println!("⏩  Seeking to {:?}", position);
        *self.position.lock().unwrap() = position;
        Ok(())
    }

    async fn set_volume(&self, volume: f32) -> Result<()> {
        if !(0.0..=1.0).contains(&volume) {
            return Err(PlaybackError::InvalidVolume(volume));
        }

        println!("🔊 Volume set to {:.0}%", volume * 100.0);
        *self.volume.lock().unwrap() = volume;
        Ok(())
    }

    async fn get_position(&self) -> Result<Duration> {
        Ok(*self.position.lock().unwrap())
    }

    async fn is_playing(&self) -> Result<bool> {
        Ok(*self.is_playing.lock().unwrap())
    }
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    println!("🎵 Core Playback - Traits Demo\n");

    // Create a simple decoder with a 3-second sine wave at 440 Hz (A4)
    let mut decoder = SimpleAudioDecoder::new(3.0, 440.0);

    // Probe the audio format
    println!("📊 Probing audio stream...");
    let probe = decoder.probe().await?;
    println!("   Codec: {:?}", probe.format.codec);
    println!("   Sample Rate: {} Hz", probe.format.sample_rate);
    println!("   Channels: {}", probe.format.channels);
    println!("   Duration: {:?}", probe.duration);
    println!("   Tags: {:?}\n", probe.tags);

    // Create a simple console adapter
    let adapter = ConsolePlaybackAdapter::new();

    // Start playback
    let source = AudioSource::CachedChunk {
        data: bytes::Bytes::new(),
        codec_hint: Some(AudioCodec::Wav),
    };
    adapter.play(source, probe.format.clone()).await?;

    // Simulate decoding and playback
    println!("\n🎧 Decoding audio frames...");
    let mut frame_count = 0;
    let mut total_samples = 0;

    while let Some(chunk) = decoder.decode_frames(4096).await? {
        frame_count += 1;
        total_samples += chunk.frames;

        // Update adapter position
        *adapter.position.lock().unwrap() = chunk.timestamp;

        // Print progress every 10 frames
        if frame_count % 10 == 0 {
            let position = adapter.get_position().await?;
            println!(
                "   Frame {}: {} frames decoded, position: {:.2}s",
                frame_count,
                chunk.frames,
                position.as_secs_f64()
            );
        }
    }

    println!("\n✅ Decoding complete!");
    println!("   Total frames: {}", frame_count);
    println!("   Total samples: {}", total_samples);

    // Demonstrate playback controls
    println!("\n🎮 Testing playback controls...");

    adapter.pause().await?;
    println!("   Playing: {}", adapter.is_playing().await?);

    adapter.resume().await?;
    println!("   Playing: {}", adapter.is_playing().await?);

    adapter.seek(Duration::from_secs(1)).await?;
    let pos = adapter.get_position().await?;
    println!("   Position after seek: {:?}", pos);

    adapter.set_volume(0.5).await?;
    adapter.set_volume(0.0).await?; // Mute
    adapter.set_volume(1.0).await?; // Max

    adapter.stop().await?;

    println!("\n🎉 Demo completed successfully!");

    Ok(())
}
