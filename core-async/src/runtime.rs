//! Runtime utilities that abstract over the underlying async executor.
//!
//! On native targets we wrap Tokio's runtime primitives so that downstream
//! crates never need to depend on Tokio directly. For WebAssembly targets we
//! currently provide lightweight stubs and helpers that rely on the browser
//! event loop.

use std::future::Future;

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::runtime::{Builder, Handle, Runtime};

/// Runs the provided future to completion using a lightweight runtime.
#[cfg(not(target_arch = "wasm32"))]
pub fn block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("core_async::runtime::block_on: failed to build Tokio runtime")
        .block_on(future)
}

/// Runs the provided future to completion on WebAssembly.
///
/// We cannot synchronously block the browser event loop, so the future is
/// scheduled using `spawn_local` and the function always returns immediately.
#[cfg(target_arch = "wasm32")]
pub fn block_on<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(async move {
        future.await;
    });
}

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_futures::spawn_local;
