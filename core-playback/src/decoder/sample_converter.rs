//! # Sample Format Converter
//!
//! Converts audio samples between different formats and layouts.

use crate::error::Result;
use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal};
use symphonia::core::conv::IntoSample;
use symphonia::core::sample::Sample;
use tracing::warn;

/// Sample converter that normalizes audio to f32 interleaved format.
///
/// Symphonia outputs audio in various formats (i16, i24, i32, f32, f64)
/// and layouts (planar, interleaved). This converter normalizes everything
/// to interleaved f32 samples in the range [-1.0, 1.0].
pub struct SampleConverter;

impl SampleConverter {
    /// Convert Symphonia AudioBufferRef to interleaved f32 samples.
    ///
    /// This is the main conversion function that handles all sample formats
    /// and layouts. The output is always:
    /// - Format: f32
    /// - Range: [-1.0, 1.0]
    /// - Layout: Interleaved (LRLRLR... for stereo)
    ///
    /// # Arguments
    ///
    /// * `buffer` - Symphonia audio buffer (any format)
    ///
    /// # Returns
    ///
    /// Vector of interleaved f32 samples normalized to [-1.0, 1.0]
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let decoded = decoder.decode(&packet)?;
    /// let samples = SampleConverter::to_interleaved_f32(&decoded)?;
    /// // samples is now Vec<f32> with interleaved channels
    /// ```
    pub fn to_interleaved_f32(buffer: &AudioBufferRef<'_>) -> Result<Vec<f32>> {
        match buffer {
            AudioBufferRef::F32(buf) => {
                // Already f32, just interleave if needed
                Ok(Self::interleave_f32_planes(&**buf))
            }
            AudioBufferRef::F64(buf) => {
                // Convert from f64 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: f64| sample.into_sample(),
                ))
            }
            AudioBufferRef::S32(buf) => {
                // Convert from i32 to f32 (normalize to [-1.0, 1.0])
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: i32| sample.into_sample(),
                ))
            }
            AudioBufferRef::S16(buf) => {
                // Convert from i16 to f32 (normalize to [-1.0, 1.0])
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: i16| sample.into_sample(),
                ))
            }
            AudioBufferRef::S24(buf) => {
                // Convert from i24 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample| IntoSample::into_sample(sample),
                ))
            }
            AudioBufferRef::S8(buf) => {
                // Convert from i8 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: i8| sample.into_sample(),
                ))
            }
            AudioBufferRef::U32(buf) => {
                // Convert from u32 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: u32| sample.into_sample(),
                ))
            }
            AudioBufferRef::U16(buf) => {
                // Convert from u16 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: u16| sample.into_sample(),
                ))
            }
            AudioBufferRef::U24(buf) => {
                // Convert from u24 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample| IntoSample::into_sample(sample),
                ))
            }
            AudioBufferRef::U8(buf) => {
                // Convert from u8 to f32
                Ok(Self::convert_and_interleave(
                    &**buf,
                    |sample: u8| sample.into_sample(),
                ))
            }
        }
    }

    /// Interleave f32 planar audio buffer.
    ///
    /// Converts from planar format (LLLL...RRRR...) to interleaved (LRLRLR...).
    fn interleave_f32_planes(buf: &AudioBuffer<f32>) -> Vec<f32> {
        let num_channels = buf.spec().channels.count();
        let num_frames = buf.frames();
        let mut interleaved = Vec::with_capacity(num_frames * num_channels);

        // Interleave samples from all channels
        for frame_idx in 0..num_frames {
            for chan_idx in 0..num_channels {
                let plane = buf.chan(chan_idx);
                interleaved.push(plane[frame_idx]);
            }
        }

        interleaved
    }

    /// Convert and interleave samples of any type.
    ///
    /// Generic conversion function that handles any sample type that implements
    /// `IntoSample<f32>`.
    fn convert_and_interleave<T>(buf: &AudioBuffer<T>, convert: fn(T) -> f32) -> Vec<f32>
    where
        T: Sample + Copy,
    {
        let num_channels = buf.spec().channels.count();
        let num_frames = buf.frames();
        let mut interleaved = Vec::with_capacity(num_frames * num_channels);

        // Convert and interleave samples from all channels
        for frame_idx in 0..num_frames {
            for chan_idx in 0..num_channels {
                let plane = buf.chan(chan_idx);
                let sample = plane[frame_idx];
                interleaved.push(convert(sample));
            }
        }

        interleaved
    }

    /// Validate that samples are in the expected range.
    ///
    /// Checks that all samples are in the range [-1.0, 1.0] and warns if
    /// clipping is detected.
    ///
    /// # Arguments
    ///
    /// * `samples` - Interleaved f32 samples to validate
    ///
    /// # Returns
    ///
    /// Number of clipped samples (0 if all valid)
    pub fn validate_samples(samples: &[f32]) -> usize {
        let clipped = samples
            .iter()
            .filter(|&&s| s < -1.0 || s > 1.0)
            .count();

        if clipped > 0 {
            warn!(
                "Detected {} clipped samples ({:.2}% of total)",
                clipped,
                (clipped as f64 / samples.len() as f64) * 100.0
            );
        }

        clipped
    }

    /// Clamp samples to the valid range [-1.0, 1.0].
    ///
    /// This should rarely be needed as Symphonia's `IntoSample` trait handles
    /// normalization correctly, but can be used as a safety measure.
    pub fn clamp_samples(samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_samples_all_valid() {
        let samples = vec![0.0, 0.5, -0.5, 1.0, -1.0];
        let clipped = SampleConverter::validate_samples(&samples);
        assert_eq!(clipped, 0);
    }

    #[test]
    fn test_validate_samples_with_clipping() {
        let samples = vec![0.0, 1.5, -1.5, 0.5];
        let clipped = SampleConverter::validate_samples(&samples);
        assert_eq!(clipped, 2);
    }

    #[test]
    fn test_clamp_samples() {
        let mut samples = vec![0.0, 1.5, -1.5, 0.5, -0.5];
        SampleConverter::clamp_samples(&mut samples);
        
        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[1], 1.0);   // Clamped from 1.5
        assert_eq!(samples[2], -1.0);  // Clamped from -1.5
        assert_eq!(samples[3], 0.5);
        assert_eq!(samples[4], -0.5);
    }
}
