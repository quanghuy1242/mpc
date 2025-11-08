//! Convenience helpers for wiring all wasm bridge implementations together.
//!
//! Host shells can use [`build_wasm_bridges`] to construct fully initialized
//! adapters for filesystem, database, HTTP, secure storage, and settings
//! without writing repetitive glue code. The result mirrors the role that the
//! `bridge-desktop` crate plays for native targets, giving wasm builds a single
//! entry point for assembling bridge trait objects.

use std::{rc::Rc, sync::Arc};

use bridge_traits::{
    database::{DatabaseAdapter, DatabaseConfig},
    error::{BridgeError, Result as BridgeResult},
    http::HttpClient,
    storage::{FileSystemAccess, SecureStore, SettingsStore},
};

use crate::{
    filesystem::WasmFileSystem, fs_adapter::WasmFileSystemAdapter, http::WasmHttpClient,
    storage::WasmSecureStore, storage::WasmSettingsStore, WasmDbAdapter,
};

/// Configuration for [`build_wasm_bridges`].
#[derive(Debug, Clone)]
pub struct WasmBridgeConfig {
    /// Logical namespace used for storage buckets (filesystem, localStorage).
    pub namespace: String,
    /// Database configuration forwarded to `WasmDbAdapter`.
    pub database: DatabaseConfig,
}

impl WasmBridgeConfig {
    /// Create a new config using the provided namespace.
    ///
    /// The namespace is used for `WasmFileSystem` buckets and secure storage
    /// key prefixes. The database defaults to the in-memory configuration and
    /// can be overridden via [`Self::with_database`].
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            database: DatabaseConfig::in_memory(),
        }
    }

    /// Override the database configuration.
    pub fn with_database(mut self, database: DatabaseConfig) -> Self {
        self.database = database;
        self
    }
}

impl Default for WasmBridgeConfig {
    fn default() -> Self {
        Self::new("mpc")
    }
}

/// Fully constructed wasm bridge objects ready for injection into the core.
pub struct WasmBridgeSet {
    /// HTTP client powered by browser `fetch`.
    pub http_client: Arc<dyn HttpClient>,
    /// IndexedDB-backed filesystem implementation.
    pub filesystem: Arc<dyn FileSystemAccess>,
    /// JavaScript-hosted database adapter (sql.js/IndexedDB combo).
    pub database: Arc<dyn DatabaseAdapter>,
    /// AES-GCM secure credential store.
    pub secure_store: Arc<dyn SecureStore>,
    /// Plain settings store layered on `localStorage`.
    pub settings_store: Arc<dyn SettingsStore>,
}

impl WasmBridgeSet {
    /// Convenience accessor to clone the HTTP client.
    pub fn http(&self) -> Arc<dyn HttpClient> {
        Arc::clone(&self.http_client)
    }

    /// Convenience accessor to clone the filesystem bridge.
    pub fn filesystem(&self) -> Arc<dyn FileSystemAccess> {
        Arc::clone(&self.filesystem)
    }

    /// Convenience accessor to clone the database adapter.
    pub fn database(&self) -> Arc<dyn DatabaseAdapter> {
        Arc::clone(&self.database)
    }

    /// Convenience accessor to clone the secure store.
    pub fn secure_store(&self) -> Arc<dyn SecureStore> {
        Arc::clone(&self.secure_store)
    }

    /// Convenience accessor to clone the settings store.
    pub fn settings_store(&self) -> Arc<dyn SettingsStore> {
        Arc::clone(&self.settings_store)
    }
}

/// Build the default wasm bridge stack.
///
/// Hosts should call this during startup (e.g., inside their wasm bindgen
/// bootstrap) and pass the returned trait objects into `core-service` or other
/// core crates.
pub async fn build_wasm_bridges(config: WasmBridgeConfig) -> BridgeResult<WasmBridgeSet> {
    let http_client: Arc<dyn HttpClient> = Arc::new(WasmHttpClient::new()?);
    
    // Create two filesystem instances that share the same underlying IndexedDB
    // This is acceptable because WasmFileSystem uses Arc<IdbDatabase> internally,
    // so both instances reference the same database. WASM is single-threaded anyway.
    
    // Instance 1: For core_async::fs (requires Rc)
    let wasm_fs_for_core = WasmFileSystem::new(&config.namespace)
        .await
        .map_err(BridgeError::from)?;
    let fs_adapter = Rc::new(WasmFileSystemAdapter::new(Rc::new(wasm_fs_for_core)));
    unsafe {
        core_async::fs::init_filesystem(fs_adapter);
    }
    
    // Instance 2: For bridge traits (requires Arc)
    let filesystem: Arc<dyn FileSystemAccess> = Arc::new(
        WasmFileSystem::new(&config.namespace)
            .await
            .map_err(BridgeError::from)?,
    );
    
    let database: Arc<dyn DatabaseAdapter> = Arc::new(
        WasmDbAdapter::new(config.database.clone())
            .await
            .map_err(BridgeError::from)?,
    );
    let secure_store: Arc<dyn SecureStore> = Arc::new(WasmSecureStore::new(&config.namespace)?);
    let settings_store: Arc<dyn SettingsStore> =
        Arc::new(WasmSettingsStore::new(&config.namespace)?);

    Ok(WasmBridgeSet {
        http_client,
        filesystem,
        database,
        secure_store,
        settings_store,
    })
}
