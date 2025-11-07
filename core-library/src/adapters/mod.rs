//! Database adapter implementations
//!
//! This module contains concrete implementations of the `DatabaseAdapter` trait
//! for different platforms.

#[cfg(not(target_arch = "wasm32"))]
pub mod sqlite_native;

#[cfg(not(target_arch = "wasm32"))]
pub use sqlite_native::SqliteAdapter;
