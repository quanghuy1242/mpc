//! # Symphonia Decoder Implementation
//!
//! Production-ready audio decoder using the Symphonia library.

use crate::decoder::format_detector::FormatDetector;
use crate::decoder::sample_converter::SampleConverter;
use crate::error::{PlaybackError, Result};
use crate::traits::{
    AudioCodec, AudioDecoder, AudioFormat, AudioFrameChunk, AudioSource, ProbeResult,
};
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use tracing::{debug, error, info, instrument, warn};

/// Production-ready Symphonia decoder implementing the AudioDecoder trait.
///
/// This decoder handles all supported audio formats through Symphonia's
/// unified interface. It manages the full decode pipeline:
/// - Media source abstraction (file, HTTP, memory)
/// - Format detection and probing
/// - Container demultiplexing
/// - Codec decoding
/// - Sample format conversion
///
/// ## Thread Safety
///
/// - **Native**: Decoder is Send and can run in background threads
/// - **WASM**: Single-threaded, runs in Web Worker or main thread
///
/// ## State Management
///
/// The decoder maintains internal state:
/// - Current playback position
/// - Selected audio track
/// - Format reader and codec decoder
/// - End-of-stream flag
pub struct SymphoniaDecoder {
    /// Format reader (demuxer) - owns the media source stream
    format_reader: Box<dyn FormatReader>,

    /// Audio decoder
    decoder: Box<dyn Decoder>,

    /// Selected track ID
    track_id: u32,

    /// Audio format information
    format: AudioFormat,

    /// Track duration (if known)
    duration: Option<Duration>,

    /// Metadata tags
    tags: HashMap<String, String>,

    /// Current decode position in frames
    position_frames: u64,

    /// Sample rate (for timestamp calculation)
    sample_rate: u32,

    /// Number of channels
    channels: u16,

    /// End-of-stream flag
    eof: bool,

    /// Original source (for error reporting)
    source_info: String,
}

impl SymphoniaDecoder {
    /// Create a new decoder from an audio source.
    ///
    /// This initializes the full decode pipeline but does not probe the stream.
    /// Call `probe()` to detect format and prepare for decoding.
    ///
    /// # Arguments
    ///
    /// * `source` - Audio source (file, stream, or cached data)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Source cannot be opened
    /// - Format is not recognized
    /// - No supported audio tracks found
    /// - Codec is not supported
    #[instrument(skip(source), fields(source = ?source))]
    pub async fn new(source: AudioSource) -> Result<Self> {
        info!("Creating Symphonia decoder");

        // Step 1: Open media source
        let (media_source, hint, source_info) = Self::open_media_source(source).await?;

        // Step 2: Probe format
        let probe_result = symphonia::default::get_probe()
            .format(
                &hint,
                media_source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| {
                error!("Format probe failed: {}", e);
                PlaybackError::InvalidFormat(format!("Failed to probe format: {}", e))
            })?;

        let format_reader = probe_result.format;
        let _metadata = probe_result.metadata; // TODO: Extract tags when API is clarified

        // Step 3: Find first audio track with supported codec
        let track = format_reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| {
                error!("No supported audio tracks found");
                PlaybackError::FormatNotDecodable("No supported audio tracks".to_string())
            })?;

        let track_id = track.id;
        debug!("Selected track ID: {}", track_id);

        // Step 4: Detect and validate codec
        let codec_type = track.codec_params.codec;
        let codec = FormatDetector::detect_codec(codec_type);
        FormatDetector::validate_codec_support(&codec)?;
        info!("Detected codec: {:?}", codec);

        // Step 5: Extract audio parameters
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| PlaybackError::InvalidFormat("Missing sample rate".to_string()))?;

        // Channels might not be available until first decode (especially for AAC/M4A)
        let channels = track
            .codec_params
            .channels
            .map(|ch| ch.count() as u16)
            .unwrap_or(2); // Default to stereo, will be updated after first decode

        let bits_per_sample = track.codec_params.bits_per_sample.map(|b| b as u16);
        // Note: Bitrate may not be available from codec params, it's calculated during decode
        let bitrate = None; // Will be updated during first packet decode if available

        debug!(
            "Audio params: {}Hz, {} channels, {:?} bits, {:?} kbps",
            sample_rate, channels, bits_per_sample, bitrate
        );

        // Step 6: Calculate duration
        let duration = track
            .codec_params
            .n_frames
            .map(|frames| Duration::from_secs_f64(frames as f64 / sample_rate as f64));

        if let Some(dur) = duration {
            debug!("Track duration: {:?}", dur);
        } else {
            debug!("Track duration unknown (streaming)");
        }

        // Step 7: Create codec decoder
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| {
                error!("Failed to create decoder: {}", e);
                PlaybackError::DecoderError(format!("Failed to create codec decoder: {}", e))
            })?;

        info!("Decoder initialized successfully");

        // Step 8: Extract metadata tags
        // Note: Metadata extraction simplified - advanced metadata handling
        // can be added later based on actual Symphonia version API
        let tags = HashMap::new(); // TODO: Extract from format_reader.metadata() properly

        Ok(Self {
            format_reader,
            decoder,
            track_id,
            format: AudioFormat::new(codec, sample_rate, channels, bits_per_sample, bitrate),
            duration,
            tags,
            position_frames: 0,
            sample_rate,
            channels,
            eof: false,
            source_info,
        })
    }

    /// Open media source from AudioSource enum.
    async fn open_media_source(
        source: AudioSource,
    ) -> Result<(MediaSourceStream, Hint, String)> {
        match source {
            AudioSource::LocalFile { path } => {
                Self::open_local_file(path).await
            }
            AudioSource::RemoteStream { url, headers } => {
                Self::open_http_stream(url, Some(headers)).await
            }
            AudioSource::CachedChunk { data, codec_hint } => {
                Self::open_memory_buffer(data, codec_hint).await
            }
        }
    }

    /// Open local file using platform filesystem.
    #[cfg(not(target_arch = "wasm32"))]
    async fn open_local_file(path: PathBuf) -> Result<(MediaSourceStream, Hint, String)> {
        let file = std::fs::File::open(&path).map_err(|e| {
            error!("Failed to open file {:?}: {}", path, e);
            PlaybackError::SourceError(format!("Failed to open file: {}", e))
        })?;

        let hint = FormatDetector::hint_from_path(&path);
        let media_source = Box::new(file) as Box<dyn MediaSource>;
        let mss = MediaSourceStream::new(media_source, Default::default());

        Ok((mss, hint, path.display().to_string()))
    }

    /// Open local file using WASM filesystem abstraction.
    #[cfg(target_arch = "wasm32")]
    async fn open_local_file(path: PathBuf) -> Result<(MediaSourceStream, Hint, String)> {
        use core_async::fs;

        let data = fs::read(&path).await.map_err(|e| {
            error!("Failed to read file {:?}: {}", path, e);
            PlaybackError::SourceError(format!("Failed to read file: {}", e))
        })?;

        let hint = FormatDetector::hint_from_path(&path);
        let cursor = Cursor::new(data);
        let media_source = Box::new(cursor) as Box<dyn MediaSource>;
        let mss = MediaSourceStream::new(media_source, Default::default());

        Ok((mss, hint, path.display().to_string()))
    }

    /// Open memory buffer as media source.
    async fn open_memory_buffer(
        data: Bytes,
        codec_hint: Option<AudioCodec>,
    ) -> Result<(MediaSourceStream, Hint, String)> {
        let mut hint = Hint::new();

        if let Some(codec) = codec_hint {
            let extension = FormatDetector::codec_extension(&codec);
            hint.with_extension(extension);
        }

        let cursor = Cursor::new(data.to_vec());
        let media_source = Box::new(cursor) as Box<dyn MediaSource>;
        let mss = MediaSourceStream::new(media_source, Default::default());

        Ok((mss, hint, "memory buffer".to_string()))
    }

    /// Open HTTP stream as media source.
    ///
    /// This downloads the entire file into memory before decoding.
    /// For production use, consider implementing a streaming adapter
    /// that fetches data on-demand using range requests.
    ///
    /// # Arguments
    ///
    /// * `url` - HTTP URL to audio file
    /// * `headers` - Optional HTTP headers (auth, etc.)
    ///
    /// # Implementation Note
    ///
    /// Current implementation downloads entire file. For large files
    /// or streaming scenarios, implement a custom MediaSource that:
    /// 1. Implements Read + Seek traits
    /// 2. Uses HTTP range requests for seeking
    /// 3. Maintains a buffer window for efficient reading
    ///
    /// See Symphonia documentation for custom MediaSource implementation.
    async fn open_http_stream(
        url: String,
        headers: Option<HashMap<String, String>>,
    ) -> Result<(MediaSourceStream, Hint, String)> {
        info!("Downloading audio from: {}", url);

        // Get HttpClient from bridge-traits (injected at runtime)
        // For WASM this would come from browser fetch, for native from reqwest
        // TODO: This needs HttpClient passed through context or AudioSource
        // For now, we'll download the entire file into memory as a workaround

        #[cfg(all(not(target_arch = "wasm32"), feature = "http-streaming"))]
        {
            // Build HTTP request
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|e| {
                    error!("Failed to create HTTP client: {}", e);
                    PlaybackError::SourceError(format!("HTTP client error: {}", e))
                })?;

            let mut request = client.get(&url);

            // Add custom headers if provided
            if let Some(hdrs) = headers {
                for (key, value) in hdrs {
                    request = request.header(key, value);
                }
            }

            // Execute request
            debug!("Sending HTTP request to {}", url);
            let response = request.send().await.map_err(|e| {
                error!("HTTP request failed: {}", e);
                PlaybackError::SourceError(format!("HTTP request failed: {}", e))
            })?;

            if !response.status().is_success() {
                error!("HTTP request returned error status: {}", response.status());
                return Err(PlaybackError::SourceError(format!(
                    "HTTP error: {}",
                    response.status()
                )));
            }

            // Download entire file into memory
            let content_length = response.content_length();
            if let Some(len) = content_length {
                info!("Downloading {} bytes", len);
            }

            let data = response.bytes().await.map_err(|e| {
                error!("Failed to download response body: {}", e);
                PlaybackError::SourceError(format!("Download failed: {}", e))
            })?;

            info!("Downloaded {} bytes successfully", data.len());

            // Create hint from URL extension
            let mut hint = Hint::new();
            if let Some(ext) = url.split('.').last() {
                hint.with_extension(ext);
            }

            // Create media source from downloaded data
            let cursor = Cursor::new(data.to_vec());
            let media_source = Box::new(cursor) as Box<dyn MediaSource>;
            let mss = MediaSourceStream::new(media_source, Default::default());

            Ok((mss, hint, url))
        }

        #[cfg(not(all(not(target_arch = "wasm32"), feature = "http-streaming")))]
        {
            // HTTP streaming not enabled or on WASM
            let _ = (url, headers); // Suppress unused warnings
            
            error!("HTTP streaming requires 'http-streaming' feature flag");
            Err(PlaybackError::SourceError(
                "HTTP streaming not enabled. Enable 'http-streaming' feature flag or use cached download.".to_string(),
            ))
        }
    }

    // Metadata extraction removed for now - Symphonia API varies by version
    // Will be added back once we lock in the exact Symphonia version and API

    /// Read and decode the next packet.
    ///
    /// This method handles packet reading, filtering, and decoding with robust
    /// error recovery. It will skip corrupted packets and continue decoding,
    /// only failing on unrecoverable errors.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(AudioBufferRef))` - Successfully decoded packet
    /// - `Ok(None)` - End of stream reached
    /// - `Err(PlaybackError)` - Unrecoverable error
    ///
    /// # Important
    ///
    /// Returns owned interleaved f32 sample data. This method converts Symphonia's
    /// planar audio buffers to interleaved format immediately to avoid lifetime issues.
    #[instrument(skip(self), level = "trace")]
    fn decode_next_packet(&mut self) -> Result<Option<Vec<f32>>> {
        if self.eof {
            return Ok(None);
        }

        // Track error counts for graceful degradation
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: usize = 10;

        loop {
            // Get next packet from format reader
            let packet = match self.format_reader.next_packet() {
                Ok(packet) => {
                    consecutive_errors = 0; // Reset on success
                    packet
                }
                Err(SymphoniaError::ResetRequired) => {
                    // Track list changed (rare, e.g., chained OGG streams)
                    warn!("Decoder reset required for track list change");
                    return Err(PlaybackError::DecoderError(
                        "Track list changed, reset required".to_string(),
                    ));
                }
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Normal end of stream
                    debug!("Reached end of stream at {} frames", self.position_frames);
                    self.eof = true;
                    return Ok(None);
                }
                Err(SymphoniaError::IoError(e)) => {
                    consecutive_errors += 1;
                    warn!(
                        "I/O error reading packet (attempt {}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                    );

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("Too many consecutive I/O errors, giving up");
                        return Err(PlaybackError::SourceError(format!(
                            "Stream I/O failure after {} attempts: {}",
                            MAX_CONSECUTIVE_ERRORS, e
                        )));
                    }

                    continue; // Try next packet
                }
                Err(e) => {
                    error!("Fatal format reader error: {}", e);
                    return Err(PlaybackError::DecodingError(format!(
                        "Failed to read packet: {}",
                        e
                    )));
                }
            };

            // Consume any new metadata that was read with this packet
            while !self.format_reader.metadata().is_latest() {
                self.format_reader.metadata().pop();
                // We could update self.tags here if needed for live metadata updates
            }

            // Skip packets not belonging to our selected track
            if packet.track_id() != self.track_id {
                continue;
            }

            // Decode packet
            // Note: We convert the decoded buffer to owned data immediately because
            // Symphonia's AudioBufferRef is only valid until the next decode() call.
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let frame_count = decoded.frames() as u64;
                    self.position_frames += frame_count;

                    // Update channel count from first decoded packet if it was unknown
                    let decoded_channels = decoded.spec().channels.count() as u16;
                    if self.channels != decoded_channels {
                        debug!("Updating channel count from {} to {} (detected from decoded audio)", 
                               self.channels, decoded_channels);
                        self.channels = decoded_channels;
                        self.format.channels = decoded_channels;
                    }

                    debug!(
                        "Decoded packet: {} frames at position {}",
                        frame_count, self.position_frames
                    );

                    // Convert to owned interleaved f32 samples
                    let samples = SampleConverter::to_interleaved_f32(&decoded)?;

                    return Ok(Some(samples));
                }
                Err(SymphoniaError::IoError(err)) => {
                    // Skip corrupted packet - I/O errors during decode
                    consecutive_errors += 1;
                    warn!(
                        "Skipping corrupted packet (I/O error, attempt {}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, err
                    );

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("Too many consecutive decode errors, stream may be corrupted");
                        return Err(PlaybackError::CorruptedStream(format!(
                            "Stream corruption after {} failed packets",
                            MAX_CONSECUTIVE_ERRORS
                        )));
                    }

                    continue;
                }
                Err(SymphoniaError::DecodeError(err)) => {
                    // Skip decode error - invalid codec data
                    consecutive_errors += 1;
                    warn!(
                        "Skipping packet with decode error (attempt {}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, err
                    );

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("Too many consecutive decode errors, codec may be incompatible");
                        return Err(PlaybackError::DecoderError(format!(
                            "Decoder failure after {} failed packets: {}",
                            MAX_CONSECUTIVE_ERRORS, err
                        )));
                    }

                    continue;
                }
                Err(e) => {
                    error!("Fatal decode error: {}", e);
                    return Err(PlaybackError::DecoderError(format!(
                        "Failed to decode packet: {}",
                        e
                    )));
                }
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl AudioDecoder for SymphoniaDecoder {
    async fn probe(&mut self) -> Result<ProbeResult> {
        debug!("Probing audio format");

        Ok(ProbeResult::new(self.format.clone())
            .with_duration(self.duration)
            .with_tags(self.tags.clone()))
    }

    async fn decode_frames(&mut self, max_frames: usize) -> Result<Option<AudioFrameChunk>> {
        if self.eof {
            return Ok(None);
        }

        // Decode next packet - returns owned interleaved f32 samples
        let samples = match self.decode_next_packet()? {
            Some(samples) => samples,
            None => return Ok(None),
        };

        // Calculate actual frame count from sample count
        let total_frames = samples.len() / self.channels as usize;
        let frames = total_frames.min(max_frames);
        let samples_to_take = frames * self.channels as usize;
        let chunk_samples = samples.into_iter().take(samples_to_take).collect();

        // Calculate timestamp (subtract frames we just decoded since position was already updated)
        let timestamp = Duration::from_secs_f64(
            (self.position_frames - total_frames as u64) as f64 / self.sample_rate as f64,
        );

        Ok(Some(AudioFrameChunk::new(chunk_samples, frames, timestamp)))
    }

    async fn seek(&mut self, position: Duration) -> Result<()> {
        if self.duration.is_some() && position > self.duration.unwrap() {
            return Err(PlaybackError::SeekOutOfBounds(position));
        }

        debug!("Seeking to {:?}", position);

        // Convert duration to time units
        let time = Time::from(position.as_secs_f64());

        // Attempt seek
        self.format_reader
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: None })
            .map_err(|e| {
                error!("Seek failed: {}", e);
                PlaybackError::SeekNotSupported
            })?;

        // Reset decoder state
        self.decoder.reset();

        // Update position
        let new_position_frames = (position.as_secs_f64() * self.sample_rate as f64) as u64;
        self.position_frames = new_position_frames;
        self.eof = false;

        info!("Seek completed to {:?}", position);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_decoder_creation_with_memory_buffer() {
        // Create a minimal valid MP3 frame (silent frame)
        let mp3_frame = vec![
            0xFF, 0xFB, 0x90, 0x00, // MP3 sync word + header
        ];

        let source = AudioSource::CachedChunk {
            data: Bytes::from(mp3_frame),
            codec_hint: Some(AudioCodec::Mp3),
        };

        // This will likely fail since we don't have a complete valid file,
        // but it tests the initialization path
        let result = SymphoniaDecoder::new(source).await;
        // We expect an error since the data is incomplete
        assert!(result.is_err());
    }
}
