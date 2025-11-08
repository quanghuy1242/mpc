//! Async filesystem helpers re-exported from the underlying runtime.
//!
//! On native targets this simply re-exports Tokio's `fs` module. On WASM targets
//! this provides a thin adapter around the `FileSystemAccess` trait that uses
//! IndexedDB for storage. The APIs are intentionally kept similar so downstream
//! crates can rely on a familiar surface area without depending on Tokio directly.
//!
//! # WASM Limitations
//!
//! On WASM, the following operations are not supported:
//! - `copy`, `rename` - Use read + write + delete pattern instead
//! - `hard_link`, `read_link` - IndexedDB doesn't support links
//! - File permissions - No-op on WASM
//!
//! # WASM Initialization
//!
//! The WASM filesystem must be initialized during application bootstrap:
//!
//! ```no_run
//! # #[cfg(target_arch = "wasm32")]
//! # async fn bootstrap() {
//! use core_async::fs;
//! use bridge_wasm::WasmFileSystem;
//! use std::sync::Arc;
//!
//! let wasm_fs = WasmFileSystem::new("my-app").await.unwrap();
//! unsafe {
//!     fs::init_filesystem(Arc::new(wasm_fs));
//! }
//! # }
//! ```

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::fs::{
    self, copy, create_dir, create_dir_all, hard_link, metadata, read, read_dir, read_link,
    read_to_string, remove_dir, remove_dir_all, remove_file, rename, set_permissions,
    symlink_metadata, write, DirBuilder, DirEntry, File, OpenOptions,
};

#[cfg(target_arch = "wasm32")]
pub use crate::wasm::fs::{
    copy, create_dir, create_dir_all, hard_link, metadata, read, read_dir, read_link,
    read_to_string, remove_dir, remove_dir_all, remove_file, rename, set_permissions,
    symlink_metadata, write, BridgeFileMetadata, DirBuilder, DirEntry, File, FileMetadata,
    OpenOptions, ReadDir, WasmFileSystemOps,
};

#[cfg(target_arch = "wasm32")]
pub use crate::wasm::fs::init_filesystem;
