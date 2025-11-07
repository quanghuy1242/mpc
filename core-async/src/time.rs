//! Time-related abstractions.
//!
//! This module provides platform-agnostic time operations:
//! - On native platforms: Uses `tokio::time`
//! - On WASM: Uses `gloo-timers` and standard library types
//!
//! # Platform Differences
//!
//! ## Native (Tokio)
//! - High-precision timing with efficient sleep implementation
//! - `Instant` is monotonic and suitable for measuring elapsed time
//! - `sleep` integrates with Tokio's timer wheel
//!
//! ## WASM
//! - Uses browser's `setTimeout` for sleep operations
//! - `Instant` wraps `web_sys::Performance` for high-precision timing
//! - Timing precision depends on browser implementation
//!
//! # Examples
//!
//! ```rust
//! use core_async::time::{sleep, Duration, Instant};
//!
//! async fn example() {
//!     let start = Instant::now();
//!     
//!     sleep(Duration::from_secs(1)).await;
//!     
//!     let elapsed = start.elapsed();
//!     println!("Took {:?}", elapsed);
//! }
//! ```

// ============================================================================
// Native Implementation (Tokio)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::time::{interval, sleep, sleep_until, timeout, Interval, Sleep, Timeout};

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
pub use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
use std::pin::Pin;

#[cfg(target_arch = "wasm32")]
/// Sleeps for the specified duration.
///
/// This function uses the browser's `setTimeout` API internally.
///
/// # Arguments
///
/// * `duration` - How long to sleep
///
/// # Examples
///
/// ```rust
/// use core_async::time::{sleep, Duration};
///
/// # async fn example() {
/// sleep(Duration::from_millis(100)).await;
/// # }
/// ```
pub async fn sleep(duration: Duration) {
    gloo_timers::future::sleep(duration).await
}

#[cfg(target_arch = "wasm32")]
/// A monotonic instant for measuring elapsed time.
///
/// On WASM, this uses `performance.now()` for high-precision timing.
/// The instant is relative to an arbitrary point in time (usually page load).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    millis: u64,
}

#[cfg(target_arch = "wasm32")]
impl Instant {
    /// Returns the current instant.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::time::Instant;
    ///
    /// let now = Instant::now();
    /// ```
    pub fn now() -> Self {
        let window = web_sys::window().expect("no global window");
        let performance = window.performance().expect("performance API not available");
        let millis = performance.now() as u64;
        Self { millis }
    }

    /// Returns the amount of time elapsed since this instant.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::time::Instant;
    ///
    /// let start = Instant::now();
    /// // ... do work ...
    /// let elapsed = start.elapsed();
    /// ```
    pub fn elapsed(&self) -> Duration {
        Self::now().duration_since(*self)
    }

    /// Returns the duration since an earlier instant.
    ///
    /// # Panics
    ///
    /// Panics if `earlier` is after `self`.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        let millis = self
            .millis
            .checked_sub(earlier.millis)
            .expect("supplied instant is later than self");
        Duration::from_millis(millis)
    }

    /// Returns the duration since an earlier instant, or `None` if the
    /// supplied instant is later than `self`.
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.millis
            .checked_sub(earlier.millis)
            .map(Duration::from_millis)
    }

    /// Returns a new instant that is `duration` later than this instant.
    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.millis
            .checked_add(duration.as_millis() as u64)
            .map(|millis| Instant { millis })
    }

    /// Returns a new instant that is `duration` earlier than this instant.
    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.millis
            .checked_sub(duration.as_millis() as u64)
            .map(|millis| Instant { millis })
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, duration: Duration) -> Self::Output {
        self.checked_add(duration)
            .expect("overflow when adding duration to instant")
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, duration: Duration) -> Self::Output {
        self.checked_sub(duration)
            .expect("overflow when subtracting duration from instant")
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, other: Instant) -> Self::Output {
        self.duration_since(other)
    }
}

#[cfg(target_arch = "wasm32")]
/// Requires a future to complete before the specified duration has elapsed.
///
/// If the future completes before the duration, its result is returned.
/// Otherwise, an error is returned.
///
/// # Examples
///
/// ```rust
/// use core_async::time::{timeout, Duration};
///
/// # async fn example() -> Result<(), ()> {
/// let result = timeout(Duration::from_secs(1), async {
///     // Some operation
///     42
/// }).await;
///
/// match result {
///     Ok(value) => println!("Got value: {}", value),
///     Err(_) => println!("Timed out"),
/// }
/// # Ok(())
/// # }
/// ```
pub async fn timeout<F>(duration: Duration, future: F) -> Result<F::Output, TimeoutError>
where
    F: std::future::Future,
{
    let sleep_fut = sleep(duration);

    futures::pin_mut!(future);
    futures::pin_mut!(sleep_fut);

    match futures::future::select(future, sleep_fut).await {
        futures::future::Either::Left((output, _)) => Ok(output),
        futures::future::Either::Right(_) => Err(TimeoutError),
    }
}

#[cfg(target_arch = "wasm32")]
/// Error returned when a timeout expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

#[cfg(target_arch = "wasm32")]
impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

#[cfg(target_arch = "wasm32")]
impl std::error::Error for TimeoutError {}

#[cfg(target_arch = "wasm32")]
/// A stream that yields at fixed intervals.
///
/// This is a simplified version for WASM that doesn't support all features
/// of tokio's `Interval`.
pub struct Interval {
    duration: Duration,
    next_tick: Option<Pin<Box<dyn std::future::Future<Output = ()>>>>,
}

#[cfg(target_arch = "wasm32")]
impl Interval {
    /// Creates a new interval that yields every `duration`.
    pub fn new(duration: Duration) -> Self {
        let mut interval = Self {
            duration,
            next_tick: None,
        };
        // Start the first tick
        interval.reset();
        interval
    }

    /// Resets the interval to start a new tick.
    fn reset(&mut self) {
        let duration = self.duration;
        self.next_tick = Some(Box::pin(sleep(duration)));
    }

    /// Waits for the next tick.
    pub async fn tick(&mut self) {
        if let Some(fut) = self.next_tick.take() {
            fut.await;
            self.reset();
        }
    }
}

#[cfg(target_arch = "wasm32")]
/// Creates a new interval that yields every `duration`.
///
/// # Examples
///
/// ```rust
/// use core_async::time::{interval, Duration};
///
/// # async fn example() {
/// let mut interval = interval(Duration::from_secs(1));
///
/// loop {
///     interval.tick().await;
///     println!("Tick!");
/// }
/// # }
/// ```
pub fn interval(duration: Duration) -> Interval {
    Interval::new(duration)
}

// ============================================================================
// Common Utilities
// ============================================================================

/// Returns the current time as milliseconds since UNIX_EPOCH.
///
/// This is a convenience function that works on both native and WASM.
///
/// # Examples
///
/// ```rust
/// use core_async::time::now_millis;
///
/// let timestamp = now_millis();
/// ```
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_millis() as u64
}

/// Returns the current time as seconds since UNIX_EPOCH.
///
/// This is a convenience function that works on both native and WASM.
///
/// # Examples
///
/// ```rust
/// use core_async::time::now_secs;
///
/// let timestamp = now_secs();
/// ```
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_secs()
}
