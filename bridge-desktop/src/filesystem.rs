//! File System Access Implementation using Tokio

use async_trait::async_trait;
use bridge_traits::{
    error::{BridgeError, Result},
    storage::{FileMetadata, FileSystemAccess},
};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

/// Tokio-based file system implementation
///
/// Provides async file I/O operations using:
/// - `tokio::fs` for async operations
/// - Standard library paths
/// - Platform-specific app directories
pub struct TokioFileSystem {
    cache_dir: PathBuf,
    data_dir: PathBuf,
}

impl TokioFileSystem {
    /// Create a new file system accessor with default directories
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("music-platform-core");

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local")
                    .join("share")
            })
            .join("music-platform-core");

        Self { cache_dir, data_dir }
    }

    /// Create a new file system accessor with custom directories
    pub fn with_directories(cache_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self { cache_dir, data_dir }
    }

    /// Convert std::io::Error to BridgeError
    fn map_io_error(e: std::io::Error) -> BridgeError {
        BridgeError::Io(e)
    }
}

impl Default for TokioFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystemAccess for TokioFileSystem {
    async fn get_cache_directory(&self) -> Result<PathBuf> {
        // Ensure cache directory exists
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir)
                .await
                .map_err(Self::map_io_error)?;
            debug!(path = ?self.cache_dir, "Created cache directory");
        }
        Ok(self.cache_dir.clone())
    }

    async fn get_data_directory(&self) -> Result<PathBuf> {
        // Ensure data directory exists
        if !self.data_dir.exists() {
            fs::create_dir_all(&self.data_dir)
                .await
                .map_err(Self::map_io_error)?;
            debug!(path = ?self.data_dir, "Created data directory");
        }
        Ok(self.data_dir.clone())
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        Ok(fs::try_exists(path)
            .await
            .map_err(Self::map_io_error)?)
    }

    async fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = fs::metadata(path).await.map_err(Self::map_io_error)?;

        Ok(FileMetadata {
            size: metadata.len(),
            created_at: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            is_directory: metadata.is_dir(),
        })
    }

    async fn create_dir_all(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path)
            .await
            .map_err(Self::map_io_error)?;
        debug!(path = ?path, "Created directory");
        Ok(())
    }

    async fn read_file(&self, path: &Path) -> Result<Bytes> {
        let data = fs::read(path).await.map_err(Self::map_io_error)?;
        debug!(path = ?path, size = data.len(), "Read file");
        Ok(Bytes::from(data))
    }

    async fn write_file(&self, path: &Path, data: Bytes) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent).await?;
        }

        fs::write(path, data.as_ref())
            .await
            .map_err(Self::map_io_error)?;
        debug!(path = ?path, size = data.len(), "Wrote file");
        Ok(())
    }

    async fn append_file(&self, path: &Path, data: Bytes) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent).await?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(Self::map_io_error)?;

        file.write_all(data.as_ref())
            .await
            .map_err(Self::map_io_error)?;
        file.flush().await.map_err(Self::map_io_error)?;

        debug!(path = ?path, size = data.len(), "Appended to file");
        Ok(())
    }

    async fn delete_file(&self, path: &Path) -> Result<()> {
        fs::remove_file(path).await.map_err(Self::map_io_error)?;
        debug!(path = ?path, "Deleted file");
        Ok(())
    }

    async fn delete_dir_all(&self, path: &Path) -> Result<()> {
        fs::remove_dir_all(path)
            .await
            .map_err(Self::map_io_error)?;
        debug!(path = ?path, "Deleted directory");
        Ok(())
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(path).await.map_err(Self::map_io_error)?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(Self::map_io_error)?
        {
            entries.push(entry.path());
        }

        debug!(path = ?path, count = entries.len(), "Listed directory");
        Ok(entries)
    }

    async fn open_read_stream(
        &self,
        path: &Path,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let file = fs::File::open(path).await.map_err(Self::map_io_error)?;
        debug!(path = ?path, "Opened file for reading");
        Ok(Box::new(file))
    }

    async fn open_write_stream(
        &self,
        path: &Path,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent).await?;
        }

        let file = fs::File::create(path).await.map_err(Self::map_io_error)?;
        debug!(path = ?path, "Opened file for writing");
        Ok(Box::new(file))
    }

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

        debug!(path = ?path, size = total, "Calculated directory size");
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_filesystem_creation() {
        let _fs = TokioFileSystem::new();
        assert!(true); // Just verify it constructs
    }

    #[tokio::test]
    async fn test_custom_directories() {
        let cache = env::temp_dir().join("test-cache");
        let data = env::temp_dir().join("test-data");
        let fs = TokioFileSystem::with_directories(cache.clone(), data.clone());

        let cache_dir = fs.get_cache_directory().await.unwrap();
        assert_eq!(cache_dir, cache);
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let fs = TokioFileSystem::new();
        let test_file = env::temp_dir().join("test-file.txt");

        // Clean up if exists
        let _ = fs.delete_file(&test_file).await;

        // Write
        let data = Bytes::from("Hello, World!");
        fs.write_file(&test_file, data.clone()).await.unwrap();

        // Read
        let read_data = fs.read_file(&test_file).await.unwrap();
        assert_eq!(data, read_data);

        // Clean up
        fs.delete_file(&test_file).await.unwrap();
    }
}
