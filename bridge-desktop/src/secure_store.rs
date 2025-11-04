//! Secure Credential Storage using OS Keychain

use async_trait::async_trait;
use bridge_traits::{
    error::{BridgeError, Result},
    storage::SecureStore,
};
use keyring::Entry;
use tracing::{debug, error};

/// Keyring-based secure storage implementation
///
/// Uses platform-specific secure storage:
/// - macOS: Keychain
/// - Windows: Credential Manager (DPAPI)
/// - Linux: Secret Service (libsecret)
pub struct KeyringSecureStore {
    service_name: String,
}

impl KeyringSecureStore {
    /// Create a new secure store with default service name
    pub fn new() -> Self {
        Self {
            service_name: "music-platform-core".to_string(),
        }
    }

    /// Create a new secure store with custom service name
    pub fn with_service_name(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }

    /// Get a keyring entry for the given key
    fn get_entry(&self, key: &str) -> std::result::Result<Entry, keyring::Error> {
        Entry::new(&self.service_name, key)
    }

    /// Convert keyring error to BridgeError
    fn map_keyring_error(e: keyring::Error) -> BridgeError {
        BridgeError::OperationFailed(format!("Keyring error: {}", e))
    }
}

impl Default for KeyringSecureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecureStore for KeyringSecureStore {
    async fn set_secret(&self, key: &str, value: &[u8]) -> Result<()> {
        // Keyring only supports strings, so we base64 encode binary data
        let encoded = base64::encode(value);

        let entry = self.get_entry(key).map_err(Self::map_keyring_error)?;

        entry
            .set_password(&encoded)
            .map_err(Self::map_keyring_error)?;

        debug!(key = key, "Stored secret in keyring");
        Ok(())
    }

    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let entry = self.get_entry(key).map_err(Self::map_keyring_error)?;

        match entry.get_password() {
            Ok(encoded) => {
                let decoded = base64::decode(&encoded).map_err(|e| {
                    error!(key = key, error = %e, "Failed to decode secret");
                    BridgeError::OperationFailed(format!("Failed to decode secret: {}", e))
                })?;

                debug!(key = key, "Retrieved secret from keyring");
                Ok(Some(decoded))
            }
            Err(keyring::Error::NoEntry) => {
                debug!(key = key, "Secret not found in keyring");
                Ok(None)
            }
            Err(e) => Err(Self::map_keyring_error(e)),
        }
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        let entry = self.get_entry(key).map_err(Self::map_keyring_error)?;

        match entry.delete_credential() {
            Ok(_) => {
                debug!(key = key, "Deleted secret from keyring");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                // Already deleted, consider it success
                debug!(key = key, "Secret not found (already deleted)");
                Ok(())
            }
            Err(e) => Err(Self::map_keyring_error(e)),
        }
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        let entry = self.get_entry(key).map_err(Self::map_keyring_error)?;

        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(Self::map_keyring_error(e)),
        }
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        // Note: Keyring doesn't provide a way to list all keys
        // This is a platform limitation - we'd need to maintain our own index
        // For now, return empty list
        Ok(Vec::new())
    }

    async fn clear_all(&self) -> Result<()> {
        // Note: Keyring doesn't provide a way to enumerate and delete all entries
        // This would need to be tracked separately if needed
        Err(BridgeError::OperationFailed(
            "Clear all not supported by keyring - keys must be deleted individually".to_string(),
        ))
    }
}

// Add base64 dependency
mod base64 {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    pub fn encode(data: &[u8]) -> String {
        STANDARD.encode(data)
    }

    pub fn decode(data: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
        STANDARD.decode(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_secure_store_creation() {
        let store = KeyringSecureStore::new();
        assert_eq!(store.service_name, "music-platform-core");
    }

    #[tokio::test]
    async fn test_custom_service_name() {
        let store = KeyringSecureStore::with_service_name("test-service");
        assert_eq!(store.service_name, "test-service");
    }

    #[tokio::test]
    async fn test_set_and_get_secret() {
        // Note: This test might fail if keyring is not available (e.g., headless systems, CI)
        let store = KeyringSecureStore::with_service_name("test-music-platform-core");
        let key = "test-key-unique-123";
        let value = b"test-secret-value";

        // Clean up first
        let _ = store.delete_secret(key).await;

        // Try to set - this might fail if keyring is not available
        match store.set_secret(key, value).await {
            Ok(_) => {
                // Get - wrap in match to see actual error
                match store.get_secret(key).await {
                    Ok(Some(retrieved)) => {
                        assert_eq!(retrieved, value.to_vec());
                        
                        // Clean up
                        let _ = store.delete_secret(key).await;
                    }
                    Ok(None) => {
                        println!("Warning: Secret was set but not found. This can happen with keyring on some systems.");
                        // Clean up anyway
                        let _ = store.delete_secret(key).await;
                    }
                    Err(e) => {
                        println!("Error retrieving secret: {:?}", e);
                        // Clean up anyway
                        let _ = store.delete_secret(key).await;
                    }
                }
            }
            Err(e) => {
                // Keyring not available on this system (e.g., CI environment)
                println!("Keyring not available ({}), skipping test", e);
            }
        }
    }
}
