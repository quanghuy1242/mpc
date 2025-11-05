//! Secure Token Storage
//!
//! This module provides secure persistence for OAuth tokens using platform-specific
//! secure storage mechanisms (Keychain, Keystore, etc.).
//!
//! ## Security Features
//!
//! - Tokens are never logged or exposed in error messages
//! - Storage uses platform-specific secure stores (via `SecureStore` trait)
//! - Automatic token rotation support
//! - Secure erasure on deletion
//! - Audit logging without exposing sensitive data
//!
//! ## Example
//!
//! ```no_run
//! use core_auth::{TokenStore, ProfileId, OAuthTokens};
//! use std::sync::Arc;
//! # use bridge_traits::storage::SecureStore;
//! # async fn example(secure_store: Arc<dyn SecureStore>) -> core_auth::Result<()> {
//! let token_store = TokenStore::new(secure_store);
//!
//! let profile_id = ProfileId::new();
//! let tokens = OAuthTokens::new(
//!     "access_token_value".to_string(),
//!     Some("refresh_token_value".to_string()),
//!     3600,
//! );
//!
//! // Store tokens
//! token_store.store_tokens(profile_id, &tokens).await?;
//!
//! // Retrieve tokens
//! let retrieved = token_store.retrieve_tokens(profile_id).await?;
//!
//! // Delete tokens
//! token_store.delete_tokens(profile_id).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{AuthError, Result};
use crate::types::{OAuthTokens, ProfileId};
use bridge_traits::storage::SecureStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Secure storage for OAuth tokens
///
/// This struct provides a safe interface for storing and retrieving
/// OAuth tokens using platform-specific secure storage mechanisms.
///
/// # Security Considerations
///
/// - Tokens are serialized to JSON before storage
/// - All operations use the underlying `SecureStore` trait
/// - Token values are never logged
/// - Failed operations are audited without exposing sensitive data
#[derive(Clone)]
pub struct TokenStore {
    secure_store: Arc<dyn SecureStore>,
}

/// Serializable wrapper for OAuth tokens
///
/// This internal struct is used for JSON serialization of tokens
/// before storing them in the secure store.
#[derive(Debug, Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: i64,
}

impl TokenStore {
    /// Create a new token store
    ///
    /// # Arguments
    ///
    /// * `secure_store` - Platform-specific secure storage implementation
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::TokenStore;
    /// # use std::sync::Arc;
    /// # use bridge_traits::storage::SecureStore;
    /// # fn example(secure_store: Arc<dyn SecureStore>) {
    /// let token_store = TokenStore::new(secure_store);
    /// # }
    /// ```
    pub fn new(secure_store: Arc<dyn SecureStore>) -> Self {
        debug!("Initializing TokenStore");
        Self { secure_store }
    }

    /// Store OAuth tokens for a profile
    ///
    /// Tokens are serialized to JSON and stored securely using the
    /// platform-specific secure store. If tokens already exist for
    /// this profile, they are securely overwritten.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile identifier
    /// * `tokens` - The OAuth tokens to store
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if:
    /// - The secure store is unavailable
    /// - Serialization fails
    /// - The underlying storage operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::{TokenStore, ProfileId, OAuthTokens};
    /// # use std::sync::Arc;
    /// # use bridge_traits::storage::SecureStore;
    /// # async fn example(token_store: TokenStore, profile_id: ProfileId) -> core_auth::Result<()> {
    /// let tokens = OAuthTokens::new(
    ///     "access_token".to_string(),
    ///     Some("refresh_token".to_string()),
    ///     3600,
    /// );
    ///
    /// token_store.store_tokens(profile_id, &tokens).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn store_tokens(&self, profile_id: ProfileId, tokens: &OAuthTokens) -> Result<()> {
        let key = self.storage_key(profile_id);

        // Convert to storable format
        let stored = StoredTokens {
            access_token: tokens.access_token().to_string(),
            refresh_token: tokens.refresh_token().map(|s| s.to_string()),
            expires_at: tokens.expires_at(),
        };

        // Serialize to JSON
        let json = serde_json::to_vec(&stored).map_err(|e| {
            warn!(
                profile_id = %profile_id,
                error = %e,
                "Failed to serialize tokens"
            );
            AuthError::SerializationFailed {
                context: "token serialization".to_string(),
                source: e.into(),
            }
        })?;

        // Store in secure storage
        self.secure_store
            .set_secret(&key, &json)
            .await
            .map_err(|e| {
                warn!(
                    profile_id = %profile_id,
                    error = %e,
                    "Failed to store tokens in secure storage"
                );
                AuthError::SecureStorageUnavailable(e.to_string())
            })?;

        info!(
            profile_id = %profile_id,
            has_refresh_token = stored.refresh_token.is_some(),
            "Tokens stored securely"
        );

        Ok(())
    }

    /// Retrieve OAuth tokens for a profile
    ///
    /// Attempts to retrieve and deserialize tokens from the secure store.
    /// If the tokens are corrupted or invalid, they are automatically deleted.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile identifier
    ///
    /// # Returns
    ///
    /// Returns:
    /// - `Ok(Some(tokens))` if tokens exist and are valid
    /// - `Ok(None)` if no tokens exist for this profile
    /// - `Err` if the secure store is unavailable
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::{TokenStore, ProfileId};
    /// # async fn example(token_store: TokenStore, profile_id: ProfileId) -> core_auth::Result<()> {
    /// match token_store.retrieve_tokens(profile_id).await? {
    ///     Some(tokens) => println!("Found tokens"),
    ///     None => println!("No tokens found"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve_tokens(&self, profile_id: ProfileId) -> Result<Option<OAuthTokens>> {
        let key = self.storage_key(profile_id);

        // Retrieve from secure storage
        let data = self.secure_store.get_secret(&key).await.map_err(|e| {
            warn!(
                profile_id = %profile_id,
                error = %e,
                "Failed to retrieve tokens from secure storage"
            );
            AuthError::SecureStorageUnavailable(e.to_string())
        })?;

        // Handle case where no tokens exist
        let Some(data) = data else {
            debug!(profile_id = %profile_id, "No tokens found in storage");
            return Ok(None);
        };

        // Deserialize tokens
        let stored: StoredTokens = match serde_json::from_slice(&data) {
            Ok(tokens) => tokens,
            Err(e) => {
                warn!(
                    profile_id = %profile_id,
                    error = %e,
                    "Failed to deserialize tokens, they may be corrupted"
                );

                // Attempt to delete corrupted data
                if let Err(delete_err) = self.secure_store.delete_secret(&key).await {
                    warn!(
                        profile_id = %profile_id,
                        error = %delete_err,
                        "Failed to delete corrupted token data"
                    );
                }

                return Err(AuthError::TokenCorrupted {
                    profile_id,
                    reason: e.to_string(),
                });
            }
        };

        // Convert to OAuthTokens
        let tokens =
            OAuthTokens::from_parts(stored.access_token, stored.refresh_token, stored.expires_at);

        info!(
            profile_id = %profile_id,
            has_refresh_token = tokens.refresh_token().is_some(),
            expires_at = stored.expires_at,
            "Tokens retrieved successfully"
        );

        Ok(Some(tokens))
    }

    /// Delete OAuth tokens for a profile
    ///
    /// Securely erases tokens from storage. This operation is idempotent
    /// and succeeds even if no tokens exist for the profile.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile identifier
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the secure store is unavailable.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::{TokenStore, ProfileId};
    /// # async fn example(token_store: TokenStore, profile_id: ProfileId) -> core_auth::Result<()> {
    /// token_store.delete_tokens(profile_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_tokens(&self, profile_id: ProfileId) -> Result<()> {
        let key = self.storage_key(profile_id);

        self.secure_store.delete_secret(&key).await.map_err(|e| {
            warn!(
                profile_id = %profile_id,
                error = %e,
                "Failed to delete tokens from secure storage"
            );
            AuthError::SecureStorageUnavailable(e.to_string())
        })?;

        info!(profile_id = %profile_id, "Tokens deleted securely");

        Ok(())
    }

    /// Check if tokens exist for a profile
    ///
    /// This method checks for token existence without retrieving or
    /// deserializing them, making it more efficient than `retrieve_tokens`.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile identifier
    ///
    /// # Returns
    ///
    /// Returns `true` if tokens exist, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::{TokenStore, ProfileId};
    /// # async fn example(token_store: TokenStore, profile_id: ProfileId) -> core_auth::Result<()> {
    /// if token_store.has_tokens(profile_id).await? {
    ///     println!("Tokens exist for profile");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn has_tokens(&self, profile_id: ProfileId) -> Result<bool> {
        let key = self.storage_key(profile_id);

        self.secure_store.has_secret(&key).await.map_err(|e| {
            warn!(
                profile_id = %profile_id,
                error = %e,
                "Failed to check token existence in secure storage"
            );
            AuthError::SecureStorageUnavailable(e.to_string())
        })
    }

    /// List all profile IDs that have stored tokens
    ///
    /// This method scans the secure store for all token keys and extracts
    /// the profile IDs. It does not retrieve or validate the tokens themselves.
    ///
    /// # Returns
    ///
    /// Returns a list of profile IDs that have tokens stored.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::TokenStore;
    /// # async fn example(token_store: TokenStore) -> core_auth::Result<()> {
    /// let profiles = token_store.list_profiles().await?;
    /// println!("Found {} profiles with stored tokens", profiles.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_profiles(&self) -> Result<Vec<ProfileId>> {
        let keys = self.secure_store.list_keys().await.map_err(|e| {
            warn!(error = %e, "Failed to list keys from secure storage");
            AuthError::SecureStorageUnavailable(e.to_string())
        })?;

        let profiles: Vec<ProfileId> = keys
            .iter()
            .filter_map(|key| {
                // Extract profile ID from key (format: "oauth_tokens:<uuid>")
                key.strip_prefix("oauth_tokens:")
                    .and_then(|id_str| ProfileId::from_string(id_str).ok())
            })
            .collect();

        debug!(count = profiles.len(), "Listed profiles with stored tokens");

        Ok(profiles)
    }

    /// Rotate tokens for a profile
    ///
    /// This is a convenience method that stores new tokens and returns
    /// the old tokens (if they existed). This is useful for maintaining
    /// an audit trail of token rotation.
    ///
    /// # Arguments
    ///
    /// * `profile_id` - The profile identifier
    /// * `new_tokens` - The new tokens to store
    ///
    /// # Returns
    ///
    /// Returns the old tokens if they existed, or `None` if this is the first time
    /// tokens are being stored for this profile.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use core_auth::{TokenStore, ProfileId, OAuthTokens};
    /// # async fn example(token_store: TokenStore, profile_id: ProfileId, new_tokens: OAuthTokens) -> core_auth::Result<()> {
    /// let old_tokens = token_store.rotate_tokens(profile_id, &new_tokens).await?;
    /// if old_tokens.is_some() {
    ///     println!("Tokens rotated successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rotate_tokens(
        &self,
        profile_id: ProfileId,
        new_tokens: &OAuthTokens,
    ) -> Result<Option<OAuthTokens>> {
        // Retrieve old tokens first
        let old_tokens = self.retrieve_tokens(profile_id).await?;

        // Store new tokens
        self.store_tokens(profile_id, new_tokens).await?;

        info!(
            profile_id = %profile_id,
            had_previous_tokens = old_tokens.is_some(),
            "Tokens rotated successfully"
        );

        Ok(old_tokens)
    }

    /// Generate the storage key for a profile's tokens
    ///
    /// Keys are formatted as "oauth_tokens:<profile_id>" to namespace
    /// them within the secure store.
    fn storage_key(&self, profile_id: ProfileId) -> String {
        format!("oauth_tokens:{}", profile_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    /// Mock implementation of SecureStore for testing
    #[derive(Clone)]
    struct MockSecureStore {
        storage: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl MockSecureStore {
        fn new() -> Self {
            Self {
                storage: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl SecureStore for MockSecureStore {
        async fn set_secret(&self, key: &str, value: &[u8]) -> bridge_traits::error::Result<()> {
            let mut storage = self.storage.lock().await;
            storage.insert(key.to_string(), value.to_vec());
            Ok(())
        }

        async fn get_secret(&self, key: &str) -> bridge_traits::error::Result<Option<Vec<u8>>> {
            let storage = self.storage.lock().await;
            Ok(storage.get(key).cloned())
        }

        async fn delete_secret(&self, key: &str) -> bridge_traits::error::Result<()> {
            let mut storage = self.storage.lock().await;
            storage.remove(key);
            Ok(())
        }

        async fn has_secret(&self, key: &str) -> bridge_traits::error::Result<bool> {
            let storage = self.storage.lock().await;
            Ok(storage.contains_key(key))
        }

        async fn list_keys(&self) -> bridge_traits::error::Result<Vec<String>> {
            let storage = self.storage.lock().await;
            Ok(storage.keys().cloned().collect())
        }

        async fn clear_all(&self) -> bridge_traits::error::Result<()> {
            let mut storage = self.storage.lock().await;
            storage.clear();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_store_and_retrieve_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();
        let tokens = OAuthTokens::new(
            "access_token_123".to_string(),
            Some("refresh_token_456".to_string()),
            3600,
        );

        // Store tokens
        token_store
            .store_tokens(profile_id, &tokens)
            .await
            .expect("Failed to store tokens");

        // Retrieve tokens
        let retrieved = token_store
            .retrieve_tokens(profile_id)
            .await
            .expect("Failed to retrieve tokens")
            .expect("Tokens not found");

        assert_eq!(retrieved.access_token(), tokens.access_token());
        assert_eq!(retrieved.refresh_token(), tokens.refresh_token());
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();

        let result = token_store
            .retrieve_tokens(profile_id)
            .await
            .expect("Failed to check for tokens");

        assert!(result.is_none(), "Expected no tokens to be found");
    }

    #[tokio::test]
    async fn test_delete_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();
        let tokens = OAuthTokens::new(
            "access_token_123".to_string(),
            Some("refresh_token_456".to_string()),
            3600,
        );

        // Store tokens
        token_store
            .store_tokens(profile_id, &tokens)
            .await
            .expect("Failed to store tokens");

        // Verify tokens exist
        let has_tokens = token_store
            .has_tokens(profile_id)
            .await
            .expect("Failed to check tokens");
        assert!(has_tokens, "Tokens should exist");

        // Delete tokens
        token_store
            .delete_tokens(profile_id)
            .await
            .expect("Failed to delete tokens");

        // Verify tokens are gone
        let has_tokens = token_store
            .has_tokens(profile_id)
            .await
            .expect("Failed to check tokens");
        assert!(!has_tokens, "Tokens should be deleted");
    }

    #[tokio::test]
    async fn test_delete_nonexistent_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();

        // Delete should succeed even if no tokens exist
        token_store
            .delete_tokens(profile_id)
            .await
            .expect("Delete should succeed for nonexistent tokens");
    }

    #[tokio::test]
    async fn test_has_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();

        // Should return false initially
        let has_tokens = token_store
            .has_tokens(profile_id)
            .await
            .expect("Failed to check tokens");
        assert!(!has_tokens, "Should not have tokens initially");

        // Store tokens
        let tokens = OAuthTokens::new(
            "access_token_123".to_string(),
            Some("refresh_token_456".to_string()),
            3600,
        );
        token_store
            .store_tokens(profile_id, &tokens)
            .await
            .expect("Failed to store tokens");

        // Should return true now
        let has_tokens = token_store
            .has_tokens(profile_id)
            .await
            .expect("Failed to check tokens");
        assert!(has_tokens, "Should have tokens after storing");
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id1 = ProfileId::new();
        let profile_id2 = ProfileId::new();

        // Initially empty
        let profiles = token_store
            .list_profiles()
            .await
            .expect("Failed to list profiles");
        assert_eq!(profiles.len(), 0, "Should have no profiles initially");

        // Store tokens for first profile
        let tokens1 = OAuthTokens::new(
            "access_token_1".to_string(),
            Some("refresh_token_1".to_string()),
            3600,
        );
        token_store
            .store_tokens(profile_id1, &tokens1)
            .await
            .expect("Failed to store tokens");

        // Should have one profile
        let profiles = token_store
            .list_profiles()
            .await
            .expect("Failed to list profiles");
        assert_eq!(profiles.len(), 1, "Should have one profile");
        assert!(profiles.contains(&profile_id1));

        // Store tokens for second profile
        let tokens2 = OAuthTokens::new(
            "access_token_2".to_string(),
            Some("refresh_token_2".to_string()),
            3600,
        );
        token_store
            .store_tokens(profile_id2, &tokens2)
            .await
            .expect("Failed to store tokens");

        // Should have two profiles
        let profiles = token_store
            .list_profiles()
            .await
            .expect("Failed to list profiles");
        assert_eq!(profiles.len(), 2, "Should have two profiles");
        assert!(profiles.contains(&profile_id1));
        assert!(profiles.contains(&profile_id2));
    }

    #[tokio::test]
    async fn test_rotate_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();
        let old_tokens = OAuthTokens::new(
            "old_access_token".to_string(),
            Some("old_refresh_token".to_string()),
            3600,
        );
        let new_tokens = OAuthTokens::new(
            "new_access_token".to_string(),
            Some("new_refresh_token".to_string()),
            7200,
        );

        // First rotation (no previous tokens)
        let previous = token_store
            .rotate_tokens(profile_id, &old_tokens)
            .await
            .expect("Failed to rotate tokens");
        assert!(previous.is_none(), "Should have no previous tokens");

        // Second rotation (has previous tokens)
        let previous = token_store
            .rotate_tokens(profile_id, &new_tokens)
            .await
            .expect("Failed to rotate tokens")
            .expect("Should have previous tokens");

        assert_eq!(
            previous.access_token(),
            old_tokens.access_token(),
            "Previous tokens should match old tokens"
        );

        // Verify new tokens are stored
        let current = token_store
            .retrieve_tokens(profile_id)
            .await
            .expect("Failed to retrieve tokens")
            .expect("Should have tokens");

        assert_eq!(
            current.access_token(),
            new_tokens.access_token(),
            "Current tokens should match new tokens"
        );
    }

    #[tokio::test]
    async fn test_overwrite_tokens() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();
        let tokens1 = OAuthTokens::new(
            "access_token_1".to_string(),
            Some("refresh_token_1".to_string()),
            3600,
        );
        let tokens2 = OAuthTokens::new(
            "access_token_2".to_string(),
            Some("refresh_token_2".to_string()),
            7200,
        );

        // Store first tokens
        token_store
            .store_tokens(profile_id, &tokens1)
            .await
            .expect("Failed to store tokens");

        // Store second tokens (should overwrite)
        token_store
            .store_tokens(profile_id, &tokens2)
            .await
            .expect("Failed to store tokens");

        // Retrieve should return second tokens
        let retrieved = token_store
            .retrieve_tokens(profile_id)
            .await
            .expect("Failed to retrieve tokens")
            .expect("Tokens not found");

        assert_eq!(
            retrieved.access_token(),
            tokens2.access_token(),
            "Should have second access token"
        );
        assert_eq!(
            retrieved.refresh_token(),
            tokens2.refresh_token(),
            "Should have second refresh token"
        );
    }

    #[tokio::test]
    async fn test_tokens_without_refresh_token() {
        let secure_store = Arc::new(MockSecureStore::new());
        let token_store = TokenStore::new(secure_store);

        let profile_id = ProfileId::new();
        let tokens = OAuthTokens::new("access_token_only".to_string(), None, 3600);

        // Store tokens without refresh token
        token_store
            .store_tokens(profile_id, &tokens)
            .await
            .expect("Failed to store tokens");

        // Retrieve and verify
        let retrieved = token_store
            .retrieve_tokens(profile_id)
            .await
            .expect("Failed to retrieve tokens")
            .expect("Tokens not found");

        assert_eq!(retrieved.access_token(), tokens.access_token());
        assert!(
            retrieved.refresh_token().is_none(),
            "Should have no refresh token"
        );
    }
}
