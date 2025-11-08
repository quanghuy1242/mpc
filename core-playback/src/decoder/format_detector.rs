//! # Format Detection Module
//!
//! Provides format detection and validation using Symphonia's probe system.

use crate::error::{PlaybackError, Result};
use crate::traits::AudioCodec;
use std::path::Path;
use symphonia::core::codecs::CodecType;
use symphonia::core::probe::Hint;
use tracing::{debug, warn};

/// Format detector for audio streams.
///
/// This struct provides utilities for detecting audio format from file extensions,
/// MIME types, and container probing. It generates hints for Symphonia's probe
/// system to optimize format detection.
pub struct FormatDetector;

impl FormatDetector {
    /// Create a probe hint from file path.
    ///
    /// Extracts the file extension and creates a Symphonia `Hint` to guide
    /// format detection. This significantly improves probe accuracy and speed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_playback::FormatDetector;
    /// use std::path::Path;
    ///
    /// let hint = FormatDetector::hint_from_path(Path::new("/music/song.mp3"));
    /// // Hint will contain extension "mp3"
    /// ```
    pub fn hint_from_path(path: &Path) -> Hint {
        let mut hint = Hint::new();
        
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            debug!("Setting probe hint extension: {}", extension);
            hint.with_extension(extension);
        } else {
            debug!("No file extension found, probe will auto-detect");
        }
        
        hint
    }

    /// Create a probe hint from MIME type.
    ///
    /// Converts MIME type strings (e.g., "audio/mpeg") to format hints for
    /// Symphonia's probe system.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_playback::FormatDetector;
    ///
    /// let hint = FormatDetector::hint_from_mime_type("audio/mpeg");
    /// // Hint will be configured for MP3 detection
    /// ```
    pub fn hint_from_mime_type(mime_type: &str) -> Hint {
        let mut hint = Hint::new();
        
        debug!("Creating probe hint from MIME type: {}", mime_type);
        hint.mime_type(mime_type);
        
        hint
    }

    /// Detect audio codec from Symphonia codec type.
    ///
    /// Converts Symphonia's internal `CodecType` to our platform-agnostic
    /// `AudioCodec` enum.
    pub fn detect_codec(codec_type: CodecType) -> AudioCodec {
        use symphonia::core::codecs::*;
        
        // Compare against known codec type constants
        if codec_type == CODEC_TYPE_MP3 {
            AudioCodec::Mp3
        } else if codec_type == CODEC_TYPE_AAC {
            AudioCodec::Aac
        } else if codec_type == CODEC_TYPE_FLAC {
            AudioCodec::Flac
        } else if codec_type == CODEC_TYPE_VORBIS {
            AudioCodec::Vorbis
        } else if codec_type == CODEC_TYPE_OPUS {
            AudioCodec::Opus
        } else if codec_type == CODEC_TYPE_ALAC {
            AudioCodec::Alac
        } else if codec_type == CODEC_TYPE_PCM_S16LE
            || codec_type == CODEC_TYPE_PCM_S16BE
            || codec_type == CODEC_TYPE_PCM_S24LE
            || codec_type == CODEC_TYPE_PCM_S24BE
            || codec_type == CODEC_TYPE_PCM_S32LE
            || codec_type == CODEC_TYPE_PCM_S32BE
            || codec_type == CODEC_TYPE_PCM_F32LE
            || codec_type == CODEC_TYPE_PCM_F32BE
            || codec_type == CODEC_TYPE_PCM_F64LE
            || codec_type == CODEC_TYPE_PCM_F64BE
        {
            AudioCodec::Wav
        } else {
            warn!("Unknown codec type: {:?}", codec_type);
            AudioCodec::Unknown
        }
    }

    /// Validate if a codec is supported by current feature flags.
    ///
    /// This checks if the required Symphonia bundle/codec is enabled at
    /// compile time.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Codec is supported
    /// - `Err(PlaybackError::UnsupportedCodec)` - Codec not enabled
    pub fn validate_codec_support(codec: &AudioCodec) -> Result<()> {
        match codec {
            AudioCodec::Mp3 => {
                #[cfg(not(feature = "decoder-mp3"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "MP3 decoder not enabled. Enable 'decoder-mp3' feature".to_string(),
                ));
                Ok(())
            }
            AudioCodec::Flac => {
                #[cfg(not(feature = "decoder-flac"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "FLAC decoder not enabled. Enable 'decoder-flac' feature".to_string(),
                ));
                Ok(())
            }
            AudioCodec::Vorbis => {
                #[cfg(not(feature = "decoder-vorbis"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "Vorbis decoder not enabled. Enable 'decoder-vorbis' feature".to_string(),
                ));
                Ok(())
            }
            AudioCodec::Opus => {
                #[cfg(not(feature = "decoder-opus"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "Opus decoder not enabled. Enable 'decoder-opus' feature".to_string(),
                ));
                #[cfg(feature = "decoder-opus")]
                Ok(())
            }
            AudioCodec::Aac => {
                #[cfg(not(feature = "decoder-aac"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "AAC decoder not enabled. Enable 'decoder-aac' feature".to_string(),
                ));
                #[cfg(feature = "decoder-aac")]
                Ok(())
            }
            AudioCodec::Wav => {
                #[cfg(not(feature = "decoder-wav"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "WAV decoder not enabled. Enable 'decoder-wav' feature".to_string(),
                ));
                #[cfg(feature = "decoder-wav")]
                Ok(())
            }
            AudioCodec::Alac => {
                #[cfg(not(feature = "decoder-alac"))]
                return Err(PlaybackError::UnsupportedCodec(
                    "ALAC decoder not enabled. Enable 'decoder-alac' feature".to_string(),
                ));
                #[cfg(feature = "decoder-alac")]
                Ok(())
            }
            AudioCodec::Unknown => Err(PlaybackError::UnsupportedCodec(
                "Unknown audio codec".to_string(),
            )),
            AudioCodec::Other(name) => Err(PlaybackError::UnsupportedCodec(format!(
                "Unsupported codec: {}",
                name
            ))),
        }
    }

    /// Get the common file extension for a codec.
    pub fn codec_extension(codec: &AudioCodec) -> &'static str {
        match codec {
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Aac => "m4a",
            AudioCodec::Flac => "flac",
            AudioCodec::Vorbis => "ogg",
            AudioCodec::Opus => "opus",
            AudioCodec::Wav => "wav",
            AudioCodec::Alac => "m4a",
            AudioCodec::Unknown => "bin",
            AudioCodec::Other(_) => "bin",
        }
    }

    /// Get the MIME type for a codec.
    pub fn codec_mime_type(codec: &AudioCodec) -> &'static str {
        match codec {
            AudioCodec::Mp3 => "audio/mpeg",
            AudioCodec::Aac => "audio/mp4",
            AudioCodec::Flac => "audio/flac",
            AudioCodec::Vorbis => "audio/ogg",
            AudioCodec::Opus => "audio/opus",
            AudioCodec::Wav => "audio/wav",
            AudioCodec::Alac => "audio/mp4",
            AudioCodec::Unknown => "application/octet-stream",
            AudioCodec::Other(_) => "application/octet-stream",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hint_from_path() {
        let path = Path::new("/music/song.mp3");
        let hint = FormatDetector::hint_from_path(path);
        // Hint is opaque, but should not panic
    }

    #[test]
    fn test_hint_from_mime_type() {
        let hint = FormatDetector::hint_from_mime_type("audio/mpeg");
        // Hint is opaque, but should not panic
    }

    #[test]
    fn test_codec_extension() {
        assert_eq!(FormatDetector::codec_extension(&AudioCodec::Mp3), "mp3");
        assert_eq!(FormatDetector::codec_extension(&AudioCodec::Flac), "flac");
        assert_eq!(FormatDetector::codec_extension(&AudioCodec::Vorbis), "ogg");
        assert_eq!(FormatDetector::codec_extension(&AudioCodec::Wav), "wav");
    }

    #[test]
    fn test_codec_mime_type() {
        assert_eq!(FormatDetector::codec_mime_type(&AudioCodec::Mp3), "audio/mpeg");
        assert_eq!(FormatDetector::codec_mime_type(&AudioCodec::Flac), "audio/flac");
        assert_eq!(FormatDetector::codec_mime_type(&AudioCodec::Vorbis), "audio/ogg");
        assert_eq!(FormatDetector::codec_mime_type(&AudioCodec::Wav), "audio/wav");
    }

    #[test]
    fn test_codec_validation() {
        // Test that validation works (will pass/fail based on features)
        // We can't test specific outcomes as they depend on feature flags
        let _ = FormatDetector::validate_codec_support(&AudioCodec::Mp3);
        let _ = FormatDetector::validate_codec_support(&AudioCodec::Flac);
        
        // Unknown codecs should always fail
        assert!(FormatDetector::validate_codec_support(&AudioCodec::Unknown).is_err());
        assert!(FormatDetector::validate_codec_support(&AudioCodec::Other("custom".to_string())).is_err());
    }
}
