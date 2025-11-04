//! # Core Configuration Module
//!
//! Provides configuration management for the Music Platform Core.
//!
//! ## Overview
//!
//! The configuration system uses a builder pattern to construct a `CoreConfig`
//! instance that holds all necessary dependencies and settings for the core library.
//! It enforces fail-fast validation to ensure all required bridges are provided
//! before initialization.
//!
//! ## Required Dependencies
//!
//! - `SecureStore` - Required for credential persistence
//! - `SettingsStore` - Required for user preferences
//!
//! ## Optional Dependencies (with platform defaults)
//!
//! - `HttpClient` - HTTP operations (desktop default: reqwest)
//! - `FileSystemAccess` - File I/O (desktop default: tokio fs)
//! - `NetworkMonitor` - Connectivity detection (optional)
//! - `BackgroundExecutor` - Task scheduling (optional)
//! - `LifecycleObserver` - App lifecycle (optional)
//!
//! ## Usage
//!
//! ### Basic Configuration with Desktop Defaults
//!
//! ```ignore
//! use core_runtime::config::CoreConfig;
//! use std::sync::Arc;
//!
//! // Note: Requires implementing SecureStore and SettingsStore traits  
//! let config = CoreConfig::builder()
//!     .database_path("/path/to/music.db")
//!     .cache_dir("/path/to/cache")
//!     .secure_store(Arc::new(MySecureStore))
//!     .settings_store(Arc::new(MySettingsStore))
//!     .build()
//!     .expect("Failed to build config");
//! ```

//!
//! ### Configuration with Custom Bridges
//!
//! ```ignore
//! use core_runtime::config::CoreConfig;
//! use std::sync::Arc;
//!
//! // Note: Requires implementing HttpClient, FileSystemAccess, SecureStore, SettingsStore
//! let config = CoreConfig::builder()
//!     .database_path("/path/to/music.db")
//!     .cache_dir("/path/to/cache")
//!     .cache_size_mb(500)
//!     .http_client(Arc::new(MyHttpClient))
//!     .file_system(Arc::new(MyFileSystem))
//!     .secure_store(Arc::new(MySecureStore))
//!     .settings_store(Arc::new(MySettingsStore))
//!     .enable_lyrics(true)
//!     .enable_artwork_remote(true)
//!     .build()
//!     .expect("Failed to build config");
//! ```

//!
//! ## Error Handling
//!
//! The builder validates all required dependencies and provides actionable error
//! messages when capabilities are missing:
//!
//! ```should_panic
//! use core_runtime::config::CoreConfig;
//!
//! // This will panic with an actionable error message
//! let config = CoreConfig::builder()
//!     .database_path("/path/to/music.db")
//!     .build()
//!     .expect("Should fail - missing required bridges");
//! ```

use crate::error::{Error, Result};
use bridge_traits::{
    BackgroundExecutor, FileSystemAccess, HttpClient, LifecycleObserver, NetworkMonitor,
    SecureStore, SettingsStore,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Core configuration for the Music Platform Core.
///
/// This struct holds all dependencies and settings required to initialize
/// the core library. Use [`CoreConfigBuilder`] to construct instances.
#[derive(Clone)]
pub struct CoreConfig {
    /// Path to the SQLite database file
    pub database_path: PathBuf,

    /// Directory for storing cached files (artwork, audio chunks, etc.)
    pub cache_dir: PathBuf,

    /// Maximum cache size in megabytes
    pub cache_size_mb: usize,

    /// HTTP client for making API requests (optional with desktop default)
    pub http_client: Option<Arc<dyn HttpClient>>,

    /// File system access abstraction (optional with desktop default)
    pub file_system: Option<Arc<dyn FileSystemAccess>>,

    /// Secure credential storage (required)
    pub secure_store: Arc<dyn SecureStore>,

    /// User preferences storage (required)
    pub settings_store: Arc<dyn SettingsStore>,

    /// Network connectivity monitor (optional)
    pub network_monitor: Option<Arc<dyn NetworkMonitor>>,

    /// Background task executor (optional)
    pub background_executor: Option<Arc<dyn BackgroundExecutor>>,

    /// App lifecycle observer (optional)
    pub lifecycle_observer: Option<Arc<dyn LifecycleObserver>>,

    /// Features flags
    pub features: FeatureFlags,
}

impl std::fmt::Debug for CoreConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreConfig")
            .field("database_path", &self.database_path)
            .field("cache_dir", &self.cache_dir)
            .field("cache_size_mb", &self.cache_size_mb)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "HttpClient { ... }"),
            )
            .field(
                "file_system",
                &self
                    .file_system
                    .as_ref()
                    .map(|_| "FileSystemAccess { ... }"),
            )
            .field("secure_store", &"SecureStore { ... }")
            .field("settings_store", &"SettingsStore { ... }")
            .field(
                "network_monitor",
                &self
                    .network_monitor
                    .as_ref()
                    .map(|_| "NetworkMonitor { ... }"),
            )
            .field(
                "background_executor",
                &self
                    .background_executor
                    .as_ref()
                    .map(|_| "BackgroundExecutor { ... }"),
            )
            .field(
                "lifecycle_observer",
                &self
                    .lifecycle_observer
                    .as_ref()
                    .map(|_| "LifecycleObserver { ... }"),
            )
            .field("features", &self.features)
            .finish()
    }
}

/// Feature flags control optional functionality.
///
/// Features can be enabled during configuration to unlock additional capabilities,
/// but may require corresponding bridge implementations to function correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    /// Enable lyrics fetching from external providers
    pub enable_lyrics: bool,

    /// Enable remote artwork fetching (MusicBrainz, Last.fm, etc.)
    pub enable_artwork_remote: bool,

    /// Enable encrypted offline cache for downloaded tracks
    pub enable_offline_cache: bool,

    /// Enable background sync jobs (requires BackgroundExecutor)
    pub enable_background_sync: bool,

    /// Enable network-aware operations (requires NetworkMonitor)
    pub enable_network_awareness: bool,
}

impl CoreConfig {
    /// Creates a new builder for constructing a `CoreConfig`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use core_runtime::config::CoreConfig;
    ///
    /// let builder = CoreConfig::builder();
    /// ```
    pub fn builder() -> CoreConfigBuilder {
        CoreConfigBuilder::default()
    }

    /// Validates the configuration and returns an error if invalid.
    ///
    /// This checks:
    /// - Database path is not empty
    /// - Cache directory is not empty
    /// - Cache size is reasonable (> 0 and < 100GB)
    /// - Required bridges are provided
    /// - Feature flags are consistent with available bridges
    pub fn validate(&self) -> Result<()> {
        // Validate paths
        if self.database_path.as_os_str().is_empty() {
            return Err(Error::Config("Database path cannot be empty".to_string()));
        }

        if self.cache_dir.as_os_str().is_empty() {
            return Err(Error::Config("Cache directory cannot be empty".to_string()));
        }

        // Validate cache size (must be > 0 and < 100GB)
        if self.cache_size_mb == 0 {
            return Err(Error::Config(
                "Cache size must be greater than 0 MB".to_string(),
            ));
        }

        if self.cache_size_mb > 100_000 {
            return Err(Error::Config(
                "Cache size exceeds maximum of 100GB (100,000 MB)".to_string(),
            ));
        }

        // Validate feature flags against available bridges
        if self.features.enable_background_sync && self.background_executor.is_none() {
            return Err(Error::Config(
                "Background sync enabled but no BackgroundExecutor provided. \
                 Disable the feature or inject a BackgroundExecutor implementation."
                    .to_string(),
            ));
        }

        if self.features.enable_network_awareness && self.network_monitor.is_none() {
            return Err(Error::Config(
                "Network awareness enabled but no NetworkMonitor provided. \
                 Disable the feature or inject a NetworkMonitor implementation."
                    .to_string(),
            ));
        }

        Ok(())
    }
}

/// Builder for constructing [`CoreConfig`] instances.
///
/// Use this builder to incrementally set configuration options and then
/// call [`build()`](CoreConfigBuilder::build) to create the final config.
/// The builder validates required dependencies and provides helpful error
/// messages.
#[derive(Default)]
pub struct CoreConfigBuilder {
    database_path: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    cache_size_mb: Option<usize>,
    http_client: Option<Arc<dyn HttpClient>>,
    file_system: Option<Arc<dyn FileSystemAccess>>,
    secure_store: Option<Arc<dyn SecureStore>>,
    settings_store: Option<Arc<dyn SettingsStore>>,
    network_monitor: Option<Arc<dyn NetworkMonitor>>,
    background_executor: Option<Arc<dyn BackgroundExecutor>>,
    lifecycle_observer: Option<Arc<dyn LifecycleObserver>>,
    features: FeatureFlags,
}

impl CoreConfigBuilder {
    /// Sets the database path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SQLite database file
    ///
    /// # Examples
    ///
    /// ```
    /// use core_runtime::config::CoreConfig;
    ///
    /// let builder = CoreConfig::builder()
    ///     .database_path("/path/to/music.db");
    /// ```
    pub fn database_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.database_path = Some(path.into());
        self
    }

    /// Sets the cache directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the cache directory
    ///
    /// # Examples
    ///
    /// ```
    /// use core_runtime::config::CoreConfig;
    ///
    /// let builder = CoreConfig::builder()
    ///     .cache_dir("/path/to/cache");
    /// ```
    pub fn cache_dir<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Sets the maximum cache size in megabytes.
    ///
    /// Default: 1024 MB (1 GB)
    ///
    /// # Arguments
    ///
    /// * `size_mb` - Maximum cache size in megabytes
    ///
    /// # Examples
    ///
    /// ```
    /// use core_runtime::config::CoreConfig;
    ///
    /// let builder = CoreConfig::builder()
    ///     .cache_size_mb(2048); // 2 GB
    /// ```
    pub fn cache_size_mb(mut self, size_mb: usize) -> Self {
        self.cache_size_mb = Some(size_mb);
        self
    }

    /// Sets the HTTP client implementation.
    ///
    /// If not provided, the desktop default (reqwest-based) will be used when
    /// the `desktop-shims` feature is enabled.
    ///
    /// # Arguments
    ///
    /// * `client` - HTTP client implementation
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use core_runtime::config::CoreConfig;
    /// use std::sync::Arc;
    /// # use bridge_traits::HttpClient;
    /// # struct MyHttpClient;
    /// # #[async_trait::async_trait]
    /// # impl HttpClient for MyHttpClient {
    /// #     async fn execute(&self, request: bridge_traits::HttpRequest) -> Result<bridge_traits::HttpResponse, bridge_traits::BridgeError> { unimplemented!() }
    /// #     async fn download_stream(&self, url: String) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, bridge_traits::BridgeError> { unimplemented!() }
    /// # }
    ///
    /// let builder = CoreConfig::builder()
    ///     .http_client(Arc::new(MyHttpClient));
    /// ```
    pub fn http_client(mut self, client: Arc<dyn HttpClient>) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Sets the file system access implementation.
    ///
    /// If not provided, the desktop default (tokio fs-based) will be used when
    /// the `desktop-shims` feature is enabled.
    ///
    /// # Arguments
    ///
    /// * `fs` - File system access implementation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use core_runtime::config::CoreConfig;
    /// use std::sync::Arc;
    ///
    /// let builder = CoreConfig::builder()
    ///     .file_system(Arc::new(MyFileSystem));
    /// ```
    pub fn file_system(mut self, fs: Arc<dyn FileSystemAccess>) -> Self {
        self.file_system = Some(fs);
        self
    }

    /// Sets the secure store implementation (required).
    ///
    /// The secure store is used for persisting sensitive credentials like
    /// OAuth tokens. It must provide platform-appropriate security
    /// (Keychain on macOS/iOS, Keystore on Android, etc.).
    ///
    /// # Arguments
    ///
    /// * `store` - Secure store implementation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use core_runtime::config::CoreConfig;
    /// use std::sync::Arc;
    ///
    /// let builder = CoreConfig::builder()
    ///     .secure_store(Arc::new(MySecureStore));
    /// ```
    pub fn secure_store(mut self, store: Arc<dyn SecureStore>) -> Self {
        self.secure_store = Some(store);
        self
    }

    /// Sets the settings store implementation (required).
    ///
    /// The settings store is used for persisting user preferences and
    /// application settings.
    ///
    /// # Arguments
    ///
    /// * `store` - Settings store implementation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use core_runtime::config::CoreConfig;
    /// use std::sync::Arc;
    ///
    /// let builder = CoreConfig::builder()
    ///     .settings_store(Arc::new(MySettingsStore));
    /// ```
    pub fn settings_store(mut self, store: Arc<dyn SettingsStore>) -> Self {
        self.settings_store = Some(store);
        self
    }

    /// Sets the network monitor implementation (optional).
    ///
    /// The network monitor is used to detect connectivity changes and adapt
    /// behavior (e.g., pause sync on metered networks).
    ///
    /// # Arguments
    ///
    /// * `monitor` - Network monitor implementation
    pub fn network_monitor(mut self, monitor: Arc<dyn NetworkMonitor>) -> Self {
        self.network_monitor = Some(monitor);
        self
    }

    /// Sets the background executor implementation (optional).
    ///
    /// The background executor is used to schedule and run background tasks
    /// (e.g., sync jobs, metadata enrichment).
    ///
    /// # Arguments
    ///
    /// * `executor` - Background executor implementation
    pub fn background_executor(mut self, executor: Arc<dyn BackgroundExecutor>) -> Self {
        self.background_executor = Some(executor);
        self
    }

    /// Sets the lifecycle observer implementation (optional).
    ///
    /// The lifecycle observer is used to detect app foreground/background
    /// transitions and adjust behavior accordingly.
    ///
    /// # Arguments
    ///
    /// * `observer` - Lifecycle observer implementation
    pub fn lifecycle_observer(mut self, observer: Arc<dyn LifecycleObserver>) -> Self {
        self.lifecycle_observer = Some(observer);
        self
    }

    /// Enables or disables lyrics fetching.
    ///
    /// Default: false
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable lyrics fetching
    pub fn enable_lyrics(mut self, enabled: bool) -> Self {
        self.features.enable_lyrics = enabled;
        self
    }

    /// Enables or disables remote artwork fetching.
    ///
    /// Default: false
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable remote artwork fetching
    pub fn enable_artwork_remote(mut self, enabled: bool) -> Self {
        self.features.enable_artwork_remote = enabled;
        self
    }

    /// Enables or disables offline cache.
    ///
    /// Default: false
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable offline cache
    pub fn enable_offline_cache(mut self, enabled: bool) -> Self {
        self.features.enable_offline_cache = enabled;
        self
    }

    /// Enables or disables background sync.
    ///
    /// Requires a `BackgroundExecutor` to be provided.
    ///
    /// Default: false
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable background sync
    pub fn enable_background_sync(mut self, enabled: bool) -> Self {
        self.features.enable_background_sync = enabled;
        self
    }

    /// Enables or disables network awareness.
    ///
    /// Requires a `NetworkMonitor` to be provided.
    ///
    /// Default: false
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to enable network awareness
    pub fn enable_network_awareness(mut self, enabled: bool) -> Self {
        self.features.enable_network_awareness = enabled;
        self
    }

    /// Sets all feature flags at once.
    ///
    /// # Arguments
    ///
    /// * `features` - Feature flags to set
    pub fn features(mut self, features: FeatureFlags) -> Self {
        self.features = features;
        self
    }

    /// Builds the final `CoreConfig` instance.
    ///
    /// This validates all required dependencies are provided and returns
    /// an error with an actionable message if anything is missing.
    ///
    /// # Returns
    ///
    /// Returns `Ok(CoreConfig)` on success, or an error if:
    /// - Required bridges are missing (SecureStore, SettingsStore)
    /// - Configuration values are invalid
    /// - Feature flags are inconsistent with available bridges
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use core_runtime::config::CoreConfig;
    /// use std::sync::Arc;
    ///
    /// let config = CoreConfig::builder()
    ///     .database_path("/path/to/music.db")
    ///     .cache_dir("/path/to/cache")
    ///     .secure_store(Arc::new(MySecureStore))
    ///     .settings_store(Arc::new(MySettingsStore))
    ///     .build()?;
    /// # Ok::<(), core_runtime::Error>(())
    /// ```
    pub fn build(self) -> Result<CoreConfig> {
        // Validate required fields
        let database_path = self.database_path.ok_or_else(|| {
            Error::Config("Database path is required. Use .database_path() to set it.".to_string())
        })?;

        let cache_dir = self.cache_dir.ok_or_else(|| {
            Error::Config("Cache directory is required. Use .cache_dir() to set it.".to_string())
        })?;

        let secure_store = self.secure_store.ok_or_else(|| {
            Error::CapabilityMissing {
                capability: "SecureStore".to_string(),
                message: "SecureStore implementation is required for credential persistence. \
                         Desktop: ensure 'desktop-shims' feature is enabled and inject KeyringSecureStore. \
                         Mobile: inject platform-native secure storage (Keychain/Keystore). \
                         Web: inject WebCrypto-based secure storage."
                    .to_string(),
            }
        })?;

        let settings_store = self.settings_store.ok_or_else(|| {
            Error::CapabilityMissing {
                capability: "SettingsStore".to_string(),
                message: "SettingsStore implementation is required for user preferences. \
                         Desktop: ensure 'desktop-shims' feature is enabled and inject SqliteSettingsStore. \
                         Mobile: inject platform-native settings (UserDefaults/DataStore). \
                         Web: inject localStorage-based settings store."
                    .to_string(),
            }
        })?;

        // Create config with defaults
        let config = CoreConfig {
            database_path,
            cache_dir,
            cache_size_mb: self.cache_size_mb.unwrap_or(1024), // Default 1 GB
            http_client: self.http_client,
            file_system: self.file_system,
            secure_store,
            settings_store,
            network_monitor: self.network_monitor,
            background_executor: self.background_executor,
            lifecycle_observer: self.lifecycle_observer,
            features: self.features,
        };

        // Validate the configuration
        config.validate()?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bridge_traits::storage::SettingsTransaction;
    use bridge_traits::{BridgeError, SecureStore, SettingsStore};

    // Mock implementations for testing
    struct MockSecureStore;

    #[async_trait]
    impl SecureStore for MockSecureStore {
        async fn set_secret(
            &self,
            _key: &str,
            _value: &[u8],
        ) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn get_secret(
            &self,
            _key: &str,
        ) -> std::result::Result<Option<Vec<u8>>, BridgeError> {
            Ok(None)
        }

        async fn delete_secret(&self, _key: &str) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn list_keys(&self) -> std::result::Result<Vec<String>, BridgeError> {
            Ok(Vec::new())
        }

        async fn clear_all(&self) -> std::result::Result<(), BridgeError> {
            Ok(())
        }
    }

    struct MockSettingsStore;

    #[async_trait]
    impl SettingsStore for MockSettingsStore {
        async fn set_string(
            &self,
            _key: &str,
            _value: &str,
        ) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn get_string(&self, _key: &str) -> std::result::Result<Option<String>, BridgeError> {
            Ok(None)
        }

        async fn set_bool(&self, _key: &str, _value: bool) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn get_bool(&self, _key: &str) -> std::result::Result<Option<bool>, BridgeError> {
            Ok(None)
        }

        async fn set_i64(&self, _key: &str, _value: i64) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn get_i64(&self, _key: &str) -> std::result::Result<Option<i64>, BridgeError> {
            Ok(None)
        }

        async fn set_f64(&self, _key: &str, _value: f64) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn get_f64(&self, _key: &str) -> std::result::Result<Option<f64>, BridgeError> {
            Ok(None)
        }

        async fn delete(&self, _key: &str) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn has_key(&self, _key: &str) -> std::result::Result<bool, BridgeError> {
            Ok(false)
        }

        async fn list_keys(&self) -> std::result::Result<Vec<String>, BridgeError> {
            Ok(Vec::new())
        }

        async fn clear_all(&self) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn begin_transaction(
            &self,
        ) -> std::result::Result<Box<dyn SettingsTransaction + Send>, BridgeError> {
            Ok(Box::new(MockTransaction))
        }
    }

    struct MockTransaction;

    #[async_trait]
    impl SettingsTransaction for MockTransaction {
        async fn set_string(
            &mut self,
            _key: &str,
            _value: &str,
        ) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn commit(self: Box<Self>) -> std::result::Result<(), BridgeError> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> std::result::Result<(), BridgeError> {
            Ok(())
        }
    }

    #[test]
    fn test_builder_requires_database_path() {
        let result = CoreConfig::builder()
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Database path is required"));
    }

    #[test]
    fn test_builder_requires_cache_dir() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cache directory is required"));
    }

    #[test]
    fn test_builder_requires_secure_store() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("SecureStore"));
        assert!(err_msg.contains("credential persistence"));
    }

    #[test]
    fn test_builder_requires_settings_store() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .build();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("SettingsStore"));
        assert!(err_msg.contains("user preferences"));
    }

    #[test]
    fn test_builder_with_all_required_fields() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.database_path, PathBuf::from("/db/music.db"));
        assert_eq!(config.cache_dir, PathBuf::from("/cache"));
        assert_eq!(config.cache_size_mb, 1024); // Default
    }

    #[test]
    fn test_builder_with_custom_cache_size() {
        let config = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .cache_size_mb(2048)
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build()
            .unwrap();

        assert_eq!(config.cache_size_mb, 2048);
    }

    #[test]
    fn test_validate_rejects_zero_cache_size() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .cache_size_mb(0)
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
    }

    #[test]
    fn test_validate_rejects_excessive_cache_size() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .cache_size_mb(200_000) // 200 GB
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_feature_flags_default() {
        let flags = FeatureFlags::default();
        assert!(!flags.enable_lyrics);
        assert!(!flags.enable_artwork_remote);
        assert!(!flags.enable_offline_cache);
        assert!(!flags.enable_background_sync);
        assert!(!flags.enable_network_awareness);
    }

    #[test]
    fn test_builder_with_feature_flags() {
        let config = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .enable_lyrics(true)
            .enable_artwork_remote(true)
            .build()
            .unwrap();

        assert!(config.features.enable_lyrics);
        assert!(config.features.enable_artwork_remote);
        assert!(!config.features.enable_offline_cache);
    }

    #[test]
    fn test_validate_background_sync_requires_executor() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .enable_background_sync(true)
            .build();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Background sync enabled"));
        assert!(err_msg.contains("BackgroundExecutor"));
    }

    #[test]
    fn test_validate_network_awareness_requires_monitor() {
        let result = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .enable_network_awareness(true)
            .build();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Network awareness enabled"));
        assert!(err_msg.contains("NetworkMonitor"));
    }

    #[test]
    fn test_builder_accepts_pathbuf() {
        let config = CoreConfig::builder()
            .database_path(PathBuf::from("/db/music.db"))
            .cache_dir(PathBuf::from("/cache"))
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build()
            .unwrap();

        assert_eq!(config.database_path, PathBuf::from("/db/music.db"));
    }

    #[test]
    fn test_builder_accepts_str() {
        let config = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build()
            .unwrap();

        assert_eq!(config.database_path, PathBuf::from("/db/music.db"));
    }

    #[test]
    fn test_config_is_cloneable() {
        let config = CoreConfig::builder()
            .database_path("/db/music.db")
            .cache_dir("/cache")
            .secure_store(Arc::new(MockSecureStore))
            .settings_store(Arc::new(MockSettingsStore))
            .build()
            .unwrap();

        let cloned = config.clone();
        assert_eq!(cloned.database_path, config.database_path);
        assert_eq!(cloned.cache_size_mb, config.cache_size_mb);
    }
}
