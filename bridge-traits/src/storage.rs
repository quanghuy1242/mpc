//! Storage and File System Abstractions
//!
//! Provides platform-agnostic traits for file I/O, secure credential storage,
//! and key-value settings storage.

use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// File metadata information
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub is_directory: bool,
}

/// File system access trait
///
/// Abstracts file I/O operations to support different platforms:
/// - Desktop: Direct filesystem access
/// - iOS/Android: Sandboxed app directories, SAF/document picker
/// - Web: OPFS, IndexedDB
///
/// # Example
///
/// ```ignore
/// use bridge_traits::storage::FileSystemAccess;
///
/// async fn cache_data(fs: &dyn FileSystemAccess, data: &[u8]) -> Result<()> {
///     let cache_dir = fs.get_cache_directory().await?;
///     let file_path = cache_dir.join("data.bin");
///     fs.write_file(&file_path, data.into()).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait FileSystemAccess: Send + Sync {
    /// Get the application's cache directory
    ///
    /// This directory is suitable for temporary files that can be deleted
    /// by the system when storage is low.
    async fn get_cache_directory(&self) -> Result<PathBuf>;

    /// Get the application's data directory
    ///
    /// This directory is suitable for persistent application data.
    async fn get_data_directory(&self) -> Result<PathBuf>;

    /// Check if a file or directory exists
    async fn exists(&self, path: &Path) -> Result<bool>;

    /// Get metadata for a file or directory
    async fn metadata(&self, path: &Path) -> Result<FileMetadata>;

    /// Create a directory and all parent directories if they don't exist
    async fn create_dir_all(&self, path: &Path) -> Result<()>;

    /// Read entire file contents into memory
    ///
    /// For large files, consider using `open_read_stream` instead.
    async fn read_file(&self, path: &Path) -> Result<Bytes>;

    /// Write data to a file, creating it if it doesn't exist
    async fn write_file(&self, path: &Path, data: Bytes) -> Result<()>;

    /// Append data to an existing file or create it
    async fn append_file(&self, path: &Path, data: Bytes) -> Result<()>;

    /// Delete a file
    async fn delete_file(&self, path: &Path) -> Result<()>;

    /// Delete a directory and all its contents
    async fn delete_dir_all(&self, path: &Path) -> Result<()>;

    /// List all entries in a directory
    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>>;

    /// Open a file for streaming reads
    ///
    /// This is more efficient than `read_file` for large files.
    async fn open_read_stream(&self, path: &Path) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>>;

    /// Open a file for streaming writes
    async fn open_write_stream(&self, path: &Path) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>;

    /// Calculate total size of a directory recursively
    async fn directory_size(&self, path: &Path) -> Result<u64> {
        let mut total = 0u64;
        let entries = self.list_directory(path).await?;
        
        for entry in entries {
            let metadata = self.metadata(&entry).await?;
            if metadata.is_directory {
                total += self.directory_size(&entry).await?;
            } else {
                total += metadata.size;
            }
        }
        
        Ok(total)
    }
}

/// Secure credential storage trait
///
/// Abstracts secure storage mechanisms:
/// - macOS/iOS: Keychain
/// - Android: Keystore (hardware-backed when available)
/// - Windows: DPAPI
/// - Linux: Secret Service / libsecret
/// - Web: WebCrypto + encrypted localStorage/IndexedDB
///
/// # Security Requirements
///
/// Implementations MUST:
/// - Encrypt data at rest
/// - Use platform-provided secure storage when available
/// - Support hardware-backed encryption where possible
/// - Lock access when device is locked (platform-dependent)
/// - Never log or expose sensitive data
///
/// # Example
///
/// ```ignore
/// use bridge_traits::storage::SecureStore;
///
/// async fn store_token(store: &dyn SecureStore, token: &str) -> Result<()> {
///     store.set_secret("oauth_token", token.as_bytes()).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait SecureStore: Send + Sync {
    /// Store a secret value
    ///
    /// # Arguments
    ///
    /// * `key` - Unique identifier for the secret
    /// * `value` - Secret data to store
    ///
    /// # Security
    ///
    /// - Value is encrypted before storage
    /// - Previous value is securely erased if it exists
    async fn set_secret(&self, key: &str, value: &[u8]) -> Result<()>;

    /// Retrieve a secret value
    ///
    /// # Returns
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    ///
    /// # Security
    ///
    /// - Value is decrypted only when retrieved
    /// - Returned data should be handled securely and not logged
    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Delete a secret
    ///
    /// # Security
    ///
    /// - Data is securely erased from storage
    async fn delete_secret(&self, key: &str) -> Result<()>;

    /// Check if a secret exists without retrieving it
    async fn has_secret(&self, key: &str) -> Result<bool> {
        Ok(self.get_secret(key).await?.is_some())
    }

    /// List all secret keys (without values)
    ///
    /// Useful for debugging or migration scenarios.
    async fn list_keys(&self) -> Result<Vec<String>>;

    /// Clear all secrets
    ///
    /// Use with caution! This will delete all stored secrets.
    async fn clear_all(&self) -> Result<()>;
}

/// Key-value settings storage trait
///
/// Abstracts platform-specific preferences/settings storage:
/// - iOS: UserDefaults
/// - Android: SharedPreferences / DataStore
/// - Desktop: Config files or OS-specific preferences
/// - Web: localStorage / IndexedDB
///
/// # Example
///
/// ```ignore
/// use bridge_traits::storage::SettingsStore;
///
/// async fn save_preference(store: &dyn SettingsStore) -> Result<()> {
///     store.set_string("theme", "dark").await?;
///     store.set_bool("sync_on_wifi_only", true).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait SettingsStore: Send + Sync {
    /// Store a string value
    async fn set_string(&self, key: &str, value: &str) -> Result<()>;

    /// Retrieve a string value
    async fn get_string(&self, key: &str) -> Result<Option<String>>;

    /// Store a boolean value
    async fn set_bool(&self, key: &str, value: bool) -> Result<()>;

    /// Retrieve a boolean value
    async fn get_bool(&self, key: &str) -> Result<Option<bool>>;

    /// Store an integer value
    async fn set_i64(&self, key: &str, value: i64) -> Result<()>;

    /// Retrieve an integer value
    async fn get_i64(&self, key: &str) -> Result<Option<i64>>;

    /// Store a floating-point value
    async fn set_f64(&self, key: &str, value: f64) -> Result<()>;

    /// Retrieve a floating-point value
    async fn get_f64(&self, key: &str) -> Result<Option<f64>>;

    /// Delete a setting
    async fn delete(&self, key: &str) -> Result<()>;

    /// Check if a setting exists
    async fn has_key(&self, key: &str) -> Result<bool>;

    /// List all setting keys
    async fn list_keys(&self) -> Result<Vec<String>>;

    /// Clear all settings
    async fn clear_all(&self) -> Result<()>;

    /// Begin a transaction for atomic updates
    ///
    /// Changes are committed when the transaction is dropped successfully,
    /// or rolled back if an error occurs.
    async fn begin_transaction(&self) -> Result<Box<dyn SettingsTransaction + Send>>;
}

/// Transaction for atomic settings updates
#[async_trait]
pub trait SettingsTransaction: Send {
    /// Set a value within the transaction
    async fn set_string(&mut self, key: &str, value: &str) -> Result<()>;

    /// Commit the transaction
    async fn commit(self: Box<Self>) -> Result<()>;

    /// Rollback the transaction
    async fn rollback(self: Box<Self>) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_metadata() {
        let metadata = FileMetadata {
            size: 1024,
            created_at: Some(1234567890),
            modified_at: Some(1234567900),
            is_directory: false,
        };

        assert_eq!(metadata.size, 1024);
        assert!(!metadata.is_directory);
    }
}
