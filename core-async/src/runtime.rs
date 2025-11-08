//! Runtime utilities that abstract over the underlying async executor.
//!
//! On native targets we wrap Tokio's runtime primitives so that downstream
//! crates never need to depend on Tokio directly. For WebAssembly targets we
//! provide a LocalPool-based implementation that actually waits for completion.

// ============================================================================
// Native Implementation (Tokio)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::runtime::{Builder, Handle, Runtime};

/// Runs the provided future to completion using a lightweight runtime.
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("core_async::runtime::block_on: failed to build Tokio runtime")
        .block_on(future)
}

// ============================================================================
// WASM Implementation - Now actually blocks until completion!
// ============================================================================

#[cfg(target_arch = "wasm32")]
pub use crate::wasm::runtime::block_on;

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_futures::spawn_local;
