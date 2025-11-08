//! WASM-specific async runtime implementations.
//!
//! This module provides single-threaded async runtime implementations for WebAssembly
//! that match the API surface of the native Tokio-based implementations. The goal is to
//! provide feature parity so that downstream code can use the same APIs regardless of target.
//!
//! # Key differences from native:
//!
//! - All operations are single-threaded (no `Send` or `Sync` requirements)
//! - `spawn` returns an awaitable `JoinHandle` that stores the result
//! - `block_on` actually waits for completion using a local executor
//! - `spawn_blocking` is not available (panics with helpful error message)
//!
//! # Architecture:
//!
//! - Uses `futures::executor::LocalPool` for running futures to completion
//! - Uses `futures::channel::oneshot` for storing task results
//! - Uses `wasm_bindgen_futures::spawn_local` for fire-and-forget tasks
//! - Maintains a thread-local executor for `block_on` and spawned tasks

pub mod barrier;
pub mod cancellation_token;
pub mod fs;
pub mod notify;
pub mod runtime;
pub mod semaphore;
pub mod task;
pub mod watch;

// Re-export commonly used types
pub use barrier::{Barrier, BarrierWaitResult};
pub use cancellation_token::CancellationToken;
pub use notify::Notify;
pub use semaphore::{Semaphore, SemaphorePermit};

