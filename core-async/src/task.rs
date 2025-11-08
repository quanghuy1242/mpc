//! Task spawning and execution abstractions.
//!
//! This module provides platform-agnostic task spawning capabilities:
//! - On native platforms: Uses `tokio::task::spawn` and `spawn_blocking`
//! - On WASM: Uses single-threaded executor with awaitable `JoinHandle`
//!
//! # Platform Differences
//!
//! ## Native (Tokio)
//! - `spawn`: Returns a `JoinHandle<T>` that can be awaited
//! - `spawn_blocking`: For CPU-intensive operations, uses thread pool
//! - Tasks can be `Send` and moved between threads
//!
//! ## WASM
//! - `spawn`: Returns an awaitable `JoinHandle<T>` (now with full parity!)
//! - `spawn_blocking`: Not available (panics with helpful error)
//! - Tasks must be `'static` but not `Send` (single-threaded)
//!
//! # Examples
//!
//! ```rust
//! use core_async::task;
//!
//! async fn example() {
//!     // Spawn an async task - works on both native and WASM!
//!     let handle = task::spawn(async {
//!         // Do async work
//!         42
//!     });
//!     
//!     // Can await the handle on both platforms
//!     let result = handle.await.unwrap();
//!     assert_eq!(result, 42);
//!     
//!     // Blocking work only on native
//!     #[cfg(not(target_arch = "wasm32"))]
//!     task::spawn_blocking(|| {
//!         // CPU-intensive work
//!     });
//! }
//! ```

// ============================================================================
// Native Implementation (Tokio)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::task::{spawn_blocking, yield_now, JoinError, JoinHandle};

#[cfg(not(target_arch = "wasm32"))]
/// Spawns a new asynchronous task using the Tokio runtime.
///
/// This function spawns a new task that runs concurrently with other tasks.
/// The spawned task may run on a different thread.
///
/// # Arguments
///
/// * `future` - The async computation to run
///
/// # Returns
///
/// A `JoinHandle` that can be awaited to get the task's result.
///
/// # Examples
///
/// ```rust
/// use core_async::task::spawn;
///
/// # async fn example() {
/// let handle = spawn(async {
///     // Do async work
///     42
/// });
///
/// let result = handle.await.unwrap();
/// assert_eq!(result, 42);
/// # }
/// ```
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::task::spawn(future)
}

// ============================================================================
// WASM Implementation - Now with JoinHandle parity!
// ============================================================================

#[cfg(target_arch = "wasm32")]
pub use crate::wasm::task::{spawn, spawn_blocking, yield_now, JoinError, JoinHandle};

// ============================================================================
// Common Types
// ============================================================================

/// Result type for task operations.
pub type Result<T> = std::result::Result<T, JoinError>;
