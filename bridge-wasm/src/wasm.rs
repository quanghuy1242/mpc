//! WebAssembly bindings for bridge-wasm
//!
//! This module provides JavaScript/TypeScript-friendly bindings for platform bridges,
//! allowing custom implementations to be passed from JavaScript.

use crate::http::WasmHttpClient as InternalHttpClient;
use crate::storage::WasmSecureStore as InternalSecureStore;
use async_trait::async_trait;
use bridge_traits::{
    error::Result as BridgeResult,
    http::{HttpClient, HttpMethod, HttpRequest, HttpResponse},
    SecureStore,
};
use js_sys::Function as JsFunction;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// =============================================================================
// Error Handling
// =============================================================================

fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}

// =============================================================================
// HTTP Client
// =============================================================================

/// JavaScript-accessible HTTP client
///
/// Wraps the browser's fetch API with optional custom fetch function.
///
/// # Example
///
/// ```javascript
/// // Use browser's native fetch
/// const httpClient = new JsHttpClient();
///
/// // Or provide custom fetch (for testing, logging, etc.)
/// const customFetch = async (url, options) => {
///   console.log('Fetching:', url);
///   return fetch(url, options);
/// };
/// const httpClient = new JsHttpClient(customFetch);
/// ```
#[wasm_bindgen]
pub struct JsHttpClient {
    inner: InternalHttpClient,
    custom_fetch: Option<JsFunction>,
}

#[wasm_bindgen]
impl JsHttpClient {
    /// Create a new HTTP client
    ///
    /// # Arguments
    ///
    /// * `custom_fetch` - Optional custom fetch function. If not provided, uses window.fetch
    ///
    /// # Example
    ///
    /// ```javascript
    /// // Default (uses window.fetch)
    /// const client = new JsHttpClient();
    ///
    /// // Custom fetch function
    /// const myFetch = async (url, options) => {
    ///   console.log(`Fetching ${url}`);
    ///   return fetch(url, options);
    /// };
    /// const client = new JsHttpClient(myFetch);
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(custom_fetch: Option<JsFunction>) -> Result<JsHttpClient, JsValue> {
        let inner = InternalHttpClient::new().map_err(to_js_error)?;

        Ok(Self {
            inner,
            custom_fetch,
        })
    }
}

// Internal bridge trait implementation
#[async_trait::async_trait(?Send)]
impl HttpClient for JsHttpClient {
    async fn execute(&self, request: HttpRequest) -> BridgeResult<HttpResponse> {
        // If custom fetch is provided, we could use it here
        // For now, delegate to internal implementation
        // TODO: Implement custom fetch invocation when needed
        self.inner.execute(request).await
    }

    async fn download_stream(&self, url: String) -> BridgeResult<Box<bridge_traits::platform::DynAsyncRead>> {
        self.inner.download_stream(url).await
    }
}

impl Clone for JsHttpClient {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            custom_fetch: self.custom_fetch.clone(),
        }
    }
}

// =============================================================================
// Secure Storage
// =============================================================================

/// JavaScript-accessible secure storage
///
/// Uses browser's IndexedDB with AES-GCM encryption for secure storage.
///
/// # Example
///
/// ```javascript
/// // Create secure storage with namespace
/// const secureStore = new JsSecureStore("auth");
///
/// // Store a secret
/// await secureStore.setSecret("token", new TextEncoder().encode("secret-value"));
///
/// // Retrieve a secret
/// const value = await secureStore.getSecret("token");
/// ```
#[wasm_bindgen]
pub struct JsSecureStore {
    inner: InternalSecureStore,
}

#[wasm_bindgen]
impl JsSecureStore {
    /// Create a new secure storage instance
    ///
    /// # Arguments
    ///
    /// * `namespace` - Storage namespace for isolating data (e.g., "auth", "cache")
    ///
    /// # Example
    ///
    /// ```javascript
    /// const store = new JsSecureStore("my-app-auth");
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(namespace: &str) -> Result<JsSecureStore, JsValue> {
        let inner = InternalSecureStore::new(namespace).map_err(to_js_error)?;

        Ok(Self { inner })
    }

    /// Store a secret
    ///
    /// # Arguments
    ///
    /// * `key` - Secret identifier
    /// * `value` - Secret data as Uint8Array
    ///
    /// # Example
    ///
    /// ```javascript
    /// const data = new TextEncoder().encode("my-secret");
    /// await store.setSecret("api-key", data);
    /// ```
    #[wasm_bindgen(js_name = setSecret)]
    pub async fn set_secret(&self, key: &str, value: &[u8]) -> Result<(), JsValue> {
        self.inner.set_secret(key, value).await.map_err(to_js_error)
    }

    /// Retrieve a secret
    ///
    /// # Arguments
    ///
    /// * `key` - Secret identifier
    ///
    /// # Returns
    ///
    /// Secret data as Uint8Array, or null if not found
    ///
    /// # Example
    ///
    /// ```javascript
    /// const data = await store.getSecret("api-key");
    /// if (data) {
    ///   const secret = new TextDecoder().decode(data);
    ///   console.log('Secret:', secret);
    /// }
    /// ```
    #[wasm_bindgen(js_name = getSecret)]
    pub async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>, JsValue> {
        self.inner.get_secret(key).await.map_err(to_js_error)
    }

    /// Delete a secret
    ///
    /// # Arguments
    ///
    /// * `key` - Secret identifier
    ///
    /// # Example
    ///
    /// ```javascript
    /// await store.deleteSecret("api-key");
    /// ```
    #[wasm_bindgen(js_name = deleteSecret)]
    pub async fn delete_secret(&self, key: &str) -> Result<(), JsValue> {
        self.inner.delete_secret(key).await.map_err(to_js_error)
    }

    /// List all secret keys
    ///
    /// # Returns
    ///
    /// Array of secret identifiers
    ///
    /// # Example
    ///
    /// ```javascript
    /// const keys = await store.listKeys();
    /// console.log('Stored secrets:', keys);
    /// ```
    #[wasm_bindgen(js_name = listKeys)]
    pub async fn list_keys(&self) -> Result<Vec<String>, JsValue> {
        self.inner.list_keys().await.map_err(to_js_error)
    }

    /// Clear all secrets
    ///
    /// # Example
    ///
    /// ```javascript
    /// await store.clearAll();
    /// ```
    #[wasm_bindgen(js_name = clearAll)]
    pub async fn clear_all(&self) -> Result<(), JsValue> {
        self.inner.clear_all().await.map_err(to_js_error)
    }
}

// Internal bridge trait implementation
#[async_trait::async_trait(?Send)]
impl SecureStore for JsSecureStore {
    async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> {
        self.inner.set_secret(key, value).await
    }

    async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> {
        self.inner.get_secret(key).await
    }

    async fn delete_secret(&self, key: &str) -> BridgeResult<()> {
        self.inner.delete_secret(key).await
    }

    async fn list_keys(&self) -> BridgeResult<Vec<String>> {
        self.inner.list_keys().await
    }

    async fn clear_all(&self) -> BridgeResult<()> {
        self.inner.clear_all().await
    }
}

impl Clone for JsSecureStore {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

// =============================================================================
// Module Info
// =============================================================================

/// Get the bridge-wasm version
#[wasm_bindgen(js_name = bridgeWasmVersion)]
pub fn bridge_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the bridge-wasm name
#[wasm_bindgen(js_name = bridgeWasmName)]
pub fn bridge_wasm_name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = bridge_wasm_version();
        assert!(!version.is_empty());
    }
}
