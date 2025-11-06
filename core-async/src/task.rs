//! Task spawning and execution abstractions.
//!
//! This module provides platform-agnostic task spawning capabilities:
//! - On native platforms: Uses `tokio::task::spawn` and `spawn_blocking`
//! - On WASM: Uses `wasm_bindgen_futures::spawn_local`
//!
//! # Platform Differences
//!
//! ## Native (Tokio)
//! - `spawn`: Returns a `JoinHandle<T>` that can be awaited
//! - `spawn_blocking`: For CPU-intensive operations, uses thread pool
//! - Tasks can be `Send` and moved between threads
//!
//! ## WASM
//! - `spawn`: Returns `()`, tasks run on the main thread
//! - `spawn_blocking`: Not available (panics if called)
//! - Tasks must be `'static` but not `Send` (single-threaded)
//!
//! # Examples
//!
//! ```rust
//! use core_async::task;
//!
//! async fn example() {
//!     // Spawn an async task
//!     task::spawn(async {
//!         // Do async work
//!     });
//!     
//!     // On native, spawn blocking work (not available on WASM)
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
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
/// Spawns a new asynchronous task in the browser's event loop.
///
/// This function spawns a task using `wasm_bindgen_futures::spawn_local`.
/// The task runs on the main thread and cannot return a value directly.
///
/// # Arguments
///
/// * `future` - The async computation to run
///
/// # Platform Notes
///
/// On WASM, there is no way to retrieve the task's result or join on it.
/// If you need the result, use the future directly instead of spawning it.
///
/// # Examples
///
/// ```rust
/// use core_async::task::spawn;
///
/// # async fn example() {
/// spawn(async {
///     // Do async work
///     // Result is discarded
/// });
/// # }
/// ```
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(target_arch = "wasm32")]
/// Spawns a blocking task.
///
/// # Panics
///
/// This function always panics on WASM because there is no thread pool available.
/// Blocking operations are not supported in the browser environment.
///
/// # Platform Notes
///
/// On WASM, all operations must be non-blocking and async. If you need to perform
/// CPU-intensive work, consider:
/// - Breaking it into smaller chunks with `yield_now()` calls
/// - Using Web Workers (requires additional setup)
/// - Offloading to a server-side API
pub fn spawn_blocking<F, R>(_f: F) -> !
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    panic!(
        "spawn_blocking is not supported on WASM. \
         Use async operations or Web Workers instead."
    );
}

#[cfg(target_arch = "wasm32")]
/// Yields control back to the event loop.
///
/// This is useful for preventing long-running tasks from blocking the UI.
///
/// # Examples
///
/// ```rust
/// use core_async::task::yield_now;
///
/// # async fn example() {
/// for i in 0..1000000 {
///     // Do some work
///     if i % 1000 == 0 {
///         yield_now().await; // Let other tasks run
///     }
/// }
/// # }
/// ```
pub async fn yield_now() {
    // In WASM, we can yield by creating a microtask
    let (tx, rx) = futures::channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tx.send(());
    });
    let _ = rx.await;
}

// ============================================================================
// Common Types
// ============================================================================

/// Result type for task operations.
///
/// On native platforms, this wraps `tokio::task::JoinError`.
/// On WASM, tasks cannot fail (they panic instead), so this is mostly unused.
#[cfg(not(target_arch = "wasm32"))]
pub type Result<T> = std::result::Result<T, JoinError>;

#[cfg(target_arch = "wasm32")]
pub type Result<T> = std::result::Result<T, ()>;
