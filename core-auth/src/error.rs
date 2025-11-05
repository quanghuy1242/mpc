//! Error types for the authentication module.
//!
//! This module defines all error types that can occur during authentication flows,
//! token management, and credential storage operations.

use thiserror::Error;

/// Authentication error types.
///
/// All authentication errors provide descriptive messages with context
/// to help diagnose and fix issues.
///
/// # Examples
///
/// ```
/// use core_auth::{AuthError, Result};
///
/// fn check_auth() -> Result<()> {
///     Err(AuthError::NotAuthenticated)
/// }
/// ```
#[derive(Error, Debug)]
pub enum AuthError {
    /// Authentication with the provider failed.
    ///
    /// This occurs when the OAuth flow fails, typically due to:
    /// - User denied access
    /// - Invalid credentials
    /// - Network errors during authentication
    #[error("Provider {provider} authentication failed: {reason}")]
    AuthenticationFailed {
        /// The provider that failed to authenticate
        provider: String,
        /// Detailed reason for the failure
        reason: String,
    },

    /// Token refresh operation failed.
    ///
    /// This occurs when attempting to refresh an expired access token.
    /// Common causes:
    /// - Refresh token has been revoked
    /// - Network error during refresh
    /// - Provider API is unavailable
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),

    /// Secure storage is unavailable or misconfigured.
    ///
    /// This occurs when the platform's secure storage (Keychain, Keystore, etc.)
    /// cannot be accessed. Platform-specific guidance:
    /// - **Desktop**: Ensure keyring crate is properly configured
    /// - **iOS**: Check keychain entitlements
    /// - **Android**: Verify EncryptedSharedPreferences setup
    /// - **Web**: Check WebCrypto API availability
    #[error("Secure storage unavailable: {0}")]
    SecureStorageUnavailable(String),

    /// The specified provider is invalid or not supported.
    ///
    /// This occurs when:
    /// - Provider name is misspelled
    /// - Provider is not enabled in configuration
    /// - Provider feature flag is disabled
    #[error("Invalid provider: {0}")]
    InvalidProvider(String),

    /// User is not authenticated.
    ///
    /// This occurs when attempting an operation that requires authentication
    /// but no valid session exists.
    #[error("Not authenticated")]
    NotAuthenticated,

    /// Profile was not found.
    ///
    /// This occurs when attempting to access a profile that doesn't exist
    /// or has been deleted.
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    /// OAuth state mismatch during authentication flow.
    ///
    /// This is a security error indicating potential CSRF attack or
    /// corrupted authentication flow.
    #[error("OAuth state mismatch (expected: {expected}, got: {actual})")]
    StateMismatch {
        /// Expected state value
        expected: String,
        /// Actual state value received
        actual: String,
    },

    /// Invalid authorization code.
    ///
    /// This occurs when the authorization code from the provider
    /// is invalid or has expired.
    #[error("Invalid authorization code: {0}")]
    InvalidAuthCode(String),

    /// Network error during authentication.
    ///
    /// This wraps underlying HTTP errors from network operations.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Token has expired and cannot be refreshed.
    ///
    /// This occurs when the refresh token is also expired or invalid.
    /// User must re-authenticate.
    #[error("Token expired and cannot be refreshed")]
    TokenExpired,

    /// Bridge trait implementation error.
    ///
    /// This occurs when a required host bridge (HttpClient, SecureStore, etc.)
    /// returns an error or is unavailable.
    #[error("Bridge error: {0}")]
    BridgeError(String),

    /// Token data is corrupted or invalid.
    ///
    /// This occurs when stored token data cannot be deserialized,
    /// indicating corruption or incompatible format changes.
    #[error("Token data corrupted for profile {profile_id}: {reason}")]
    TokenCorrupted {
        /// The profile whose tokens are corrupted
        profile_id: crate::types::ProfileId,
        /// Reason for corruption detection
        reason: String,
    },

    /// Serialization/deserialization error during token storage.
    ///
    /// This occurs when token data cannot be serialized for storage.
    #[error("Serialization failed ({context}): {source}")]
    SerializationFailed {
        /// Context where serialization failed
        context: String,
        /// Underlying error
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Serialization/deserialization error.
    ///
    /// This occurs when token data cannot be serialized or deserialized.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Generic error for unexpected failures.
    #[error("Authentication error: {0}")]
    Other(String),
}

// Implement From conversions for common error types
impl From<bridge_traits::error::BridgeError> for AuthError {
    fn from(err: bridge_traits::error::BridgeError) -> Self {
        AuthError::BridgeError(err.to_string())
    }
}

/// Result type alias for authentication operations.
///
/// This is a convenience type alias that uses [`AuthError`] as the error type.
///
/// # Examples
///
/// ```
/// use core_auth::Result;
///
/// fn authenticate() -> Result<String> {
///     Ok("profile_id".to_string())
/// }
/// ```
pub type Result<T> = std::result::Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_authentication_failed() {
        let err = AuthError::AuthenticationFailed {
            provider: "GoogleDrive".to_string(),
            reason: "User denied access".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("GoogleDrive"));
        assert!(msg.contains("User denied access"));
    }

    #[test]
    fn test_error_display_token_refresh_failed() {
        let err = AuthError::TokenRefreshFailed("Network timeout".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Token refresh failed"));
        assert!(msg.contains("Network timeout"));
    }

    #[test]
    fn test_error_display_secure_storage_unavailable() {
        let err = AuthError::SecureStorageUnavailable("Keychain not available".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Secure storage unavailable"));
        assert!(msg.contains("Keychain not available"));
    }

    #[test]
    fn test_error_display_invalid_provider() {
        let err = AuthError::InvalidProvider("unknown_provider".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid provider"));
        assert!(msg.contains("unknown_provider"));
    }

    #[test]
    fn test_error_display_not_authenticated() {
        let err = AuthError::NotAuthenticated;
        let msg = format!("{}", err);
        assert_eq!(msg, "Not authenticated");
    }

    #[test]
    fn test_error_display_profile_not_found() {
        let err = AuthError::ProfileNotFound("profile-123".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Profile not found"));
        assert!(msg.contains("profile-123"));
    }

    #[test]
    fn test_error_display_state_mismatch() {
        let err = AuthError::StateMismatch {
            expected: "abc123".to_string(),
            actual: "xyz789".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("OAuth state mismatch"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("xyz789"));
    }

    #[test]
    fn test_error_display_invalid_auth_code() {
        let err = AuthError::InvalidAuthCode("expired".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid authorization code"));
        assert!(msg.contains("expired"));
    }

    #[test]
    fn test_error_display_network_error() {
        let err = AuthError::NetworkError("Connection refused".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Network error"));
        assert!(msg.contains("Connection refused"));
    }

    #[test]
    fn test_error_display_token_expired() {
        let err = AuthError::TokenExpired;
        let msg = format!("{}", err);
        assert_eq!(msg, "Token expired and cannot be refreshed");
    }

    #[test]
    fn test_error_display_bridge_error() {
        let err = AuthError::BridgeError("HTTP client unavailable".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Bridge error"));
        assert!(msg.contains("HTTP client unavailable"));
    }

    #[test]
    fn test_error_from_serde_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let auth_err: AuthError = json_err.into();
        assert!(matches!(auth_err, AuthError::SerializationError(_)));
    }

    #[test]
    fn test_result_type_ok() {
        let result: Result<String> = Ok("success".to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_result_type_err() {
        let result: Result<String> = Err(AuthError::NotAuthenticated);
        assert!(result.is_err());
    }
}
