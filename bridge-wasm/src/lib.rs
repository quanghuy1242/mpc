//! WebAssembly Bridge Implementations
//!
//! This crate provides WebAssembly-compatible implementations of the bridge traits
//! defined in `bridge-traits`. These implementations use browser APIs through
//! `web-sys` and `wasm-bindgen` to provide functionality equivalent to native
//! implementations.
//!
//! # Platform Support
//!
//! This crate is designed exclusively for the `wasm32-unknown-unknown` target.
//! It will not compile for native targets.
//!
//! # Implementations
//!
//! - `WasmFileSystem`: IndexedDB-based file system simulation
//! - `WasmDbAdapter`: WebAssembly-compatible database bridge (delegates to host-provided sql.js/IndexedDB runtime)
//! - More implementations to come (HTTP, storage, network, etc.)
//!
//! # Examples
//!
//! ```ignore
//! use bridge_wasm::filesystem::WasmFileSystem;
//! use bridge_traits::storage::FileSystemAccess;
//!
//! #[wasm_bindgen_test]
//! async fn test_file_operations() {
//!     let fs = WasmFileSystem::new("my-app").await.unwrap();
//!     let cache_dir = fs.get_cache_directory().await.unwrap();
//!     // ... use file system
//! }
//! ```

#![cfg(target_arch = "wasm32")]
// We allow unsafe_code only in bootstrap for core_async::fs::init_filesystem
#![warn(missing_docs)]

pub mod bootstrap;
pub mod database;
pub mod error;
pub mod filesystem;
pub mod fs_adapter;
pub mod http;
pub mod storage;

// Re-export commonly used types
pub use bootstrap::{build_wasm_bridges, WasmBridgeConfig, WasmBridgeSet};
pub use database::WasmDbAdapter;
pub use error::{WasmError, WasmResult};
pub use filesystem::WasmFileSystem;
pub use fs_adapter::WasmFileSystemAdapter;
pub use http::WasmHttpClient;
pub use storage::{WasmSecureStore, WasmSettingsStore};
