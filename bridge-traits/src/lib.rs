//! # Host Bridge Traits
//!
//! Platform abstraction traits that must be implemented by each host platform.
//!
//! ## Overview
//!
//! This crate defines the contract between the core library and platform-specific
//! implementations. Each trait represents a capability that the core requires but
//! that must be implemented differently per platform (desktop, iOS, Android, web).
//!
//! ## Traits
//!
//! ### Networking & I/O
//! - [`HttpClient`](http::HttpClient) - Async HTTP operations with OAuth, retry, TLS
//! - [`FileSystemAccess`](storage::FileSystemAccess) - File I/O, caching, offline storage
//!
//! ### Security & Storage
//! - [`SecureStore`](storage::SecureStore) - Credential persistence (Keychain/Keystore)
//! - [`SettingsStore`](storage::SettingsStore) - Key-value preferences storage
//!
//! ### Platform Integration
//! - [`NetworkMonitor`](network::NetworkMonitor) - Connectivity and metered network detection
//! - [`BackgroundExecutor`](background::BackgroundExecutor) - Task scheduling respecting platform constraints
//! - [`LifecycleObserver`](background::LifecycleObserver) - App foreground/background transitions
//!
//! ### Utilities
//! - [`Clock`](time::Clock) - Time source for deterministic testing
//! - [`LoggerSink`](time::LoggerSink) - Forward structured logs to host logging
//!
//! ## Platform Requirements
//!
//! Each supported platform must ship concrete adapters for every required bridge trait:
//!
//! | Platform | Implementation Crate | Status |
//! |----------|---------------------|--------|
//! | Desktop  | `bridge-desktop`    | ✅ In Progress |
//! | iOS      | TBD                 | 📋 Planned |
//! | Android  | TBD                 | 📋 Planned |
//! | Web      | TBD                 | 📋 Planned |
//!
//! ## Fail-Fast Strategy
//!
//! The core should fail fast with descriptive errors when a required capability is missing:
//!
//! ```ignore
//! use core_runtime::error::CoreError;
//!
//! pub fn new(config: CoreConfig) -> Result<Self> {
//!     let http_client = config.http_client
//!         .ok_or_else(|| CoreError::CapabilityMissing {
//!             capability: "HttpClient".to_string(),
//!             message: "No HTTP client implementation provided. \
//!                      Desktop: ensure default feature is enabled. \
//!                      Mobile: inject platform-native adapter.".to_string()
//!         })?;
//!     // ...
//! }
//! ```
//!
//! ## Error Handling
//!
//! All bridge traits use the [`BridgeError`](error::BridgeError) type for consistent
//! error handling. Platform implementations should:
//!
//! - Convert platform-specific errors to `BridgeError`
//! - Provide actionable error messages
//! - Include error context (e.g., file paths, network status)
//!
//! ## Thread Safety
//!
//! All bridge traits require `Send + Sync` bounds to support safe concurrent usage
//! across async tasks. Implementations must ensure thread safety.
//!
//! ## Examples
//!
//! ### Implementing HttpClient
//!
//! ```ignore
//! use bridge_traits::http::{HttpClient, HttpRequest, HttpResponse};
//! use bridge_traits::error::Result;
//! use async_trait::async_trait;
//!
//! pub struct MyHttpClient {
//!     client: reqwest::Client,
//! }
//!
//! #[async_trait]
//! impl HttpClient for MyHttpClient {
//!     async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
//!         // Implementation
//!         todo!()
//!     }
//!     
//!     async fn download_stream(&self, url: String) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
//!         // Implementation
//!         todo!()
//!     }
//! }
//! ```

pub mod background;
pub mod error;
pub mod http;
pub mod network;
pub mod storage;
pub mod time;

pub use error::BridgeError;

// Re-export commonly used types
pub use background::{BackgroundExecutor, LifecycleObserver, LifecycleState, TaskConstraints};
pub use http::{HttpClient, HttpMethod, HttpRequest, HttpResponse};
pub use network::{NetworkInfo, NetworkMonitor, NetworkStatus, NetworkType};
pub use storage::{FileSystemAccess, RemoteFile, SecureStore, SettingsStore, StorageProvider};
pub use time::{Clock, LogEntry, LogLevel, LoggerSink, SystemClock};
