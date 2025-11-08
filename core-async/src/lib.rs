//! Runtime-agnostic async abstraction layer for Music Platform Core.
//!
//! This crate provides a unified async API that works across different runtime environments:
//! - Native platforms (desktop): Uses Tokio runtime
//! - WebAssembly: Uses browser's event loop with wasm-bindgen-futures
//!
//! # Architecture
//!
//! The crate uses conditional compilation (`#[cfg]`) to provide platform-specific
//! implementations while maintaining a consistent API surface. All core-* and provider-*
//! crates should depend on this crate instead of directly depending on tokio.
//!
//! # Modules
//!
//! - `task`: Task spawning and execution
//! - `time`: Time-related operations (sleep, duration, instant)
//! - `sync`: Synchronization primitives (Mutex, RwLock, channels)
//!
//! # Examples
//!
//! ```rust
//! use core_async::task;
//! use core_async::time::{sleep, Duration};
//!
//! async fn example() {
//!     // Spawn a concurrent task
//!     let handle = task::spawn(async {
//!         sleep(Duration::from_secs(1)).await;
//!         42
//!     });
//!     
//!     // On native, this returns tokio::task::JoinHandle
//!     // On WASM, spawns via wasm_bindgen_futures::spawn_local
//! }
//! ```

// Re-export the async entry-point/test macros so downstream crates never need
// direct Tokio dependencies.
pub use core_async_macros::{main, test};

#[cfg(target_arch = "wasm32")]
pub mod test_support {
    pub use wasm_bindgen_test::wasm_bindgen_test;
}

// Core modules
pub mod fs;
pub mod io;
pub mod runtime;
pub mod sync;
pub mod task;
pub mod time;

// WASM-specific implementations
#[cfg(target_arch = "wasm32")]
mod wasm;

// Re-export commonly used types at crate root for convenience
pub use task::spawn;
pub use time::{sleep, Duration, Instant};
