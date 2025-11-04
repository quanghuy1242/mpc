//! # Desktop Bridge Implementations
//!
//! Default implementations of bridge traits for desktop platforms
//! (macOS, Windows, Linux).
//!
//! ## Overview
//!
//! This crate provides production-ready implementations of all bridge traits
//! using desktop-appropriate libraries:
//! - `HttpClient` using `reqwest`
//! - `FileSystemAccess` using `std::fs` and `tokio::fs`
//! - `SecureStore` using `keyring` crate
//! - `SettingsStore` using SQLite-backed key-value store
//! - `NetworkMonitor` using platform-specific network APIs
//! - `BackgroundExecutor` using Tokio thread pool
//! - `LifecycleObserver` as no-op (desktop always foreground)
//!
//! ## Feature Flags
//!
//! - `secure-store`: Enable OS keychain integration (default)
//!
//! ## Usage
//!
//! ```ignore
//! use bridge_desktop::{ReqwestHttpClient, TokioFileSystem};
//! use bridge_traits::{HttpClient, FileSystemAccess};
//!
//! #[tokio::main]
//! async fn main() {
//!     let http_client = ReqwestHttpClient::new();
//!     let fs = TokioFileSystem::new();
//!     
//!     // Use in core configuration
//! }
//! ```

mod background;
mod filesystem;
mod http;
mod network;
mod settings;

#[cfg(feature = "secure-store")]
mod secure_store;

pub use background::{DesktopLifecycleObserver, TokioBackgroundExecutor};
pub use filesystem::TokioFileSystem;
pub use http::ReqwestHttpClient;
pub use network::DesktopNetworkMonitor;
pub use settings::SqliteSettingsStore;

#[cfg(feature = "secure-store")]
pub use secure_store::KeyringSecureStore;
