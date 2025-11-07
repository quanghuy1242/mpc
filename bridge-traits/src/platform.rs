//! Platform-specific helper abstractions used to keep trait bounds aligned with
//! the threading guarantees of each target.
//!
//! Native targets require `Send + Sync` to allow bridge implementations to be
//! shared freely across async tasks. WebAssembly builds, however, run entirely
//! on a single thread and cannot satisfy those bounds because browser-provided
//! objects (e.g., `web_sys` types) are not thread-safe. The helper traits below
//! make the required bounds conditional without duplicating every trait
//! definition.

/// Marker trait that applies `Send + Sync` on native targets while becoming a
/// no-op on `wasm32`.
#[cfg(not(target_arch = "wasm32"))]
pub trait PlatformSendSync: Send + Sync {}

#[cfg(not(target_arch = "wasm32"))]
impl<T> PlatformSendSync for T where T: Send + Sync {}

#[cfg(target_arch = "wasm32")]
pub trait PlatformSendSync {}

#[cfg(target_arch = "wasm32")]
impl<T> PlatformSendSync for T {}

/// Marker trait equivalent to `Send` on native targets.
#[cfg(not(target_arch = "wasm32"))]
pub trait PlatformSend: Send {}

#[cfg(not(target_arch = "wasm32"))]
impl<T> PlatformSend for T where T: Send {}

#[cfg(target_arch = "wasm32")]
pub trait PlatformSend {}

#[cfg(target_arch = "wasm32")]
impl<T> PlatformSend for T {}

/// Dynamic async reader type that enforces `Send` when available.
#[cfg(not(target_arch = "wasm32"))]
pub type DynAsyncRead = dyn core_async::io::AsyncRead + Send + Unpin;

#[cfg(target_arch = "wasm32")]
pub type DynAsyncRead = dyn core_async::io::AsyncRead + Unpin;

/// Dynamic async writer type that enforces `Send` when available.
#[cfg(not(target_arch = "wasm32"))]
pub type DynAsyncWrite = dyn core_async::io::AsyncWrite + Send + Unpin;

#[cfg(target_arch = "wasm32")]
pub type DynAsyncWrite = dyn core_async::io::AsyncWrite + Unpin;
