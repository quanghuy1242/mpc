//! # Authentication Manager
//!
//! Unified authentication orchestrator for multi-provider OAuth 2.0 flows.
//!
//! ## Overview
//!
//! The `AuthManager` provides a high-level API for managing authentication across
//! multiple cloud storage providers. It orchestrates OAuth flows, token management,
//! and session tracking while emitting auth events to the application's event bus.
//!
//! ## Features
//!
//! - Multi-provider support (Google Drive, OneDrive)
//! - Automatic token refresh before expiration
//! - Concurrent sign-in attempt protection
//! - Event emission for auth state changes
//! - Secure token storage via platform-specific secure stores
//! - Timeout and cancellation support
//!
//! ## Usage
//!
//! ```no_run
//! use core_auth::{AuthManager, ProviderKind};
//! use core_runtime::events::EventBus;
//! use std::sync::Arc;
//! # use bridge_traits::{SecureStore, http::HttpClient, error::Result as BridgeResult};
//! # struct MockSecureStore;
//! # #[async_trait::async_trait]
//! # impl SecureStore for MockSecureStore {
//! #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
//! #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
//! #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
//! #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
//! #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
//! # }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let http_client: Arc<dyn HttpClient> = todo!();
//! let event_bus = EventBus::new(100);
//! let secure_store = Arc::new(MockSecureStore);
//!
//! let manager = AuthManager::new(secure_store, event_bus.clone(), http_client);
//!
//! // List available providers
//! let providers = manager.list_providers();
//!
//! // Sign in (would launch browser flow in real implementation)
//! // let profile_id = manager.sign_in(ProviderKind::GoogleDrive).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{AuthError, Result};
use crate::oauth::{OAuthConfig, OAuthFlowManager, PkceVerifier};
use crate::token_store::TokenStore;
#[cfg(test)]
use crate::types::OAuthTokens;
use crate::types::{AuthState, ProfileId, ProviderKind};
use bridge_traits::{http::HttpClient, SecureStore};
use core_runtime::events::{AuthEvent, CoreEvent, EventBus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, instrument, warn};

/// Default timeout for authentication operations (2 minutes)
const DEFAULT_AUTH_TIMEOUT: Duration = Duration::from_secs(120);

/// Buffer time before token expiration to trigger refresh (5 minutes)
const TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(300);

/// Information about an available provider.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// The provider type
    pub kind: ProviderKind,
    /// Human-readable display name
    pub display_name: String,
    /// OAuth authorization URL
    pub auth_url: String,
    /// OAuth token URL
    pub token_url: String,
    /// Required OAuth scopes
    pub scopes: Vec<String>,
}

/// Current authentication session information.
#[derive(Debug, Clone)]
pub struct Session {
    /// The authenticated profile ID
    pub profile_id: ProfileId,
    /// The provider used for this session
    pub provider: ProviderKind,
    /// Current authentication state
    pub state: AuthState,
    /// Token expiration timestamp (if available)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// State tracking for in-progress sign-in operations.
#[derive(Debug)]
struct SignInProgress {
    state: String,
    verifier: PkceVerifier,
}

/// Unified authentication manager orchestrating OAuth flows and token management.
///
/// The `AuthManager` provides a high-level API for:
/// - Initiating OAuth 2.0 flows with PKCE
/// - Managing tokens with automatic refresh
/// - Tracking authentication sessions
/// - Emitting auth state events
/// - Handling concurrent operations safely
pub struct AuthManager {
    /// Token storage using platform-specific secure store
    token_store: TokenStore,
    /// Event bus for emitting auth events
    event_bus: EventBus,
    /// OAuth flow managers per provider
    oauth_managers: HashMap<ProviderKind, OAuthFlowManager>,
    /// Currently active session
    current_session: Arc<RwLock<Option<Session>>>,
    /// In-progress sign-in operations (keyed by provider)
    in_progress: Arc<Mutex<HashMap<ProviderKind, SignInProgress>>>,
    /// Token refresh locks to prevent concurrent refreshes
    refresh_locks: Arc<Mutex<HashMap<ProfileId, Arc<Mutex<()>>>>>,
}

impl AuthManager {
    /// Creates a new authentication manager.
    ///
    /// # Arguments
    ///
    /// * `secure_store` - Platform-specific secure storage for tokens
    /// * `event_bus` - Event bus for emitting authentication events
    /// * `http_client` - Host-provided HTTP client abstraction for OAuth calls
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use core_auth::AuthManager;
    /// use core_runtime::events::EventBus;
    /// use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, http::HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    ///
    /// let event_bus = EventBus::new(100);
    /// let secure_store = Arc::new(MockSecureStore);
    /// let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// ```
    pub fn new(
        secure_store: Arc<dyn SecureStore>,
        event_bus: EventBus,
        http_client: Arc<dyn HttpClient>,
    ) -> Self {
        let token_store = TokenStore::new(secure_store);

        // Initialize OAuth managers for each provider
        let mut oauth_managers = HashMap::new();
        oauth_managers.insert(
            ProviderKind::GoogleDrive,
            OAuthFlowManager::new(Self::google_drive_config(), http_client.clone()),
        );
        oauth_managers.insert(
            ProviderKind::OneDrive,
            OAuthFlowManager::new(Self::onedrive_config(), http_client.clone()),
        );

        Self {
            token_store,
            event_bus,
            oauth_managers,
            current_session: Arc::new(RwLock::new(None)),
            in_progress: Arc::new(Mutex::new(HashMap::new())),
            refresh_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Lists all available authentication providers.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::AuthManager;
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, http::HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// let providers = manager.list_providers();
    /// for provider in providers {
    ///     println!("{}: {}", provider.display_name, provider.auth_url);
    /// }
    /// ```
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                kind: ProviderKind::GoogleDrive,
                display_name: "Google Drive".to_string(),
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scopes: vec!["https://www.googleapis.com/auth/drive.readonly".to_string()],
            },
            ProviderInfo {
                kind: ProviderKind::OneDrive,
                display_name: "Microsoft OneDrive".to_string(),
                auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                    .to_string(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
                scopes: vec!["Files.Read".to_string(), "offline_access".to_string()],
            },
        ]
    }

    /// Initiates the OAuth 2.0 sign-in flow for the specified provider.
    ///
    /// This method:
    /// 1. Checks for concurrent sign-in attempts
    /// 2. Generates PKCE parameters
    /// 3. Builds the authorization URL
    /// 4. Emits `SigningIn` event
    /// 5. Returns the authorization URL for the host to launch in a browser
    ///
    /// The host application must:
    /// - Open the returned URL in a browser or web view
    /// - Capture the OAuth callback with the authorization code
    /// - Call `complete_sign_in` with the code
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider to authenticate with
    ///
    /// # Returns
    ///
    /// The authorization URL to present to the user
    ///
    /// # Errors
    ///
    /// - `AuthError::SignInInProgress` - A sign-in is already in progress for this provider
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::{AuthManager, ProviderKind};
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, http::HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// let auth_url = manager.sign_in(ProviderKind::GoogleDrive).await?;
    /// println!("Please visit: {}", auth_url);
    /// // Host launches browser, captures callback, calls complete_sign_in
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(provider = %provider))]
    pub async fn sign_in(&self, provider: ProviderKind) -> Result<String> {
        // Check for concurrent sign-in
        let mut in_progress = self.in_progress.lock().await;
        if in_progress.contains_key(&provider) {
            warn!("Sign-in already in progress for provider");
            return Err(AuthError::SignInInProgress {
                provider: provider.to_string(),
            });
        }

        info!("Initiating sign-in flow");

        // Get OAuth manager for provider
        let oauth_manager = self
            .oauth_managers
            .get(&provider)
            .ok_or_else(|| AuthError::InvalidProvider(provider.to_string()))?;

        // Generate PKCE parameters and build authorization URL
        let (auth_url, verifier) = oauth_manager.build_auth_url()?;

        // Extract state from verifier
        let state = verifier.state().to_string();

        // Track in-progress sign-in
        in_progress.insert(
            provider,
            SignInProgress {
                state: state.clone(),
                verifier,
            },
        );
        drop(in_progress);

        // Emit SigningIn event
        let event = CoreEvent::Auth(AuthEvent::SigningIn {
            provider: provider.to_string(),
        });
        let _ = self.event_bus.emit(event);

        info!("Sign-in flow initiated, authorization URL generated");
        Ok(auth_url)
    }

    /// Completes the OAuth sign-in flow after receiving the authorization code.
    ///
    /// This method:
    /// 1. Validates the state parameter (CSRF protection)
    /// 2. Exchanges the authorization code for tokens
    /// 3. Stores the tokens securely
    /// 4. Creates a new session
    /// 5. Emits `SignedIn` event
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider being authenticated
    /// * `code` - The authorization code from the OAuth callback
    /// * `state` - The state parameter from the OAuth callback (for CSRF validation)
    ///
    /// # Returns
    ///
    /// The newly created profile ID
    ///
    /// # Errors
    ///
    /// - `AuthError::NoSignInInProgress` - No sign-in was initiated for this provider
    /// - `AuthError::InvalidState` - State parameter doesn't match (CSRF attack)
    /// - `AuthError::TokenExchangeFailed` - Failed to exchange code for tokens
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::{AuthManager, ProviderKind};
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// // After user approves in browser and callback is received
    /// let code = "authorization_code_from_callback";
    /// let state = "state_from_callback";
    /// // let profile_id = manager.complete_sign_in(
    /// //     ProviderKind::GoogleDrive,
    /// //     code.to_string(),
    /// //     state.to_string()
    /// // ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, code), fields(provider = %provider))]
    pub async fn complete_sign_in(
        &self,
        provider: ProviderKind,
        code: String,
        state: String,
    ) -> Result<ProfileId> {
        // Get and remove in-progress sign-in
        let mut in_progress = self.in_progress.lock().await;
        let sign_in_data = in_progress.remove(&provider).ok_or_else(|| {
            warn!("No sign-in in progress for provider");
            AuthError::NoSignInInProgress {
                provider: provider.to_string(),
            }
        })?;
        drop(in_progress);

        // Validate state (CSRF protection)
        if state != sign_in_data.state {
            error!("State parameter mismatch - possible CSRF attack");
            let event = CoreEvent::Auth(AuthEvent::AuthError {
                profile_id: None,
                message: "Invalid state parameter".to_string(),
                recoverable: false,
            });
            let _ = self.event_bus.emit(event);

            return Err(AuthError::InvalidState);
        }

        // Get OAuth manager
        let oauth_manager = self
            .oauth_managers
            .get(&provider)
            .ok_or_else(|| AuthError::InvalidProvider(provider.to_string()))?;

        // Exchange code for tokens with state verification
        info!("Exchanging authorization code for tokens");
        let tokens = match timeout(
            DEFAULT_AUTH_TIMEOUT,
            oauth_manager.exchange_code(&code, &state, &sign_in_data.verifier),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                error!("Token exchange timed out");
                let event = CoreEvent::Auth(AuthEvent::AuthError {
                    profile_id: None,
                    message: "Authentication timeout".to_string(),
                    recoverable: true,
                });
                let _ = self.event_bus.emit(event);
                return Err(AuthError::OperationTimeout {
                    operation: "token exchange".to_string(),
                });
            }
        };

        // Create profile and store tokens
        let profile_id = ProfileId::new();
        self.token_store
            .store_tokens(profile_id, &tokens)
            .await
            .map_err(|e| {
                error!("Failed to store tokens: {}", e);
                let event = CoreEvent::Auth(AuthEvent::AuthError {
                    profile_id: Some(profile_id.to_string()),
                    message: format!("Failed to store credentials: {}", e),
                    recoverable: false,
                });
                let _ = self.event_bus.emit(event);
                e
            })?;

        // Update current session
        let expires_at = Some(tokens.expires_at);

        let session = Session {
            profile_id,
            provider,
            state: AuthState::SignedIn,
            expires_at,
        };

        {
            let mut current_session = self.current_session.write().await;
            *current_session = Some(session);
        }

        // Emit SignedIn event
        let event = CoreEvent::Auth(AuthEvent::SignedIn {
            profile_id: profile_id.to_string(),
            provider: provider.to_string(),
        });
        let _ = self.event_bus.emit(event);

        info!(profile_id = %profile_id, "Sign-in completed successfully");
        Ok(profile_id)
    }

    /// Signs out the specified profile, clearing tokens and session.
    ///
    /// This method:
    /// 1. Deletes stored tokens securely
    /// 2. Clears the current session if it matches
    /// 3. Emits `SignedOut` event
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile to sign out
    ///
    /// # Errors
    ///
    /// Returns an error if token deletion fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::{AuthManager, ProfileId};
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// let profile_id = ProfileId::new();
    /// manager.sign_out(profile_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(profile_id = %profile_id))]
    pub async fn sign_out(&self, profile_id: ProfileId) -> Result<()> {
        info!("Signing out profile");

        // Delete tokens
        self.token_store
            .delete_tokens(profile_id)
            .await
            .map_err(|e| {
                error!("Failed to delete tokens: {}", e);
                e
            })?;

        // Clear current session if it matches
        {
            let mut current_session = self.current_session.write().await;
            if let Some(session) = current_session.as_ref() {
                if session.profile_id == profile_id {
                    *current_session = None;
                }
            }
        }

        // Emit SignedOut event
        let event = CoreEvent::Auth(AuthEvent::SignedOut {
            profile_id: profile_id.to_string(),
        });
        let _ = self.event_bus.emit(event);

        info!("Sign-out completed successfully");
        Ok(())
    }

    /// Gets a valid access token for the specified profile, refreshing if necessary.
    ///
    /// This method:
    /// 1. Retrieves stored tokens
    /// 2. Checks token expiration
    /// 3. Refreshes token if needed (with buffer time)
    /// 4. Returns the valid access token
    ///
    /// Token refresh is protected by a per-profile lock to prevent concurrent refreshes.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile to get a token for
    ///
    /// # Returns
    ///
    /// A valid access token string
    ///
    /// # Errors
    ///
    /// - `AuthError::ProfileNotFound` - No tokens stored for this profile
    /// - `AuthError::TokenRefreshFailed` - Token refresh failed
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::{AuthManager, ProfileId};
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// # let profile_id = ProfileId::new();
    /// // Get a valid token (automatically refreshes if needed)
    /// // let token = manager.get_valid_token(profile_id).await?;
    /// // Use token for API calls...
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(profile_id = %profile_id))]
    pub async fn get_valid_token(&self, profile_id: ProfileId) -> Result<String> {
        // Acquire or create refresh lock for this profile
        let refresh_lock = {
            let mut locks = self.refresh_locks.lock().await;
            locks
                .entry(profile_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Lock to prevent concurrent refreshes
        let _guard = refresh_lock.lock().await;

        // Retrieve current tokens
        let tokens = self
            .token_store
            .retrieve_tokens(profile_id)
            .await?
            .ok_or_else(|| {
                warn!("No tokens found for profile");
                AuthError::ProfileNotFound(profile_id.to_string())
            })?;

        // Check if token needs refresh (using is_expired_with_buffer from OAuthTokens)
        let needs_refresh = tokens.is_expired_with_buffer(TOKEN_REFRESH_BUFFER.as_secs() as i64);

        if !needs_refresh {
            debug!("Token is valid, no refresh needed");
            return Ok(tokens.access_token);
        }

        // Token needs refresh
        info!("Token expired or expiring soon, refreshing");

        // Emit TokenRefreshing event
        let event = CoreEvent::Auth(AuthEvent::TokenRefreshing {
            profile_id: profile_id.to_string(),
        });
        let _ = self.event_bus.emit(event);

        // Get refresh token
        let refresh_token = tokens.refresh_token.ok_or_else(|| {
            error!("No refresh token available");
            AuthError::NoRefreshToken {
                profile_id: profile_id.to_string(),
            }
        })?;

        // Get provider from session or default to GoogleDrive
        let provider = {
            let session = self.current_session.read().await;
            session
                .as_ref()
                .filter(|s| s.profile_id == profile_id)
                .map(|s| s.provider)
                .unwrap_or(ProviderKind::GoogleDrive)
        };

        // Get OAuth manager
        let oauth_manager = self
            .oauth_managers
            .get(&provider)
            .ok_or_else(|| AuthError::InvalidProvider(provider.to_string()))?;

        // Refresh the token
        let new_tokens = match timeout(
            DEFAULT_AUTH_TIMEOUT,
            oauth_manager.refresh_access_token(&refresh_token),
        )
        .await
        {
            Ok(result) => result.map_err(|e| {
                error!("Token refresh failed: {}", e);
                let event = CoreEvent::Auth(AuthEvent::AuthError {
                    profile_id: Some(profile_id.to_string()),
                    message: format!("Token refresh failed: {}", e),
                    recoverable: true,
                });
                let _ = self.event_bus.emit(event);
                e
            })?,
            Err(_) => {
                error!("Token refresh timed out");
                let event = CoreEvent::Auth(AuthEvent::AuthError {
                    profile_id: Some(profile_id.to_string()),
                    message: "Token refresh timeout".to_string(),
                    recoverable: true,
                });
                let _ = self.event_bus.emit(event);
                return Err(AuthError::OperationTimeout {
                    operation: "token refresh".to_string(),
                });
            }
        };

        // Store refreshed tokens
        self.token_store
            .store_tokens(profile_id, &new_tokens)
            .await?;

        // Emit TokenRefreshed event
        let expires_at = new_tokens.expires_at();
        let event = CoreEvent::Auth(AuthEvent::TokenRefreshed {
            profile_id: profile_id.to_string(),
            expires_at: expires_at as u64,
        });
        let _ = self.event_bus.emit(event);

        info!("Token refreshed successfully");
        Ok(new_tokens.access_token)
    }

    /// Gets the current active session, if any.
    ///
    /// # Returns
    ///
    /// The current session information, or `None` if no user is signed in
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::AuthManager;
    /// # use core_runtime::events::EventBus;
    /// # use std::sync::Arc;
    /// # use bridge_traits::{SecureStore, HttpClient, error::Result as BridgeResult};
    /// # struct MockSecureStore;
    /// # #[async_trait::async_trait]
    /// # impl SecureStore for MockSecureStore {
    /// #     async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> { Ok(()) }
    /// #     async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> { Ok(None) }
    /// #     async fn delete_secret(&self, key: &str) -> BridgeResult<()> { Ok(()) }
    /// #     async fn list_keys(&self) -> BridgeResult<Vec<String>> { Ok(vec![]) }
    /// #     async fn clear_all(&self) -> BridgeResult<()> { Ok(()) }
    /// # }
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let event_bus = EventBus::new(100);
    /// # let secure_store = Arc::new(MockSecureStore);
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let manager = AuthManager::new(secure_store, event_bus, http_client);
    /// if let Some(session) = manager.current_session().await {
    ///     println!("Signed in as: {}", session.profile_id);
    /// }
    /// # }
    /// ```
    pub async fn current_session(&self) -> Option<Session> {
        self.current_session.read().await.clone()
    }

    /// Cancels an in-progress sign-in for the specified provider.
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider to cancel sign-in for
    ///
    /// # Returns
    ///
    /// `true` if a sign-in was cancelled, `false` if none was in progress
    pub async fn cancel_sign_in(&self, provider: ProviderKind) -> bool {
        let mut in_progress = self.in_progress.lock().await;
        in_progress.remove(&provider).is_some()
    }

    /// Google Drive OAuth configuration.
    fn google_drive_config() -> OAuthConfig {
        OAuthConfig {
            provider: ProviderKind::GoogleDrive,
            client_id: std::env::var("GOOGLE_CLIENT_ID")
                .unwrap_or_else(|_| "placeholder_client_id".to_string()),
            client_secret: std::env::var("GOOGLE_CLIENT_SECRET").ok(),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["https://www.googleapis.com/auth/drive.readonly".to_string()],
        }
    }

    /// OneDrive OAuth configuration.
    fn onedrive_config() -> OAuthConfig {
        OAuthConfig {
            provider: ProviderKind::OneDrive,
            client_id: std::env::var("ONEDRIVE_CLIENT_ID")
                .unwrap_or_else(|_| "placeholder_client_id".to_string()),
            client_secret: std::env::var("ONEDRIVE_CLIENT_SECRET").ok(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["Files.Read".to_string(), "offline_access".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::error::{BridgeError, Result as BridgeResult};
    use bridge_traits::http::{HttpClient, HttpRequest, HttpResponse};
    use bridge_traits::SecureStore;
    use std::collections::HashMap as StdHashMap;
    use tokio::sync::Mutex as TokioMutex;

    // Mock SecureStore for testing
    struct MockSecureStore {
        storage: Arc<TokioMutex<StdHashMap<String, Vec<u8>>>>,
    }

    impl MockSecureStore {
        fn new() -> Self {
            Self {
                storage: Arc::new(TokioMutex::new(StdHashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl SecureStore for MockSecureStore {
        async fn set_secret(&self, key: &str, value: &[u8]) -> BridgeResult<()> {
            let mut storage = self.storage.lock().await;
            storage.insert(key.to_string(), value.to_vec());
            Ok(())
        }

        async fn get_secret(&self, key: &str) -> BridgeResult<Option<Vec<u8>>> {
            let storage = self.storage.lock().await;
            Ok(storage.get(key).cloned())
        }

        async fn delete_secret(&self, key: &str) -> BridgeResult<()> {
            let mut storage = self.storage.lock().await;
            storage.remove(key);
            Ok(())
        }

        async fn list_keys(&self) -> BridgeResult<Vec<String>> {
            let storage = self.storage.lock().await;
            Ok(storage.keys().cloned().collect())
        }

        async fn clear_all(&self) -> BridgeResult<()> {
            let mut storage = self.storage.lock().await;
            storage.clear();
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockHttpClient;

    #[async_trait::async_trait]
    impl HttpClient for MockHttpClient {
        async fn execute(&self, _request: HttpRequest) -> BridgeResult<HttpResponse> {
            Err(BridgeError::OperationFailed(
                "HTTP client not mocked for AuthManager tests".to_string(),
            ))
        }

        async fn download_stream(
            &self,
            _url: String,
        ) -> BridgeResult<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
            Err(BridgeError::OperationFailed(
                "HTTP client not mocked for AuthManager tests".to_string(),
            ))
        }
    }

    #[test]
    fn test_create_auth_manager() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        // Verify providers are initialized
        let providers = manager.list_providers();
        assert_eq!(providers.len(), 2);
        assert!(providers
            .iter()
            .any(|p| p.kind == ProviderKind::GoogleDrive));
        assert!(providers.iter().any(|p| p.kind == ProviderKind::OneDrive));
    }

    #[tokio::test]
    async fn test_list_providers() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        let providers = manager.list_providers();
        assert_eq!(providers.len(), 2);

        let google = providers
            .iter()
            .find(|p| p.kind == ProviderKind::GoogleDrive)
            .unwrap();
        assert_eq!(google.display_name, "Google Drive");
        assert!(!google.scopes.is_empty());

        let onedrive = providers
            .iter()
            .find(|p| p.kind == ProviderKind::OneDrive)
            .unwrap();
        assert_eq!(onedrive.display_name, "Microsoft OneDrive");
        assert!(!onedrive.scopes.is_empty());
    }

    #[tokio::test]
    async fn test_sign_in_initiates_flow() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus.clone(), http_client);

        // Subscribe to events
        let mut receiver = event_bus.subscribe();

        // Initiate sign-in
        let result = manager.sign_in(ProviderKind::GoogleDrive).await;
        assert!(result.is_ok());

        let auth_url = result.unwrap();
        assert!(auth_url.contains("accounts.google.com"));
        assert!(auth_url.contains("client_id"));
        assert!(auth_url.contains("code_challenge"));

        // Verify SigningIn event was emitted
        let event = receiver.try_recv().unwrap();
        match event {
            CoreEvent::Auth(AuthEvent::SigningIn { provider }) => {
                assert_eq!(provider, "Google Drive");
            }
            _ => panic!("Expected SigningIn event"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_sign_in_prevented() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client.clone());

        // First sign-in should succeed
        let result1 = manager.sign_in(ProviderKind::GoogleDrive).await;
        assert!(result1.is_ok());

        // Second concurrent sign-in should fail
        let result2 = manager.sign_in(ProviderKind::GoogleDrive).await;
        assert!(result2.is_err());
        assert!(matches!(
            result2.unwrap_err(),
            AuthError::SignInInProgress { .. }
        ));
    }

    #[tokio::test]
    async fn test_cancel_sign_in() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        // Initiate sign-in
        let result = manager.sign_in(ProviderKind::GoogleDrive).await;
        assert!(result.is_ok());

        // Cancel should succeed
        let cancelled = manager.cancel_sign_in(ProviderKind::GoogleDrive).await;
        assert!(cancelled);

        // Second cancel should return false
        let cancelled2 = manager.cancel_sign_in(ProviderKind::GoogleDrive).await;
        assert!(!cancelled2);

        // Should be able to sign in again after cancel
        let result2 = manager.sign_in(ProviderKind::GoogleDrive).await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_sign_out() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus.clone(), http_client);

        let profile_id = ProfileId::new();

        // Store some tokens first
        let tokens = OAuthTokens::new(
            "test_token".to_string(),
            Some("refresh_token".to_string()),
            3600,
        );
        manager
            .token_store
            .store_tokens(profile_id, &tokens)
            .await
            .unwrap();

        // Subscribe to events
        let mut receiver = event_bus.subscribe();

        // Sign out
        let result = manager.sign_out(profile_id).await;
        assert!(result.is_ok());

        // Verify tokens were deleted
        let retrieved = manager
            .token_store
            .retrieve_tokens(profile_id)
            .await
            .unwrap();
        assert!(retrieved.is_none());

        // Verify SignedOut event was emitted
        let event = receiver.try_recv().unwrap();
        match event {
            CoreEvent::Auth(AuthEvent::SignedOut { profile_id: pid }) => {
                assert_eq!(pid, profile_id.to_string());
            }
            _ => panic!("Expected SignedOut event"),
        }
    }

    #[tokio::test]
    async fn test_current_session_none_initially() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        let session = manager.current_session().await;
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_get_valid_token_no_profile() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        let profile_id = ProfileId::new();
        let result = manager.get_valid_token(profile_id).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AuthError::ProfileNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_provider_info_completeness() {
        let event_bus = EventBus::new(100);
        let secure_store = Arc::new(MockSecureStore::new());
        let http_client = Arc::new(MockHttpClient::default());
        let manager = AuthManager::new(secure_store, event_bus, http_client);

        let providers = manager.list_providers();

        for provider in providers {
            assert!(!provider.display_name.is_empty());
            assert!(!provider.auth_url.is_empty());
            assert!(!provider.token_url.is_empty());
            assert!(!provider.scopes.is_empty());
            assert!(provider.auth_url.starts_with("https://"));
            assert!(provider.token_url.starts_with("https://"));
        }
    }
}
