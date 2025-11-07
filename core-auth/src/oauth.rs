//! OAuth 2.0 Authorization Flow Manager with PKCE Support
//!
//! This module implements RFC 6749 (OAuth 2.0) and RFC 7636 (PKCE) for secure
//! authorization flows with cloud storage providers.
//!
//! # Overview
//!
//! The OAuth flow manager handles:
//! - Building authorization URLs with PKCE challenge
//! - Exchanging authorization codes for tokens
//! - Refreshing access tokens
//! - State verification for CSRF protection
//!
//! # Security
//!
//! - Uses PKCE (Proof Key for Code Exchange) for additional security
//! - Generates cryptographically secure random state and code verifier
//! - Validates state parameter to prevent CSRF attacks
//! - Never logs sensitive values (tokens, codes, verifiers)
//!
//! # Example
//!
//! ```no_run
//! use core_auth::oauth::{OAuthFlowManager, OAuthConfig};
//! use core_auth::ProviderKind;
//! use std::sync::Arc;
//!
//! # async fn example() -> core_auth::Result<()> {
//! # use bridge_traits::http::HttpClient;
//! # let http_client: Arc<dyn HttpClient> = todo!();
//! let config = OAuthConfig {
//!     provider: ProviderKind::GoogleDrive,
//!     client_id: "your-client-id".to_string(),
//!     client_secret: Some("your-client-secret".to_string()),
//!     redirect_uri: "http://localhost:8080/callback".to_string(),
//!     scopes: vec!["https://www.googleapis.com/auth/drive.readonly".to_string()],
//!     auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
//!     token_url: "https://oauth2.googleapis.com/token".to_string(),
//! };
//!
//! let flow_manager = OAuthFlowManager::new(config, http_client);
//! let (auth_url, pkce_verifier) = flow_manager.build_auth_url()?;
//! // Redirect user to auth_url...
//! # Ok(())
//! # }
//! ```

use crate::error::{AuthError, Result};
use crate::types::{OAuthTokens, ProviderKind};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use bridge_traits::http::{HttpClient, HttpMethod, HttpRequest};
use bytes::Bytes;
use core_async::time::sleep;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_urlencoded;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{instrument, warn};
use url::Url;

/// OAuth 2.0 provider configuration.
///
/// Contains all necessary information to perform OAuth flows with a specific provider.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// The provider kind (GoogleDrive, OneDrive, etc.)
    pub provider: ProviderKind,
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret (optional for public clients)
    pub client_secret: Option<String>,
    /// Redirect URI for OAuth callback
    pub redirect_uri: String,
    /// List of OAuth scopes to request
    pub scopes: Vec<String>,
    /// Authorization endpoint URL
    pub auth_url: String,
    /// Token endpoint URL
    pub token_url: String,
}

/// PKCE (Proof Key for Code Exchange) verifier.
///
/// Contains the code verifier that must be stored securely during
/// the authorization flow and used when exchanging the authorization code.
///
/// # Security
///
/// The verifier must be kept secret and never transmitted to the authorization server.
/// Only the challenge (derived from the verifier) is sent during authorization.
#[derive(Debug, Clone)]
pub struct PkceVerifier {
    /// The code verifier (base64-url-encoded random string)
    verifier: String,
    /// The state parameter for CSRF protection
    state: String,
}

impl PkceVerifier {
    /// Create a new PKCE verifier with cryptographically secure random values.
    ///
    /// Generates:
    /// - A 32-byte random code verifier (base64-url-encoded)
    /// - A 16-byte random state parameter (base64-url-encoded)
    ///
    /// Both values use URL-safe base64 encoding without padding.
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();

        // Generate code verifier (43-128 characters per RFC 7636)
        let mut verifier_bytes = [0u8; 32];
        rng.fill(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Generate state for CSRF protection
        let mut state_bytes = [0u8; 16];
        rng.fill(&mut state_bytes);
        let state = URL_SAFE_NO_PAD.encode(state_bytes);

        Self { verifier, state }
    }

    /// Get the code verifier string.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Get the state parameter.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// Compute the code challenge from the verifier.
    ///
    /// Uses S256 method: BASE64URL(SHA256(code_verifier))
    pub fn challenge(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.verifier.as_bytes());
        let hash = hasher.finalize();
        URL_SAFE_NO_PAD.encode(hash)
    }
}

impl Default for PkceVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// OAuth 2.0 flow manager.
///
/// Handles the complete OAuth 2.0 authorization code flow with PKCE support.
pub struct OAuthFlowManager {
    config: OAuthConfig,
    http_client: Arc<dyn HttpClient>,
}

impl OAuthFlowManager {
    /// Create a new OAuth flow manager with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth provider configuration
    /// * `http_client` - HTTP client for making token requests
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use core_auth::oauth::{OAuthFlowManager, OAuthConfig};
    /// use core_auth::ProviderKind;
    /// use std::sync::Arc;
    ///
    /// # use bridge_traits::http::HttpClient;
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// let config = OAuthConfig {
    ///     provider: ProviderKind::GoogleDrive,
    ///     client_id: "client-id".to_string(),
    ///     client_secret: Some("secret".to_string()),
    ///     redirect_uri: "http://localhost:8080/callback".to_string(),
    ///     scopes: vec!["https://www.googleapis.com/auth/drive.readonly".to_string()],
    ///     auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
    ///     token_url: "https://oauth2.googleapis.com/token".to_string(),
    /// };
    ///
    /// let manager = OAuthFlowManager::new(config, http_client);
    /// ```
    pub fn new(config: OAuthConfig, http_client: Arc<dyn HttpClient>) -> Self {
        Self {
            config,
            http_client,
        }
    }

    /// Build the authorization URL with PKCE challenge.
    ///
    /// Creates a URL that the user should visit to authorize the application.
    /// Returns both the URL and the PKCE verifier, which must be stored securely
    /// for later use when exchanging the authorization code.
    ///
    /// # Returns
    ///
    /// A tuple of (authorization_url, pkce_verifier)
    ///
    /// # Errors
    ///
    /// Returns an error if the authorization URL cannot be parsed or constructed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::oauth::{OAuthFlowManager, OAuthConfig};
    /// # use core_auth::ProviderKind;
    /// # use std::sync::Arc;
    /// # use bridge_traits::http::HttpClient;
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let config = OAuthConfig {
    /// #     provider: ProviderKind::GoogleDrive,
    /// #     client_id: "client-id".to_string(),
    /// #     client_secret: Some("secret".to_string()),
    /// #     redirect_uri: "http://localhost:8080/callback".to_string(),
    /// #     scopes: vec!["scope1".to_string()],
    /// #     auth_url: "https://provider.com/auth".to_string(),
    /// #     token_url: "https://provider.com/token".to_string(),
    /// # };
    /// # let manager = OAuthFlowManager::new(config, http_client);
    /// let (auth_url, verifier) = manager.build_auth_url().unwrap();
    /// println!("Visit: {}", auth_url);
    /// // Store verifier securely for later use
    /// ```
    #[instrument(skip(self), fields(provider = %self.config.provider))]
    pub fn build_auth_url(&self) -> Result<(String, PkceVerifier)> {
        let verifier = PkceVerifier::new();
        let challenge = verifier.challenge();

        let mut url = Url::parse(&self.config.auth_url)
            .map_err(|e| AuthError::Other(format!("Invalid auth URL: {}", e)))?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("redirect_uri", &self.config.redirect_uri);
            query.append_pair("response_type", "code");
            query.append_pair("scope", &self.config.scopes.join(" "));
            query.append_pair("state", verifier.state());
            query.append_pair("code_challenge", &challenge);
            query.append_pair("code_challenge_method", "S256");
            query.append_pair("access_type", "offline"); // Request refresh token
        }

        tracing::debug!(
            "Built authorization URL for provider {}",
            self.config.provider
        );

        Ok((url.to_string(), verifier))
    }

    /// Exchange an authorization code for OAuth tokens.
    ///
    /// This should be called after the user completes authorization and the
    /// callback receives the authorization code and state.
    ///
    /// # Arguments
    ///
    /// * `code` - The authorization code from the OAuth callback
    /// * `state` - The state parameter from the OAuth callback
    /// * `verifier` - The PKCE verifier from `build_auth_url()`
    ///
    /// # Returns
    ///
    /// The OAuth tokens (access token, refresh token, expiration)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The state doesn't match (CSRF protection)
    /// - The authorization code is invalid
    /// - Network errors occur
    /// - The token endpoint returns an error
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::oauth::{OAuthFlowManager, OAuthConfig, PkceVerifier};
    /// # use core_auth::ProviderKind;
    /// # use std::sync::Arc;
    /// # use bridge_traits::http::HttpClient;
    /// # async fn example() -> core_auth::Result<()> {
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let config = OAuthConfig {
    /// #     provider: ProviderKind::GoogleDrive,
    /// #     client_id: "client-id".to_string(),
    /// #     client_secret: Some("secret".to_string()),
    /// #     redirect_uri: "http://localhost:8080/callback".to_string(),
    /// #     scopes: vec!["scope1".to_string()],
    /// #     auth_url: "https://provider.com/auth".to_string(),
    /// #     token_url: "https://provider.com/token".to_string(),
    /// # };
    /// # let manager = OAuthFlowManager::new(config, http_client);
    /// # let (_, verifier) = manager.build_auth_url()?;
    /// # let callback_code = "code_from_callback";
    /// # let callback_state = verifier.state().to_string();
    /// let tokens = manager.exchange_code(callback_code, &callback_state, &verifier).await?;
    /// println!("Access token expires at: {}", tokens.expires_at);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, code, verifier), fields(provider = %self.config.provider))]
    pub async fn exchange_code(
        &self,
        code: &str,
        state: &str,
        verifier: &PkceVerifier,
    ) -> Result<OAuthTokens> {
        // Verify state to prevent CSRF attacks
        if state != verifier.state() {
            warn!(
                "OAuth state mismatch for provider {}: expected '{}', got '{}'",
                self.config.provider,
                verifier.state(),
                state
            );
            return Err(AuthError::StateMismatch {
                expected: verifier.state().to_string(),
                actual: state.to_string(),
            });
        }

        // Build token request
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", &self.config.redirect_uri);
        params.insert("client_id", &self.config.client_id);
        params.insert("code_verifier", verifier.verifier());

        if let Some(ref client_secret) = self.config.client_secret {
            params.insert("client_secret", client_secret);
        }

        tracing::debug!("Exchanging authorization code for tokens");

        let encoded_body = serde_urlencoded::to_string(&params)
            .map_err(|e| AuthError::Other(format!("Failed to encode token request: {}", e)))?;
        let body = Bytes::from(encoded_body);

        let request = HttpRequest::new(HttpMethod::Post, self.config.token_url.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body);

        let response = self
            .http_client
            .execute(request)
            .await
            .map_err(|e| AuthError::NetworkError(e.to_string()))?;

        if !response.is_success() {
            let status = response.status;
            let error_body = response
                .text()
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            warn!(
                status = status,
                error = %error_body,
                "Token exchange failed while exchanging authorization code"
            );

            return Err(AuthError::InvalidAuthCode(format!(
                "Token endpoint returned {}: {}",
                status, error_body
            )));
        }

        // Parse token response
        let token_response: TokenResponse = response
            .json()
            .map_err(|e| AuthError::Other(format!("Failed to parse token response: {}", e)))?;

        tracing::info!(
            "Successfully exchanged code for tokens (expires in {}s)",
            token_response.expires_in
        );

        Ok(OAuthTokens::new(
            token_response.access_token,
            token_response.refresh_token,
            token_response.expires_in,
        ))
    }

    /// Refresh an access token using a refresh token.
    ///
    /// This should be called when the access token is expired or about to expire.
    /// The refresh token is long-lived and can be used multiple times.
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - The refresh token from previous authentication
    ///
    /// # Returns
    ///
    /// New OAuth tokens with a fresh access token
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The refresh token is invalid or revoked
    /// - Network errors occur
    /// - The token endpoint returns an error
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use core_auth::oauth::{OAuthFlowManager, OAuthConfig};
    /// # use core_auth::ProviderKind;
    /// # use std::sync::Arc;
    /// # use bridge_traits::http::HttpClient;
    /// # async fn example() -> core_auth::Result<()> {
    /// # let http_client: Arc<dyn HttpClient> = todo!();
    /// # let config = OAuthConfig {
    /// #     provider: ProviderKind::GoogleDrive,
    /// #     client_id: "client-id".to_string(),
    /// #     client_secret: Some("secret".to_string()),
    /// #     redirect_uri: "http://localhost:8080/callback".to_string(),
    /// #     scopes: vec!["scope1".to_string()],
    /// #     auth_url: "https://provider.com/auth".to_string(),
    /// #     token_url: "https://provider.com/token".to_string(),
    /// # };
    /// # let manager = OAuthFlowManager::new(config, http_client);
    /// # let old_refresh_token = "refresh_token";
    /// let new_tokens = manager.refresh_access_token(old_refresh_token).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, refresh_token), fields(provider = %self.config.provider))]
    pub async fn refresh_access_token(&self, refresh_token: &str) -> Result<OAuthTokens> {
        // Build refresh request
        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);
        params.insert("client_id", &self.config.client_id);

        if let Some(ref client_secret) = self.config.client_secret {
            params.insert("client_secret", client_secret);
        }

        tracing::debug!("Refreshing access token");

        let encoded_body = serde_urlencoded::to_string(&params)
            .map_err(|e| AuthError::Other(format!("Failed to encode token request: {}", e)))?;
        let body = Bytes::from(encoded_body);

        let mut attempts = 0;
        const MAX_RETRIES: u32 = 3;

        loop {
            attempts += 1;

            let request = HttpRequest::new(HttpMethod::Post, self.config.token_url.clone())
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body.clone());

            let response = self
                .http_client
                .execute(request)
                .await
                .map_err(|e| AuthError::TokenRefreshFailed(e.to_string()))?;

            if response.is_success() {
                let token_response: TokenResponse = response.json().map_err(|e| {
                    AuthError::Other(format!("Failed to parse token response: {}", e))
                })?;

                tracing::info!(
                    "Successfully refreshed token (expires in {}s)",
                    token_response.expires_in
                );

                return Ok(OAuthTokens::new(
                    token_response.access_token,
                    token_response
                        .refresh_token
                        .or_else(|| Some(refresh_token.to_string())),
                    token_response.expires_in,
                ));
            }

            let status = response.status;

            if (400..500).contains(&status) {
                let error_body = response
                    .text()
                    .unwrap_or_else(|_| "Unable to read error response".to_string());

                warn!(
                    status = status,
                    error = %error_body,
                    "Token refresh failed without retry"
                );

                return Err(AuthError::TokenRefreshFailed(format!(
                    "Token endpoint returned {}: {}",
                    status, error_body
                )));
            }

            if attempts >= MAX_RETRIES {
                let error_body = response
                    .text()
                    .unwrap_or_else(|_| "Unable to read error response".to_string());

                return Err(AuthError::TokenRefreshFailed(format!(
                    "Token refresh failed after {} attempts. Last error: {} - {}",
                    attempts, status, error_body
                )));
            }

            let delay = Duration::from_millis(100 * 2u64.pow(attempts - 1));
            warn!(
                status = status,
                attempts = attempts,
                delay_ms = delay.as_millis(),
                "Token refresh failed, retrying"
            );
            sleep(delay).await;
        }
    }
}

/// Token response from the OAuth provider.
///
/// This structure represents the JSON response from the token endpoint.
#[derive(Debug, Deserialize, Serialize)]
struct TokenResponse {
    access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default = "default_expires_in")]
    expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
}

fn default_expires_in() -> i64 {
    3600 // Default to 1 hour if not specified
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::error::{BridgeError, Result as BridgeResult};
    use bridge_traits::http::{HttpClient, HttpRequest, HttpResponse};
    use core_async::io::AsyncRead;
    use std::sync::Arc;

    #[derive(Default)]
    struct StubHttpClient;

    #[async_trait::async_trait]
    impl HttpClient for StubHttpClient {
        async fn execute(&self, _request: HttpRequest) -> BridgeResult<HttpResponse> {
            Err(BridgeError::OperationFailed(
                "HTTP client not mocked for unit test".to_string(),
            ))
        }

        async fn download_stream(
            &self,
            _url: String,
        ) -> BridgeResult<Box<dyn AsyncRead + Send + Unpin>> {
            Err(BridgeError::OperationFailed(
                "HTTP client not mocked for unit test".to_string(),
            ))
        }
    }

    #[test]
    fn test_pkce_verifier_generation() {
        let verifier = PkceVerifier::new();

        // Verifier should be non-empty
        assert!(!verifier.verifier().is_empty());
        assert!(!verifier.state().is_empty());

        // Challenge should be deterministic for same verifier
        let challenge1 = verifier.challenge();
        let challenge2 = verifier.challenge();
        assert_eq!(challenge1, challenge2);

        // Different verifiers should produce different values
        let verifier2 = PkceVerifier::new();
        assert_ne!(verifier.verifier(), verifier2.verifier());
        assert_ne!(verifier.state(), verifier2.state());
        assert_ne!(verifier.challenge(), verifier2.challenge());
    }

    #[test]
    fn test_pkce_challenge_computation() {
        // Test with known verifier to verify SHA256 + base64 computation
        let verifier = PkceVerifier {
            verifier: "test_verifier".to_string(),
            state: "test_state".to_string(),
        };

        let challenge = verifier.challenge();

        // Verify challenge is base64-url-encoded
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        assert!(!challenge.contains('='));

        // Verify it's reproducible
        assert_eq!(challenge, verifier.challenge());
    }

    #[test]
    fn test_oauth_config_creation() {
        let config = OAuthConfig {
            provider: ProviderKind::GoogleDrive,
            client_id: "test-client".to_string(),
            client_secret: Some("secret".to_string()),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["scope1".to_string(), "scope2".to_string()],
            auth_url: "https://provider.com/auth".to_string(),
            token_url: "https://provider.com/token".to_string(),
        };

        assert_eq!(config.client_id, "test-client");
        assert_eq!(config.scopes.len(), 2);
    }

    #[test]
    fn test_build_auth_url() {
        let config = OAuthConfig {
            provider: ProviderKind::GoogleDrive,
            client_id: "test-client".to_string(),
            client_secret: Some("secret".to_string()),
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec!["scope1".to_string(), "scope2".to_string()],
            auth_url: "https://provider.com/auth".to_string(),
            token_url: "https://provider.com/token".to_string(),
        };

        let manager = OAuthFlowManager::new(config, Arc::new(StubHttpClient::default()));
        let result = manager.build_auth_url();

        assert!(result.is_ok());
        let (url, verifier) = result.unwrap();

        // Verify URL contains required parameters
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("redirect_uri=http"));
        assert!(url.contains("response_type=code"));
        // URL encoding can use either + or %20 for spaces - both are valid
        assert!(url.contains("scope=scope1+scope2") || url.contains("scope=scope1%20scope2"));
        assert!(url.contains(&format!("state={}", verifier.state())));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("access_type=offline"));
    }

    #[test]
    fn test_build_auth_url_invalid_url() {
        let config = OAuthConfig {
            provider: ProviderKind::GoogleDrive,
            client_id: "test-client".to_string(),
            client_secret: None,
            redirect_uri: "http://localhost:8080/callback".to_string(),
            scopes: vec![],
            auth_url: "not a valid url".to_string(),
            token_url: "https://provider.com/token".to_string(),
        };

        let manager = OAuthFlowManager::new(config, Arc::new(StubHttpClient::default()));
        let result = manager.build_auth_url();

        assert!(result.is_err());
    }

    #[test]
    fn test_state_verification() {
        // This would be tested in the exchange_code method
        // Here we just verify the state is included in the verifier
        let verifier = PkceVerifier::new();
        let state = verifier.state();

        assert!(!state.is_empty());
        assert_eq!(state, verifier.state());
    }

    #[test]
    fn test_token_response_deserialization() {
        let json = r#"{
            "access_token": "ya29.a0...",
            "refresh_token": "1//0g...",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;

        let response: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.access_token, "ya29.a0...");
        assert_eq!(response.refresh_token, Some("1//0g...".to_string()));
        assert_eq!(response.expires_in, 3600);
    }

    #[test]
    fn test_token_response_deserialization_minimal() {
        let json = r#"{
            "access_token": "token"
        }"#;

        let response: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.access_token, "token");
        assert_eq!(response.refresh_token, None);
        assert_eq!(response.expires_in, 3600); // Default value
    }
}
