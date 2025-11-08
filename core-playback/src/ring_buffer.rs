//! # Ring Buffer for PCM Audio Samples
//!
//! Provides a lock-free (on native) or mutex-based (on WASM) circular buffer
//! for passing PCM samples between the decoder (producer) and playback adapter (consumer).
//!
//! ## Design
//!
//! - **Native**: Uses atomic operations for lock-free read/write
//! - **WASM**: Uses Rc<RefCell<>> for single-threaded access
//! - **Capacity**: Fixed size determined at creation
//! - **Overwrite Policy**: Old samples are overwritten when buffer is full
//!
//! ## Usage
//!
//! ```rust
//! use core_playback::ring_buffer::RingBuffer;
//!
//! // Create a buffer for 1 second of stereo audio at 44.1kHz
//! let buffer = RingBuffer::new(44100 * 2);
//!
//! // Producer: Write samples
//! let samples = vec![0.1f32, -0.1, 0.2, -0.2];
//! buffer.write(&samples);
//!
//! // Consumer: Read samples
//! let mut output = vec![0.0f32; 1024];
//! let read = buffer.read(&mut output);
//! ```

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

// ============================================================================
// Native Implementation (Lock-Free with Atomics)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct RingBuffer {
    inner: Arc<RingBufferInner>,
}

#[cfg(not(target_arch = "wasm32"))]
struct RingBufferInner {
    buffer: parking_lot::Mutex<Vec<f32>>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

#[cfg(not(target_arch = "wasm32"))]
impl RingBuffer {
    /// Create a new ring buffer with the specified capacity in samples.
    ///
    /// For stereo audio at 44.1 kHz with 1 second buffer: `capacity = 44100 * 2`
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RingBufferInner {
                buffer: parking_lot::Mutex::new(vec![0.0; capacity]),
                capacity,
                write_pos: AtomicUsize::new(0),
                read_pos: AtomicUsize::new(0),
            }),
        }
    }

    /// Write samples to the ring buffer.
    ///
    /// Returns the number of samples actually written. If the buffer is full,
    /// old samples will be overwritten.
    pub fn write(&self, samples: &[f32]) -> usize {
        if samples.is_empty() {
            return 0;
        }

        let mut buffer = self.inner.buffer.lock();
        let write_pos = self.inner.write_pos.load(Ordering::Acquire);
        let mut written = 0;

        for &sample in samples {
            let pos = (write_pos + written) % self.inner.capacity;
            buffer[pos] = sample;
            written += 1;
        }

        self.inner
            .write_pos
            .store((write_pos + written) % self.inner.capacity, Ordering::Release);

        written
    }

    /// Read samples from the ring buffer.
    ///
    /// Fills `output` with as many samples as available, up to `output.len()`.
    /// Returns the number of samples actually read.
    pub fn read(&self, output: &mut [f32]) -> usize {
        if output.is_empty() {
            return 0;
        }

        let buffer = self.inner.buffer.lock();
        let read_pos = self.inner.read_pos.load(Ordering::Acquire);
        let write_pos = self.inner.write_pos.load(Ordering::Acquire);

        let available = self.available_samples_internal(read_pos, write_pos);
        let to_read = available.min(output.len());

        for i in 0..to_read {
            let pos = (read_pos + i) % self.inner.capacity;
            output[i] = buffer[pos];
        }

        self.inner
            .read_pos
            .store((read_pos + to_read) % self.inner.capacity, Ordering::Release);

        to_read
    }

    /// Returns the number of samples currently available to read.
    pub fn available(&self) -> usize {
        let read_pos = self.inner.read_pos.load(Ordering::Acquire);
        let write_pos = self.inner.write_pos.load(Ordering::Acquire);
        self.available_samples_internal(read_pos, write_pos)
    }

    fn available_samples_internal(&self, read_pos: usize, write_pos: usize) -> usize {
        if write_pos >= read_pos {
            write_pos - read_pos
        } else {
            self.inner.capacity - read_pos + write_pos
        }
    }

    /// Returns the number of samples that can be written before overwriting.
    pub fn free_space(&self) -> usize {
        self.inner.capacity - self.available()
    }

    /// Returns the total capacity of the buffer in samples.
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    /// Returns the buffer fill percentage (0.0 to 1.0).
    pub fn fill_level(&self) -> f32 {
        self.available() as f32 / self.inner.capacity as f32
    }

    /// Clear all samples from the buffer.
    pub fn clear(&self) {
        let mut buffer = self.inner.buffer.lock();
        buffer.fill(0.0);
        self.inner.write_pos.store(0, Ordering::Release);
        self.inner.read_pos.store(0, Ordering::Release);
    }

    /// Returns `true` if the buffer has no samples available.
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Returns `true` if the buffer is full.
    pub fn is_full(&self) -> bool {
        self.available() >= self.inner.capacity - 1
    }
}

// ============================================================================
// WASM Implementation (Single-Threaded with RefCell)
// ============================================================================

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct RingBuffer {
    inner: Rc<RefCell<RingBufferState>>,
}

#[cfg(target_arch = "wasm32")]
struct RingBufferState {
    buffer: Vec<f32>,
    capacity: usize,
    write_pos: usize,
    read_pos: usize,
}

#[cfg(target_arch = "wasm32")]
impl RingBuffer {
    /// Create a new ring buffer with the specified capacity in samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RingBufferState {
                buffer: vec![0.0; capacity],
                capacity,
                write_pos: 0,
                read_pos: 0,
            })),
        }
    }

    /// Write samples to the ring buffer.
    pub fn write(&self, samples: &[f32]) -> usize {
        if samples.is_empty() {
            return 0;
        }

        let mut state = self.inner.borrow_mut();
        let mut written = 0;

        for &sample in samples {
            let pos = (state.write_pos + written) % state.capacity;
            state.buffer[pos] = sample;
            written += 1;
        }

        state.write_pos = (state.write_pos + written) % state.capacity;
        written
    }

    /// Read samples from the ring buffer.
    pub fn read(&self, output: &mut [f32]) -> usize {
        if output.is_empty() {
            return 0;
        }

        let mut state = self.inner.borrow_mut();
        let available = self.available_samples_internal(&state);
        let to_read = available.min(output.len());

        for i in 0..to_read {
            let pos = (state.read_pos + i) % state.capacity;
            output[i] = state.buffer[pos];
        }

        state.read_pos = (state.read_pos + to_read) % state.capacity;
        to_read
    }

    /// Returns the number of samples currently available to read.
    pub fn available(&self) -> usize {
        let state = self.inner.borrow();
        self.available_samples_internal(&state)
    }

    fn available_samples_internal(&self, state: &RingBufferState) -> usize {
        if state.write_pos >= state.read_pos {
            state.write_pos - state.read_pos
        } else {
            state.capacity - state.read_pos + state.write_pos
        }
    }

    /// Returns the number of samples that can be written before overwriting.
    pub fn free_space(&self) -> usize {
        self.capacity() - self.available()
    }

    /// Returns the total capacity of the buffer in samples.
    pub fn capacity(&self) -> usize {
        self.inner.borrow().capacity
    }

    /// Returns the buffer fill percentage (0.0 to 1.0).
    pub fn fill_level(&self) -> f32 {
        self.available() as f32 / self.capacity() as f32
    }

    /// Clear all samples from the buffer.
    pub fn clear(&self) {
        let mut state = self.inner.borrow_mut();
        state.buffer.fill(0.0);
        state.write_pos = 0;
        state.read_pos = 0;
    }

    /// Returns `true` if the buffer has no samples available.
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Returns `true` if the buffer is full.
    pub fn is_full(&self) -> bool {
        self.available() >= self.capacity() - 1
    }
}

// ============================================================================
// Common Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_creation() {
        let buffer = RingBuffer::new(1024);
        assert_eq!(buffer.capacity(), 1024);
        assert_eq!(buffer.available(), 0);
        assert!(buffer.is_empty());
        assert!(!buffer.is_full());
    }

    #[test]
    fn test_ring_buffer_write_read() {
        let buffer = RingBuffer::new(1024);

        // Write samples
        let samples = vec![0.1, 0.2, 0.3, 0.4];
        let written = buffer.write(&samples);
        assert_eq!(written, 4);
        assert_eq!(buffer.available(), 4);

        // Read samples
        let mut output = vec![0.0; 4];
        let read = buffer.read(&mut output);
        assert_eq!(read, 4);
        assert_eq!(output, samples);
        assert_eq!(buffer.available(), 0);
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let buffer = RingBuffer::new(8);

        // Fill buffer
        let samples1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        buffer.write(&samples1);

        // Read half
        let mut output = vec![0.0; 4];
        buffer.read(&mut output);
        assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0]);

        // Write more (should wrap)
        let samples2 = vec![9.0, 10.0, 11.0, 12.0];
        buffer.write(&samples2);

        // Read remaining
        let mut output = vec![0.0; 8];
        let read = buffer.read(&mut output);
        assert_eq!(read, 8);
        assert_eq!(&output[..8], &[5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0]);
    }

    #[test]
    fn test_ring_buffer_overwrite() {
        let buffer = RingBuffer::new(4);

        // Write more than capacity
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        buffer.write(&samples);

        // Latest samples should be in buffer
        let mut output = vec![0.0; 4];
        let read = buffer.read(&mut output);
        assert!(read <= 4);
    }

    #[test]
    fn test_ring_buffer_partial_read() {
        let buffer = RingBuffer::new(1024);

        // Write 10 samples
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        buffer.write(&samples);

        // Read only 5 samples
        let mut output = vec![0.0; 5];
        let read = buffer.read(&mut output);
        assert_eq!(read, 5);
        assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(buffer.available(), 5);
    }

    #[test]
    fn test_ring_buffer_fill_level() {
        let buffer = RingBuffer::new(100);

        let samples = vec![1.0; 50];
        buffer.write(&samples);

        let fill = buffer.fill_level();
        assert!((fill - 0.5).abs() < 0.01); // ~50%
    }

    #[test]
    fn test_ring_buffer_clear() {
        let buffer = RingBuffer::new(1024);

        let samples = vec![1.0, 2.0, 3.0, 4.0];
        buffer.write(&samples);
        assert_eq!(buffer.available(), 4);

        buffer.clear();
        assert_eq!(buffer.available(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_ring_buffer_free_space() {
        let buffer = RingBuffer::new(100);

        let samples = vec![1.0; 30];
        buffer.write(&samples);

        let free = buffer.free_space();
        assert_eq!(free, 70);
    }
}
