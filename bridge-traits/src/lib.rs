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
//! - `HttpClient` - Async HTTP operations with OAuth, retry, TLS
//! - `FileSystemAccess` - File I/O, caching, offline storage
//! - `SecureStore` - Credential persistence (Keychain/Keystore)
//! - `SettingsStore` - Key-value preferences storage
//! - `NetworkMonitor` - Connectivity and metered network detection
//! - `BackgroundExecutor` - Task scheduling respecting platform constraints
//! - `LifecycleObserver` - App foreground/background transitions
//! - `Clock` - Time source for deterministic testing
//! - `LoggerSink` - Forward structured logs to host logging

pub mod error;
pub mod http;
pub mod storage;

pub use error::BridgeError;

// Placeholder modules for TASK-002
