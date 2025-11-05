use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for an authenticated user profile.
///
/// Each profile represents a single authenticated user account
/// for a specific cloud storage provider. A user can have multiple
/// profiles (e.g., multiple Google Drive accounts).
///
/// # Examples
///
/// ```
/// use core_auth::ProfileId;
///
/// // Create a new profile ID
/// let profile_id = ProfileId::new();
///
/// // Parse from string
/// let id_str = "550e8400-e29b-41d4-a716-446655440000";
/// let profile_id = ProfileId::from_string(id_str).unwrap();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(Uuid);

impl ProfileId {
    /// Create a new random profile ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a profile ID from a string
    ///
    /// # Arguments
    ///
    /// * `s` - UUID string representation
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::ProfileId;
    ///
    /// let id = ProfileId::from_string("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// ```
    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the inner UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ProfileId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Supported cloud storage providers.
///
/// Each provider has its own OAuth 2.0 configuration and API endpoints.
///
/// # Examples
///
/// ```
/// use core_auth::ProviderKind;
///
/// let provider = ProviderKind::GoogleDrive;
/// assert_eq!(provider.display_name(), "Google Drive");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    /// Google Drive cloud storage
    GoogleDrive,
    /// Microsoft OneDrive cloud storage
    OneDrive,
}

impl ProviderKind {
    /// Get the human-readable display name for this provider
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::ProviderKind;
    ///
    /// assert_eq!(ProviderKind::GoogleDrive.display_name(), "Google Drive");
    /// assert_eq!(ProviderKind::OneDrive.display_name(), "OneDrive");
    /// ```
    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderKind::GoogleDrive => "Google Drive",
            ProviderKind::OneDrive => "OneDrive",
        }
    }

    /// Get the provider identifier string
    ///
    /// Used for logging and configuration purposes.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::ProviderKind;
    ///
    /// assert_eq!(ProviderKind::GoogleDrive.as_str(), "google_drive");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::GoogleDrive => "google_drive",
            ProviderKind::OneDrive => "onedrive",
        }
    }

    /// Parse a provider kind from a string identifier
    ///
    /// # Arguments
    ///
    /// * `s` - Provider identifier string
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::ProviderKind;
    ///
    /// assert_eq!(ProviderKind::parse("google_drive"), Some(ProviderKind::GoogleDrive));
    /// assert_eq!(ProviderKind::parse("invalid"), None);
    /// ```
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "google_drive" | "googledrive" => Some(ProviderKind::GoogleDrive),
            "onedrive" | "one_drive" => Some(ProviderKind::OneDrive),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// OAuth 2.0 token set.
///
/// Contains the access token, refresh token, and expiration time
/// for an authenticated session.
///
/// # Security
///
/// Tokens should be stored securely and never logged. The `Debug` implementation
/// redacts sensitive information.
///
/// # Examples
///
/// ```
/// use core_auth::OAuthTokens;
/// use chrono::{Duration, Utc};
///
/// let tokens = OAuthTokens {
///     access_token: "ya29.a0...".to_string(),
///     refresh_token: "1//0g...".to_string(),
///     expires_at: Utc::now() + Duration::hours(1),
/// };
///
/// // Check if token is expired
/// assert!(!tokens.is_expired());
/// ```
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// The access token used for API requests
    pub access_token: String,
    /// The refresh token used to obtain new access tokens
    pub refresh_token: String,
    /// When the access token expires (UTC)
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl OAuthTokens {
    /// Create a new token set
    ///
    /// # Arguments
    ///
    /// * `access_token` - The OAuth access token
    /// * `refresh_token` - The OAuth refresh token
    /// * `expires_in` - Number of seconds until token expiration
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::OAuthTokens;
    ///
    /// let tokens = OAuthTokens::new(
    ///     "access_token".to_string(),
    ///     "refresh_token".to_string(),
    ///     3600, // 1 hour
    /// );
    /// ```
    pub fn new(access_token: String, refresh_token: String, expires_in: i64) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(expires_in),
        }
    }

    /// Check if the access token is expired or will expire soon
    ///
    /// Returns `true` if the token is expired or will expire within the buffer period.
    /// The buffer period ensures we refresh tokens before they actually expire.
    ///
    /// # Arguments
    ///
    /// * `buffer_seconds` - Number of seconds before expiration to consider expired (default: 300)
    ///
    /// # Examples
    ///
    /// ```
    /// use core_auth::OAuthTokens;
    /// use chrono::{Duration, Utc};
    ///
    /// let tokens = OAuthTokens {
    ///     access_token: "token".to_string(),
    ///     refresh_token: "refresh".to_string(),
    ///     expires_at: Utc::now() + Duration::minutes(10),
    /// };
    ///
    /// assert!(!tokens.is_expired());
    /// assert!(!tokens.is_expired_with_buffer(60)); // 1 minute buffer
    /// ```
    pub fn is_expired(&self) -> bool {
        self.is_expired_with_buffer(300)
    }

    /// Check if the access token is expired with a custom buffer
    ///
    /// # Arguments
    ///
    /// * `buffer_seconds` - Number of seconds before expiration to consider expired
    pub fn is_expired_with_buffer(&self, buffer_seconds: i64) -> bool {
        let now = chrono::Utc::now();
        let buffer = chrono::Duration::seconds(buffer_seconds);
        now >= self.expires_at - buffer
    }

    /// Get the time remaining until token expiration
    ///
    /// Returns `None` if the token is already expired.
    pub fn time_until_expiry(&self) -> Option<chrono::Duration> {
        let now = chrono::Utc::now();
        if now >= self.expires_at {
            None
        } else {
            Some(self.expires_at - now)
        }
    }
}

// Custom Debug implementation to avoid logging tokens
impl fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Authentication state for a profile.
///
/// Tracks the current state of the authentication flow.
///
/// # State Transitions
///
/// ```text
/// SignedOut -> SigningIn -> SignedIn
///                             ^  |
///                             |  v
///                      TokenRefreshing
/// ```
///
/// # Examples
///
/// ```
/// use core_auth::AuthState;
///
/// let state = AuthState::SignedOut;
/// assert!(state.is_authenticated() == false);
///
/// let state = AuthState::SignedIn;
/// assert!(state.is_authenticated() == true);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthState {
    /// User is not authenticated
    #[default]
    SignedOut,
    /// Authentication flow is in progress
    SigningIn,
    /// User is authenticated with valid tokens
    SignedIn,
    /// Token refresh is in progress
    TokenRefreshing,
}

impl AuthState {
    /// Check if the user is authenticated (has valid credentials)
    ///
    /// Returns `true` for `SignedIn` and `TokenRefreshing` states.
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthState::SignedIn | AuthState::TokenRefreshing)
    }

    /// Check if an operation is in progress
    ///
    /// Returns `true` for `SigningIn` and `TokenRefreshing` states.
    pub fn is_in_progress(&self) -> bool {
        matches!(self, AuthState::SigningIn | AuthState::TokenRefreshing)
    }
}

impl fmt::Display for AuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthState::SignedOut => write!(f, "Signed Out"),
            AuthState::SigningIn => write!(f, "Signing In..."),
            AuthState::SignedIn => write!(f, "Signed In"),
            AuthState::TokenRefreshing => write!(f, "Refreshing Token..."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_profile_id_creation() {
        let id1 = ProfileId::new();
        let id2 = ProfileId::new();
        assert_ne!(id1, id2, "Profile IDs should be unique");
    }

    #[test]
    fn test_profile_id_from_string() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = ProfileId::from_string(uuid_str).unwrap();
        assert_eq!(id.to_string(), uuid_str);
    }

    #[test]
    fn test_profile_id_from_string_invalid() {
        let result = ProfileId::from_string("invalid-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_id_display() {
        let id = ProfileId::new();
        let display = format!("{}", id);
        assert!(uuid::Uuid::parse_str(&display).is_ok());
    }

    #[test]
    fn test_profile_id_serialization() {
        let id = ProfileId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: ProfileId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_provider_kind_display_name() {
        assert_eq!(ProviderKind::GoogleDrive.display_name(), "Google Drive");
        assert_eq!(ProviderKind::OneDrive.display_name(), "OneDrive");
    }

    #[test]
    fn test_provider_kind_as_str() {
        assert_eq!(ProviderKind::GoogleDrive.as_str(), "google_drive");
        assert_eq!(ProviderKind::OneDrive.as_str(), "onedrive");
    }

    #[test]
    fn test_provider_kind_from_str() {
        assert_eq!(
            ProviderKind::parse("google_drive"),
            Some(ProviderKind::GoogleDrive)
        );
        assert_eq!(
            ProviderKind::parse("GoogleDrive"),
            Some(ProviderKind::GoogleDrive)
        );
        assert_eq!(
            ProviderKind::parse("onedrive"),
            Some(ProviderKind::OneDrive)
        );
        assert_eq!(
            ProviderKind::parse("one_drive"),
            Some(ProviderKind::OneDrive)
        );
        assert_eq!(ProviderKind::parse("invalid"), None);
    }

    #[test]
    fn test_provider_kind_display() {
        assert_eq!(format!("{}", ProviderKind::GoogleDrive), "Google Drive");
        assert_eq!(format!("{}", ProviderKind::OneDrive), "OneDrive");
    }

    #[test]
    fn test_provider_kind_serialization() {
        let provider = ProviderKind::GoogleDrive;
        let json = serde_json::to_string(&provider).unwrap();
        let deserialized: ProviderKind = serde_json::from_str(&json).unwrap();
        assert_eq!(provider, deserialized);
    }

    #[test]
    fn test_oauth_tokens_new() {
        let tokens = OAuthTokens::new("access".to_string(), "refresh".to_string(), 3600);
        assert_eq!(tokens.access_token, "access");
        assert_eq!(tokens.refresh_token, "refresh");
        assert!(tokens.time_until_expiry().is_some());
    }

    #[test]
    fn test_oauth_tokens_is_expired_fresh() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        assert!(!tokens.is_expired());
    }

    #[test]
    fn test_oauth_tokens_is_expired_within_buffer() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() + Duration::seconds(200), // Less than default buffer
        };
        assert!(tokens.is_expired());
    }

    #[test]
    fn test_oauth_tokens_is_expired_past() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() - Duration::hours(1),
        };
        assert!(tokens.is_expired());
    }

    #[test]
    fn test_oauth_tokens_is_expired_with_buffer() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() + Duration::minutes(10),
        };
        assert!(!tokens.is_expired_with_buffer(60)); // 1 minute buffer
        assert!(tokens.is_expired_with_buffer(600)); // 10 minute buffer
    }

    #[test]
    fn test_oauth_tokens_time_until_expiry() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        let remaining = tokens.time_until_expiry().unwrap();
        assert!(remaining.num_minutes() >= 59 && remaining.num_minutes() <= 60);
    }

    #[test]
    fn test_oauth_tokens_time_until_expiry_expired() {
        let tokens = OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Utc::now() - Duration::hours(1),
        };
        assert!(tokens.time_until_expiry().is_none());
    }

    #[test]
    fn test_oauth_tokens_debug_redacts() {
        let tokens = OAuthTokens {
            access_token: "secret_access_token".to_string(),
            refresh_token: "secret_refresh_token".to_string(),
            expires_at: Utc::now(),
        };
        let debug_str = format!("{:?}", tokens);
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("secret_access_token"));
        assert!(!debug_str.contains("secret_refresh_token"));
    }

    #[test]
    fn test_oauth_tokens_serialization() {
        let tokens = OAuthTokens::new("access".to_string(), "refresh".to_string(), 3600);
        let json = serde_json::to_string(&tokens).unwrap();
        let deserialized: OAuthTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(tokens.access_token, deserialized.access_token);
        assert_eq!(tokens.refresh_token, deserialized.refresh_token);
    }

    #[test]
    fn test_auth_state_is_authenticated() {
        assert!(!AuthState::SignedOut.is_authenticated());
        assert!(!AuthState::SigningIn.is_authenticated());
        assert!(AuthState::SignedIn.is_authenticated());
        assert!(AuthState::TokenRefreshing.is_authenticated());
    }

    #[test]
    fn test_auth_state_is_in_progress() {
        assert!(!AuthState::SignedOut.is_in_progress());
        assert!(AuthState::SigningIn.is_in_progress());
        assert!(!AuthState::SignedIn.is_in_progress());
        assert!(AuthState::TokenRefreshing.is_in_progress());
    }

    #[test]
    fn test_auth_state_default() {
        assert_eq!(AuthState::default(), AuthState::SignedOut);
    }

    #[test]
    fn test_auth_state_display() {
        assert_eq!(format!("{}", AuthState::SignedOut), "Signed Out");
        assert_eq!(format!("{}", AuthState::SigningIn), "Signing In...");
        assert_eq!(format!("{}", AuthState::SignedIn), "Signed In");
        assert_eq!(
            format!("{}", AuthState::TokenRefreshing),
            "Refreshing Token..."
        );
    }

    #[test]
    fn test_auth_state_serialization() {
        let state = AuthState::SignedIn;
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: AuthState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }
}
