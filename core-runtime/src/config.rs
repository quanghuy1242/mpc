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
//! When the `desktop-shims` feature is enabled, desktop-ready defaults for
//! `SecureStore` and `SettingsStore` are injected automatically if not provided.
//!
//! ## Usage
//!
//! ### Basic Configuration with Desktop Defaults
//!
//! ```ignore
//! use core_runtime::config::CoreConfig;
//!
//! let config = CoreConfig::builder()
//!     .database_path("/path/to/music.db")
//!     .cache_dir("/path/to/cache")
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
use std::path::{Path, PathBuf};
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

    /// External metadata API configuration (MusicBrainz, Last.fm)
    pub metadata_api_config: MetadataApiConfig,
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

/// Configuration for external metadata API services.
///
/// Provides API keys and optional configuration for remote metadata services like
/// MusicBrainz Cover Art Archive and Last.fm. These services are used for:
/// - Fetching album artwork when not embedded in audio files
/// - Enriching metadata with additional information
///
/// # Security Note
///
/// API keys should never be hardcoded in the binary. They should be:
/// - Loaded from environment variables
/// - Stored in secure configuration files
/// - Injected via the host platform's secure configuration system
///
/// # Example
///
/// ```no_run
/// use core_runtime::config::MetadataApiConfig;
///
/// let config = MetadataApiConfig {
///     musicbrainz_user_agent: Some("MyMusicApp/1.0 (contact@example.com)".to_string()),
///     lastfm_api_key: Some("your_lastfm_api_key".to_string()),
///     rate_limit_delay_ms: 1000, // 1 request per second
/// };
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataApiConfig {
    /// MusicBrainz user agent string (format: "AppName/Version (Contact)")
    ///
    /// MusicBrainz requires a user agent that identifies your application.
    /// Format: "ApplicationName/Version (ContactEmail)"
    /// Example: "MyMusicApp/1.0 (contact@example.com)"
    ///
    /// This is required for accessing the MusicBrainz API.
    /// See: https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting
    pub musicbrainz_user_agent: Option<String>,

    /// Last.fm API key for album.getInfo and artwork fetching
    ///
    /// Obtain an API key from: https://www.last.fm/api/account/create
    /// This is optional - if not provided, Last.fm fetching will be disabled.
    pub lastfm_api_key: Option<String>,

    /// Rate limit delay in milliseconds between API requests
    ///
    /// Default: 1000ms (1 request per second)
    /// MusicBrainz recommends 1 request/second for anonymous clients.
    /// Last.fm rate limits are higher but we apply the same for safety.
    pub rate_limit_delay_ms: u64,
}

impl MetadataApiConfig {
    /// Creates a new MetadataApiConfig with no API keys configured
    pub fn new() -> Self {
        Self {
            musicbrainz_user_agent: None,
            lastfm_api_key: None,
            rate_limit_delay_ms: 1000,
        }
    }

    /// Sets the MusicBrainz user agent
    pub fn with_musicbrainz_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.musicbrainz_user_agent = Some(user_agent.into());
        self
    }

    /// Sets the Last.fm API key
    pub fn with_lastfm_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.lastfm_api_key = Some(api_key.into());
        self
    }

    /// Sets the rate limit delay in milliseconds
    pub fn with_rate_limit_delay_ms(mut self, delay_ms: u64) -> Self {
        self.rate_limit_delay_ms = delay_ms;
        self
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate MusicBrainz user agent format if provided
        if let Some(ref ua) = self.musicbrainz_user_agent {
            if ua.is_empty() {
                return Err(Error::Config(
                    "MusicBrainz user agent cannot be empty".to_string(),
                ));
            }
            if !ua.contains('/') || !ua.contains('(') || !ua.contains(')') {
                return Err(Error::Config(
                    "MusicBrainz user agent must follow format: 'AppName/Version (Contact)'"
                        .to_string(),
                ));
            }
        }

        // Validate rate limit
        if self.rate_limit_delay_ms == 0 {
            return Err(Error::Config(
                "Rate limit delay must be greater than 0ms".to_string(),
            ));
        }

        if self.rate_limit_delay_ms > 60000 {
            return Err(Error::Config(
                "Rate limit delay exceeds maximum of 60 seconds (60,000ms)".to_string(),
            ));
        }

        Ok(())
    }

    /// Checks if MusicBrainz is configured
    pub fn has_musicbrainz(&self) -> bool {
        self.musicbrainz_user_agent.is_some()
    }

    /// Checks if Last.fm is configured
    pub fn has_lastfm(&self) -> bool {
        self.lastfm_api_key.is_some()
    }
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

#[cfg(not(feature = "desktop-shims"))]
fn secure_store_missing_error() -> Error {
    Error::CapabilityMissing {
        capability: "SecureStore".to_string(),
        message: "SecureStore implementation is required for credential persistence. \
                 Desktop: ensure the 'desktop-shims' feature is enabled to use the default KeyringSecureStore. \
                 Mobile: inject platform-native secure storage (Keychain/Keystore). \
                 Web: inject WebCrypto-based secure storage."
            .to_string(),
    }
}

#[cfg(not(feature = "desktop-shims"))]
fn settings_store_missing_error() -> Error {
    Error::CapabilityMissing {
        capability: "SettingsStore".to_string(),
        message: "SettingsStore implementation is required for user preferences. \
                 Desktop: ensure the 'desktop-shims' feature is enabled to use the default SqliteSettingsStore. \
                 Mobile: inject platform-native settings (UserDefaults/DataStore). \
                 Web: inject localStorage-based settings store."
            .to_string(),
    }
}

#[cfg(feature = "desktop-shims")]
fn provide_default_secure_store() -> Result<Arc<dyn SecureStore>> {
    use bridge_desktop::KeyringSecureStore;

    let store: Arc<dyn SecureStore> = Arc::new(KeyringSecureStore::new());
    Ok(store)
}

#[cfg(not(feature = "desktop-shims"))]
fn provide_default_secure_store() -> Result<Arc<dyn SecureStore>> {
    Err(secure_store_missing_error())
}

#[cfg(feature = "desktop-shims")]
fn provide_default_settings_store(
    database_path: &Path,
    cache_dir: &Path,
) -> Result<Arc<dyn SettingsStore>> {
    use bridge_desktop::SqliteSettingsStore;
    use std::thread;
    use tokio::runtime::{Handle, Runtime};

    let candidate = database_path
        .parent()
        .map(|parent| parent.join("settings.db"))
        .unwrap_or_else(|| cache_dir.join("settings.db"));

    let init_store = |path: PathBuf| -> Result<_> {
        let runtime = Runtime::new().map_err(|e| {
            Error::Internal(format!(
                "Failed to create Tokio runtime for default settings store: {}",
                e
            ))
        })?;

        runtime
            .block_on(SqliteSettingsStore::new(path))
            .map_err(|e| {
                Error::Internal(format!("Failed to initialize default SettingsStore: {}", e))
            })
    };

    let store = match Handle::try_current() {
        Ok(_) => {
            let path = candidate.clone();
            thread::spawn(move || init_store(path))
                .join()
                .map_err(|_| {
                    Error::Internal(
                        "Tokio worker thread panicked while creating default SettingsStore"
                            .to_string(),
                    )
                })??
        }
        Err(_) => init_store(candidate.clone())?,
    };

    let store: Arc<dyn SettingsStore> = Arc::new(store);
    Ok(store)
}

#[cfg(not(feature = "desktop-shims"))]
fn provide_default_settings_store(
    _database_path: &Path,
    _cache_dir: &Path,
) -> Result<Arc<dyn SettingsStore>> {
    Err(settings_store_missing_error())
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
    metadata_api_config: Option<MetadataApiConfig>,
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

    /// Sets the metadata API configuration for remote artwork and metadata fetching.
    ///
    /// This configuration includes API keys and settings for MusicBrainz and Last.fm.
    ///
    /// # Arguments
    ///
    /// * `config` - Metadata API configuration
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use core_runtime::config::{CoreConfig, MetadataApiConfig};
    ///
    /// let api_config = MetadataApiConfig::new()
    ///     .with_musicbrainz_user_agent("MyApp/1.0 (contact@example.com)")
    ///     .with_lastfm_api_key("your_api_key");
    ///
    /// let builder = CoreConfig::builder()
    ///     .metadata_api_config(api_config);
    /// ```
    pub fn metadata_api_config(mut self, config: MetadataApiConfig) -> Self {
        self.metadata_api_config = Some(config);
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
    /// let config = CoreConfig::builder()
    ///     .database_path("/path/to/music.db")
    ///     .cache_dir("/path/to/cache")
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

        let secure_store = match self.secure_store {
            Some(store) => store,
            None => provide_default_secure_store()?,
        };

        let settings_store = match self.settings_store {
            Some(store) => store,
            None => provide_default_settings_store(&database_path, &cache_dir)?,
        };

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
            metadata_api_config: self.metadata_api_config.unwrap_or_default(),
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
    use std::sync::Arc;

    #[cfg(feature = "desktop-shims")]
    use tokio::runtime::Runtime;
    #[cfg(feature = "desktop-shims")]
    use uuid::Uuid;

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

    #[cfg(feature = "desktop-shims")]
    fn desktop_test_paths() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("core-runtime-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let db_path = base.join("music.db");
        let cache_dir = base.join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        (base, db_path, cache_dir)
    }

    #[cfg(feature = "desktop-shims")]
    #[test]
    fn test_build_with_desktop_defaults() {
        let (base_dir, db_path, cache_dir) = desktop_test_paths();

        let config = CoreConfig::builder()
            .database_path(&db_path)
            .cache_dir(&cache_dir)
            .build()
            .expect("desktop defaults should succeed");

        let settings = config.settings_store.clone();
        let rt = Runtime::new().expect("runtime");
        rt.block_on(async {
            settings.set_string("theme", "dark").await.unwrap();
            let value = settings.get_string("theme").await.unwrap();
            assert_eq!(value.as_deref(), Some("dark"));
        });

        drop(config);
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[cfg(feature = "desktop-shims")]
    #[tokio::test]
    // TODO(#TASK-005): Re-enable once desktop keyring support is available in CI
    #[ignore = "TODO: Enable once desktop environment is available"]
    async fn test_build_with_desktop_defaults_inside_runtime() {
        let (base_dir, db_path, cache_dir) = desktop_test_paths();

        let config = CoreConfig::builder()
            .database_path(&db_path)
            .cache_dir(&cache_dir)
            .build()
            .expect("desktop defaults should succeed inside runtime");

        {
            let settings = config.settings_store.clone();
            settings
                .set_string("in-runtime", "ok")
                .await
                .expect("settings write inside runtime");
            let value = settings
                .get_string("in-runtime")
                .await
                .expect("settings read inside runtime");
            assert_eq!(value.as_deref(), Some("ok"));
        }

        drop(config);
        let _ = tokio::fs::remove_dir_all(&base_dir).await;
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
