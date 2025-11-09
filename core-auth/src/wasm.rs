//! WebAssembly bindings for core-auth
//!
//! This module provides JavaScript/TypeScript-friendly bindings for the authentication
//! functionality using wasm-bindgen.

use crate::manager::{AuthManager, ProviderInfo};
use crate::types::{AuthState, ProviderKind, ProfileId};
use bridge_wasm::{JsHttpClient, JsSecureStore}; // ✅ Use JS-compatible bridge types
use core_runtime::wasm::JsEventBus; // ✅ Import JS-compatible EventBus
use std::sync::Arc;
use wasm_bindgen::prelude::*;

// NOTE: Logging functions (initLogging, etc.) are exported by core-runtime.
// Event creation functions are also from core-runtime.
// We only export auth-specific functionality here.

// =============================================================================
// Types - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible provider kind enum
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub enum JsProviderKind {
    GoogleDrive,
    OneDrive,
}

impl From<JsProviderKind> for ProviderKind {
    fn from(js: JsProviderKind) -> Self {
        match js {
            JsProviderKind::GoogleDrive => ProviderKind::GoogleDrive,
            JsProviderKind::OneDrive => ProviderKind::OneDrive,
        }
    }
}

impl From<ProviderKind> for JsProviderKind {
    fn from(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::GoogleDrive => JsProviderKind::GoogleDrive,
            ProviderKind::OneDrive => JsProviderKind::OneDrive,
        }
    }
}

/// JavaScript-accessible provider information
#[wasm_bindgen]
#[derive(Clone)]
pub struct JsProviderInfo {
    inner: ProviderInfo,
}

#[wasm_bindgen]
impl JsProviderInfo {
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> JsProviderKind {
        self.inner.kind.into()
    }

    #[wasm_bindgen(js_name = displayName, getter)]
    pub fn display_name(&self) -> String {
        self.inner.display_name.clone()
    }

    #[wasm_bindgen(js_name = authUrl, getter)]
    pub fn auth_url(&self) -> String {
        self.inner.auth_url.clone()
    }

    #[wasm_bindgen(js_name = tokenUrl, getter)]
    pub fn token_url(&self) -> String {
        self.inner.token_url.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn scopes(&self) -> Vec<String> {
        self.inner.scopes.clone()
    }
}

impl From<ProviderInfo> for JsProviderInfo {
    fn from(inner: ProviderInfo) -> Self {
        Self { inner }
    }
}

/// JavaScript-accessible authentication state
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub enum JsAuthState {
    SignedOut,
    SigningIn,
    SignedIn,
    TokenRefreshing,
}

impl From<AuthState> for JsAuthState {
    fn from(state: AuthState) -> Self {
        match state {
            AuthState::SignedOut => JsAuthState::SignedOut,
            AuthState::SigningIn => JsAuthState::SigningIn,
            AuthState::SignedIn => JsAuthState::SignedIn,
            AuthState::TokenRefreshing => JsAuthState::TokenRefreshing,
        }
    }
}

impl From<JsAuthState> for AuthState {
    fn from(js: JsAuthState) -> Self {
        match js {
            JsAuthState::SignedOut => AuthState::SignedOut,
            JsAuthState::SigningIn => AuthState::SigningIn,
            JsAuthState::SignedIn => AuthState::SignedIn,
            JsAuthState::TokenRefreshing => AuthState::TokenRefreshing,
        }
    }
}

// NOTE: Session type removed - requires sync access to AuthManager state
// which isn't possible with current async architecture. Host applications
// should track session state via events instead.

// =============================================================================
// AuthManager - Main API
// =============================================================================

/// JavaScript-accessible authentication manager
///
/// Manages OAuth 2.0 flows for multiple cloud storage providers.
///
/// # Example
///
/// ```javascript
/// import init, { JsAuthManager, JsProviderKind } from './core_auth';
/// import { JsEventBus } from './core_runtime';
///
/// await init();
///
/// // Create event bus
/// const eventBus = new JsEventBus(100);
///
/// // Create auth manager (bridges created internally)
/// const authManager = JsAuthManager.create(eventBus);
///
/// // Use it
/// const authUrl = await authManager.signIn(JsProviderKind.GoogleDrive);
/// ```
#[wasm_bindgen]
pub struct JsAuthManager {
    inner: Arc<AuthManager>,
}

#[wasm_bindgen]
impl JsAuthManager {
    /// Create a new authentication manager
    ///
    /// Accepts custom bridge implementations for maximum flexibility.
    ///
    /// # Arguments
    ///
    /// * `event_bus` - Event bus from core-runtime for emitting auth events
    /// * `http_client` - HTTP client from bridge-wasm (can pass custom fetch)
    /// * `secure_store` - Secure storage from bridge-wasm (with custom namespace)
    ///
    /// # Example
    ///
    /// ```javascript
    /// import { JsEventBus } from './core_runtime';
    /// import { JsHttpClient, JsSecureStore } from './bridge_wasm';
    /// import { JsAuthManager } from './core_auth';
    ///
    /// const eventBus = new JsEventBus(100);
    /// const httpClient = new JsHttpClient(); // Or: new JsHttpClient(customFetch)
    /// const secureStore = new JsSecureStore("auth");
    ///
    /// const authManager = new JsAuthManager(eventBus, httpClient, secureStore);
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(
        event_bus: &JsEventBus,
        http_client: &JsHttpClient,
        secure_store: &JsSecureStore,
    ) -> std::result::Result<JsAuthManager, JsValue> {
        // Wrap JS bridges in Arc for AuthManager
        let http_client_arc: Arc<dyn bridge_traits::http::HttpClient> = 
            Arc::new(http_client.clone());
        let secure_store_arc: Arc<dyn bridge_traits::SecureStore> = 
            Arc::new(secure_store.clone());

        // Extract native EventBus from JsEventBus wrapper
        let native_event_bus = (**event_bus.inner()).clone();

        // Create AuthManager
        let manager = Arc::new(AuthManager::new(secure_store_arc, native_event_bus, http_client_arc));

        Ok(Self { inner: manager })
    }

    /// List all available authentication providers
    ///
    /// # Example
    ///
    /// ```javascript
    /// const providers = authManager.listProviders();
    /// providers.forEach(provider => {
    ///   console.log(`${provider.displayName}: ${provider.authUrl}`);
    /// });
    /// ```
    #[wasm_bindgen(js_name = listProviders)]
    pub fn list_providers(&self) -> Vec<JsProviderInfo> {
        self.inner
            .list_providers()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Initiate OAuth 2.0 sign-in flow
    ///
    /// Returns the authorization URL that should be opened in a browser.
    ///
    /// # Example
    ///
    /// ```javascript
    /// const authUrl = await authManager.signIn(JsProviderKind.GoogleDrive);
    /// window.open(authUrl, '_blank');
    /// ```
    #[wasm_bindgen(js_name = signIn)]
    pub async fn sign_in(&self, provider: JsProviderKind) -> std::result::Result<String, JsValue> {
        self.inner
            .sign_in(provider.into())
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Complete the OAuth sign-in flow
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider being authenticated
    /// * `code` - Authorization code from OAuth callback
    /// * `state` - State parameter from OAuth callback (CSRF protection)
    ///
    /// # Returns
    ///
    /// The profile ID (UUID string)
    ///
    /// # Example
    ///
    /// ```javascript
    /// const urlParams = new URLSearchParams(window.location.search);
    /// const code = urlParams.get('code');
    /// const state = urlParams.get('state');
    ///
    /// const profileId = await authManager.completeSignIn(
    ///   JsProviderKind.GoogleDrive,
    ///   code,
    ///   state
    /// );
    /// console.log('Signed in! Profile ID:', profileId);
    /// ```
    #[wasm_bindgen(js_name = completeSignIn)]
    pub async fn complete_sign_in(
        &self,
        provider: JsProviderKind,
        code: String,
        state: String,
    ) -> std::result::Result<String, JsValue> {
        self.inner
            .complete_sign_in(provider.into(), code, state)
            .await
            .map(|profile_id| profile_id.to_string())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Cancel an in-progress sign-in operation
    ///
    /// # Example
    ///
    /// ```javascript
    /// // User closed OAuth popup
    /// await authManager.cancelSignIn(JsProviderKind.GoogleDrive);
    /// ```
    #[wasm_bindgen(js_name = cancelSignIn)]
    pub async fn cancel_sign_in(&self, provider: JsProviderKind) -> std::result::Result<(), JsValue> {
        self.inner.cancel_sign_in(provider.into()).await;
        Ok(())
    }

    /// Sign out and clear tokens
    ///
    /// # Example
    ///
    /// ```javascript
    /// await authManager.signOut(profileId);
    /// ```
    #[wasm_bindgen(js_name = signOut)]
    pub async fn sign_out(&self, profile_id: String) -> std::result::Result<(), JsValue> {
        let pid = ProfileId::from_string(&profile_id)
            .map_err(|e| JsValue::from_str(&format!("Invalid profile ID: {}", e)))?;

        self.inner
            .sign_out(pid)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get the module version
#[wasm_bindgen(js_name = authVersion)]
pub fn auth_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the module name
#[wasm_bindgen(js_name = authName)]
pub fn auth_name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

/// Parse a provider kind from string
///
/// # Arguments
///
/// * `s` - Provider identifier ("google_drive", "onedrive", etc.)
///
/// # Returns
///
/// The provider kind, or null if invalid
///
/// # Example
///
/// ```javascript
/// const provider = parseProvider('google_drive');
/// if (provider) {
///   console.log('Valid provider:', provider);
/// }
/// ```
#[wasm_bindgen(js_name = parseProvider)]
pub fn parse_provider(s: &str) -> Option<JsProviderKind> {
    ProviderKind::parse(s).map(Into::into)
}

/// Get display name for a provider
///
/// # Example
///
/// ```javascript
/// console.log(providerDisplayName(JsProviderKind.GoogleDrive));
/// // Output: "Google Drive"
/// ```
#[wasm_bindgen(js_name = providerDisplayName)]
pub fn provider_display_name(provider: JsProviderKind) -> String {
    let kind: ProviderKind = provider.into();
    kind.display_name().to_string()
}

/// Check if a profile ID string is valid
///
/// # Example
///
/// ```javascript
/// if (isValidProfileId(profileId)) {
///   // Safe to use
/// } else {
///   console.error('Invalid profile ID');
/// }
/// ```
#[wasm_bindgen(js_name = isValidProfileId)]
pub fn is_valid_profile_id(s: &str) -> bool {
    ProfileId::from_string(s).is_ok()
}
