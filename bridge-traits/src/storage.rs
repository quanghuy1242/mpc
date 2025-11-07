//! Storage and File System Abstractions
//!
//! Provides platform-agnostic traits for file I/O, secure credential storage,
//! and key-value settings storage.

use bytes::Bytes;
use std::path::{Path, PathBuf};

use crate::{
    error::Result,
    platform::{DynAsyncRead, DynAsyncWrite, PlatformSend, PlatformSendSync},
};

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
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait FileSystemAccess: PlatformSendSync {
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
    async fn open_read_stream(&self, path: &Path) -> Result<Box<DynAsyncRead>>;

    /// Open a file for streaming writes
    async fn open_write_stream(&self, path: &Path) -> Result<Box<DynAsyncWrite>>;

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
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SecureStore: PlatformSendSync {
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
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SettingsStore: PlatformSendSync {
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
    async fn begin_transaction(&self) -> Result<Box<dyn SettingsTransaction>>;
}

/// Transaction for atomic settings updates
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait SettingsTransaction: PlatformSend {
    /// Set a value within the transaction
    async fn set_string(&mut self, key: &str, value: &str) -> Result<()>;

    /// Commit the transaction
    async fn commit(self: Box<Self>) -> Result<()>;

    /// Rollback the transaction
    async fn rollback(self: Box<Self>) -> Result<()>;
}

/// Remote file information from cloud storage providers
///
/// Represents a file or folder retrieved from a cloud storage provider
/// like Google Drive or OneDrive.
#[derive(Debug, Clone)]
pub struct RemoteFile {
    /// Unique identifier for the file in the provider's system
    pub id: String,

    /// File name
    pub name: String,

    /// MIME type (e.g., "audio/mpeg", "audio/flac")
    pub mime_type: Option<String>,

    /// File size in bytes (None for folders)
    pub size: Option<u64>,

    /// Creation timestamp (Unix timestamp)
    pub created_at: Option<i64>,

    /// Modification timestamp (Unix timestamp)
    pub modified_at: Option<i64>,

    /// Whether this is a folder/directory
    pub is_folder: bool,

    /// Parent folder ID(s)
    pub parent_ids: Vec<String>,

    /// MD5 hash of file contents (if available)
    pub md5_checksum: Option<String>,

    /// Provider-specific metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Cloud storage provider trait
///
/// Abstracts cloud storage providers (Google Drive, OneDrive, WebDAV, etc.)
/// for listing, downloading, and syncing music files.
///
/// # Sync Strategy
///
/// Providers should support both full sync (initial indexing) and incremental
/// sync (change detection) for efficient synchronization:
///
/// - **Full Sync**: Use `list_media()` to enumerate all audio files
/// - **Incremental Sync**: Use `get_changes()` with a cursor to fetch only updates
///
/// # Example
///
/// ```ignore
/// use bridge_traits::storage::StorageProvider;
///
/// async fn sync_music(provider: &dyn StorageProvider) -> Result<Vec<RemoteFile>> {
///     let (files, next_cursor) = provider.list_media(None).await?;
///     
///     // Filter audio files
///     let audio_files: Vec<RemoteFile> = files
///         .into_iter()
///         .filter(|f| {
///             f.mime_type
///                 .as_ref()
///                 .map(|m| m.starts_with("audio/"))
///                 .unwrap_or(false)
///         })
///         .collect();
///     
///     Ok(audio_files)
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait StorageProvider: PlatformSendSync {
    /// List media files from the cloud storage
    ///
    /// Returns a paginated list of files. Audio files should be filtered by MIME type
    /// on the client side.
    ///
    /// # Arguments
    ///
    /// * `cursor` - Optional pagination token from a previous call. Pass `None` for the first page.
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - `Vec<RemoteFile>` - List of files in this page
    /// - `Option<String>` - Cursor for the next page, or `None` if this is the last page
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Authentication token is invalid or expired
    /// - Network request fails
    /// - API rate limit is exceeded
    /// - Provider API returns an error
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (files, next_cursor) = provider.list_media(None).await?;
    /// println!("Found {} files", files.len());
    ///
    /// if let Some(cursor) = next_cursor {
    ///     let (more_files, _) = provider.list_media(Some(cursor)).await?;
    ///     println!("Found {} more files", more_files.len());
    /// }
    /// ```
    async fn list_media(&self, cursor: Option<String>)
        -> Result<(Vec<RemoteFile>, Option<String>)>;

    /// Get detailed metadata for a specific file
    ///
    /// # Arguments
    ///
    /// * `file_id` - The unique file identifier from the provider
    ///
    /// # Returns
    ///
    /// Returns the `RemoteFile` with complete metadata
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File doesn't exist
    /// - Authentication is invalid
    /// - Network request fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// let file = provider.get_metadata("abc123").await?;
    /// println!("File: {} ({})", file.name, file.mime_type.unwrap_or_default());
    /// ```
    async fn get_metadata(&self, file_id: &str) -> Result<RemoteFile>;

    /// Download file contents as a byte stream
    ///
    /// Returns a streaming reader for efficient downloading of large files.
    /// Supports optional range requests for resumable downloads.
    ///
    /// # Arguments
    ///
    /// * `file_id` - The unique file identifier
    /// * `range` - Optional byte range in format "bytes=start-end" (e.g., "bytes=0-1023")
    ///
    /// # Returns
    ///
    /// Returns a `Bytes` buffer containing the file contents (or requested range)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File doesn't exist
    /// - File is a folder (cannot download)
    /// - Authentication is invalid
    /// - Network request fails
    /// - Range request is invalid
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Full download
    /// let data = provider.download("abc123", None).await?;
    /// println!("Downloaded {} bytes", data.len());
    ///
    /// // Range request for first 1KB
    /// let partial = provider.download("abc123", Some("bytes=0-1023")).await?;
    /// ```
    async fn download(&self, file_id: &str, range: Option<&str>) -> Result<Bytes>;

    /// Get incremental changes since a previous sync
    ///
    /// Enables efficient incremental synchronization by fetching only files that
    /// have been created, modified, or deleted since the last sync.
    ///
    /// # Arguments
    ///
    /// * `cursor` - Change token from a previous sync. Pass `None` or the token from
    ///   `list_media()` to start tracking changes.
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - `Vec<RemoteFile>` - List of changed files (includes deleted files with special marker)
    /// - `Option<String>` - New change token to use for the next incremental sync
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Change token is invalid or expired
    /// - Authentication is invalid
    /// - Network request fails
    ///
    /// # Notes
    ///
    /// - Change tokens may expire after a certain period (e.g., 7 days for Google Drive)
    /// - If token is expired, perform a full sync with `list_media()` and get a new token
    /// - Deleted files may be included in results (check provider-specific metadata)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Initial sync
    /// let (files, cursor) = provider.list_media(None).await?;
    /// store_cursor(&cursor);
    ///
    /// // Later: incremental sync
    /// let stored_cursor = load_cursor();
    /// match provider.get_changes(stored_cursor).await {
    ///     Ok((changes, new_cursor)) => {
    ///         println!("Found {} changes", changes.len());
    ///         store_cursor(&new_cursor);
    ///     }
    ///     Err(e) if e.to_string().contains("token expired") => {
    ///         // Fall back to full sync
    ///         let (files, cursor) = provider.list_media(None).await?;
    ///         store_cursor(&cursor);
    ///     }
    ///     Err(e) => return Err(e),
    /// }
    /// ```
    async fn get_changes(
        &self,
        cursor: Option<String>,
    ) -> Result<(Vec<RemoteFile>, Option<String>)>;
}

// Blanket implementation for Arc<dyn FileSystemAccess> to allow passing Arc directly
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl FileSystemAccess for std::sync::Arc<dyn FileSystemAccess> {
    async fn get_cache_directory(&self) -> Result<PathBuf> {
        (**self).get_cache_directory().await
    }

    async fn get_data_directory(&self) -> Result<PathBuf> {
        (**self).get_data_directory().await
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        (**self).exists(path).await
    }

    async fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        (**self).metadata(path).await
    }

    async fn create_dir_all(&self, path: &Path) -> Result<()> {
        (**self).create_dir_all(path).await
    }

    async fn read_file(&self, path: &Path) -> Result<Bytes> {
        (**self).read_file(path).await
    }

    async fn write_file(&self, path: &Path, data: Bytes) -> Result<()> {
        (**self).write_file(path, data).await
    }

    async fn append_file(&self, path: &Path, data: Bytes) -> Result<()> {
        (**self).append_file(path, data).await
    }

    async fn delete_file(&self, path: &Path) -> Result<()> {
        (**self).delete_file(path).await
    }

    async fn delete_dir_all(&self, path: &Path) -> Result<()> {
        (**self).delete_dir_all(path).await
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>> {
        (**self).list_directory(path).await
    }

    async fn open_read_stream(&self, path: &Path) -> Result<Box<DynAsyncRead>> {
        (**self).open_read_stream(path).await
    }

    async fn open_write_stream(&self, path: &Path) -> Result<Box<DynAsyncWrite>> {
        (**self).open_write_stream(path).await
    }
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

    #[test]
    fn test_remote_file_creation() {
        let mut metadata_map = std::collections::HashMap::new();
        metadata_map.insert("key".to_string(), "value".to_string());

        let file = RemoteFile {
            id: "file123".to_string(),
            name: "song.mp3".to_string(),
            mime_type: Some("audio/mpeg".to_string()),
            size: Some(5242880),
            created_at: Some(1234567890),
            modified_at: Some(1234567900),
            is_folder: false,
            parent_ids: vec!["folder1".to_string()],
            md5_checksum: Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
            metadata: metadata_map,
        };

        assert_eq!(file.id, "file123");
        assert_eq!(file.name, "song.mp3");
        assert!(!file.is_folder);
        assert_eq!(file.size, Some(5242880));
    }
}
