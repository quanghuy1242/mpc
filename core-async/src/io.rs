//! Async I/O traits and utilities.
//!
//! This module provides runtime-agnostic async I/O traits that work across
//! native and WASM platforms.
//!
//! # Platform Support
//!
//! - **Native**: Re-exports tokio's I/O traits and utilities
//! - **WASM**: Currently uses futures-io traits as tokio I/O is not available
//!
//! # Examples
//!
//! ```rust
//! use core_async::io::{AsyncRead, AsyncWrite};
//! use std::pin::Pin;
//! use std::task::{Context, Poll};
//!
//! async fn read_data<R: AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
//!     let mut buffer = Vec::new();
//!     // Use AsyncReadExt methods
//!     Ok(buffer)
//! }
//! ```

// =============================================================================
// Native Implementation (Tokio-based)
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt, BufReader, BufWriter, ReadBuf,
};

// =============================================================================
// WASM Implementation (futures-io based)
// =============================================================================

#[cfg(target_arch = "wasm32")]
pub use futures::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt, BufReader, BufWriter,
};

// Note: ReadBuf is tokio-specific and doesn't have a direct equivalent in futures-io.
// For WASM, code that uses ReadBuf should be refactored to use standard buffer slices.
