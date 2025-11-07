//! Core service façade and bootstrap helpers.
//!
//! This crate wires host-provided bridge implementations (HTTP, filesystem,
//! database, secure storage, settings) into the shared Rust core. Desktop apps
//! typically enable the `desktop-shims` feature (which depends on
//! `bridge-desktop`), whereas WebAssembly builds enable the `wasm` feature and
//! rely on the adapters from `bridge-wasm`.

pub mod error;

pub use error::{CoreError, Result};

use std::sync::Arc;

use bridge_traits::{
    database::DatabaseAdapter,
    http::HttpClient,
    storage::{FileSystemAccess, SecureStore, SettingsStore},
};

#[cfg(feature = "wasm")]
pub use bridge_wasm::WasmBridgeConfig;
#[cfg(feature = "wasm")]
use bridge_wasm::{build_wasm_bridges, WasmBridgeSet};

/// Aggregated handle to all bridge dependencies the core requires.
pub struct CoreDependencies {
    pub http_client: Arc<dyn HttpClient>,
    pub filesystem: Arc<dyn FileSystemAccess>,
    pub database: Arc<dyn DatabaseAdapter>,
    pub secure_store: Arc<dyn SecureStore>,
    pub settings_store: Arc<dyn SettingsStore>,
}

impl CoreDependencies {
    /// Construct a dependency bundle from explicit bridge handles.
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        filesystem: Arc<dyn FileSystemAccess>,
        database: Arc<dyn DatabaseAdapter>,
        secure_store: Arc<dyn SecureStore>,
        settings_store: Arc<dyn SettingsStore>,
    ) -> Self {
        Self {
            http_client,
            filesystem,
            database,
            secure_store,
            settings_store,
        }
    }
}

#[cfg(feature = "wasm")]
impl From<WasmBridgeSet> for CoreDependencies {
    fn from(set: WasmBridgeSet) -> Self {
        Self {
            http_client: set.http_client,
            filesystem: set.filesystem,
            database: set.database,
            secure_store: set.secure_store,
            settings_store: set.settings_store,
        }
    }
}

/// Primary façade exposed to host applications.
#[derive(Clone)]
pub struct CoreService {
    deps: Arc<CoreDependencies>,
}

impl CoreService {
    /// Create a new service from the provided dependencies.
    pub fn new(deps: CoreDependencies) -> Self {
        Self {
            deps: Arc::new(deps),
        }
    }

    /// Access the bridge dependencies being used by the service.
    pub fn dependencies(&self) -> Arc<CoreDependencies> {
        Arc::clone(&self.deps)
    }
}

/// Convenience bootstrapper for WebAssembly hosts.
///
/// ```
/// # #[cfg(feature = "wasm")]
/// # async fn example() -> core_service::Result<()> {
/// use core_service::{bootstrap_wasm, WasmBridgeConfig};
///
/// let config = WasmBridgeConfig::new("my-app-namespace");
/// let core = bootstrap_wasm(config).await?;
/// let http = core.dependencies().http_client.clone();
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "wasm")]
pub async fn bootstrap_wasm(config: WasmBridgeConfig) -> Result<CoreService> {
    let bridges = build_wasm_bridges(config)
        .await
        .map_err(|err| CoreError::InitializationFailed(err.to_string()))?;
    Ok(CoreService::new(CoreDependencies::from(bridges)))
}
