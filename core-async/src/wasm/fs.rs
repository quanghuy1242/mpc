//! WebAssembly Filesystem Adapter
//!
//! This module provides a thin adapter that exposes a Tokio-like filesystem API for WASM
//! environments. It uses dynamic dispatch to call into a filesystem implementation
//! (typically `bridge-wasm::WasmFileSystem`) that stores files in IndexedDB.
//!
//! # Architecture
//!
//! The filesystem is injected during bootstrap via `init_filesystem()`. This avoids
//! circular dependencies between core-async and bridge-traits while still providing
//! a familiar async filesystem API.
//!
//! # Usage
//!
//! ```no_run
//! use core_async::fs;
//!
//! # async fn example() -> std::io::Result<()> {
//! // Write a file
//! fs::write("/data/config.json", b"{}").await?;
//!
//! // Read a file
//! let contents = fs::read_to_string("/data/config.json").await?;
//!
//! // List directory
//! let mut entries = fs::read_dir("/data").await?;
//! while let Some(entry) = entries.next_entry().await? {
//!     println!("{:?}", entry.path());
//! }
//! # Ok(())
//! # }
//! ```

use bytes::Bytes;
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};

// Re-export for convenience
pub use std::fs::Permissions;

/// File metadata returned by the filesystem
#[derive(Debug, Clone)]
pub struct BridgeFileMetadata {
    pub size: u64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub is_directory: bool,
}

/// Trait for filesystem operations (subset needed by core-async)
///
/// This is a minimal trait that avoids depending on bridge-traits directly.
/// The actual implementation is provided by bridge-wasm::WasmFileSystem.
pub trait WasmFileSystemOps {
    fn read_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Bytes, String>> + 'a>>;

    fn write_file<'a>(
        &'a self,
        path: &'a Path,
        data: Bytes,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

    fn delete_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

    fn delete_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

    fn create_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PathBuf>, String>> + 'a>>;

    fn metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<BridgeFileMetadata, String>> + 'a>>;

    fn exists<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + 'a>>;
}

/// Global filesystem instance holder
static mut FILESYSTEM: Option<Rc<dyn WasmFileSystemOps>> = None;

/// Initialize the WASM filesystem with a filesystem implementation
///
/// This must be called once during application startup, typically from
/// the bootstrap function.
///
/// # Safety
///
/// This function is unsafe because it modifies a global mutable static.
/// It should only be called once from the main thread during initialization.
///
/// # Panics
///
/// Panics if called more than once.
pub unsafe fn init_filesystem(fs: Rc<dyn WasmFileSystemOps>) {
    if FILESYSTEM.is_some() {
        panic!("WASM filesystem already initialized");
    }
    FILESYSTEM = Some(fs);
}

/// Get the global filesystem instance
fn get_filesystem() -> io::Result<Rc<dyn WasmFileSystemOps>> {
    unsafe {
        FILESYSTEM
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Filesystem not initialized. Call init_filesystem() during bootstrap."))
    }
}

/// Convert string error to io::Error
fn string_error_to_io(err: String) -> io::Error {
    // Try to determine error kind from message
    if err.contains("not found") || err.contains("NotFound") {
        io::Error::new(io::ErrorKind::NotFound, err)
    } else if err.contains("permission") || err.contains("PermissionDenied") {
        io::Error::new(io::ErrorKind::PermissionDenied, err)
    } else {
        io::Error::new(io::ErrorKind::Other, err)
    }
}

// ============================================================================
// Free Functions (Tokio-like API)
// ============================================================================

/// Read the entire contents of a file into a bytes vector.
pub async fn read<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let fs = get_filesystem()?;
    let bytes = fs
        .read_file(path.as_ref())
        .await
        .map_err(string_error_to_io)?;
    Ok(bytes.to_vec())
}

/// Read the entire contents of a file into a string.
pub async fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let bytes = read(path).await?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write a slice as the entire contents of a file.
pub async fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> io::Result<()> {
    let fs = get_filesystem()?;
    fs.write_file(path.as_ref(), Bytes::copy_from_slice(contents.as_ref()))
        .await
        .map_err(string_error_to_io)
}

/// Create a new, empty directory at the provided path, including all parent directories.
pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let fs = get_filesystem()?;
    fs.create_dir_all(path.as_ref())
        .await
        .map_err(string_error_to_io)
}

/// Removes a file from the filesystem.
pub async fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let fs = get_filesystem()?;
    fs.delete_file(path.as_ref())
        .await
        .map_err(string_error_to_io)
}

/// Removes a directory and all its contents recursively.
pub async fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let fs = get_filesystem()?;
    fs.delete_dir_all(path.as_ref())
        .await
        .map_err(string_error_to_io)
}

/// Returns an iterator over the entries within a directory.
pub async fn read_dir<P: AsRef<Path>>(path: P) -> io::Result<ReadDir> {
    let fs = get_filesystem()?;
    let entries = fs
        .list_directory(path.as_ref())
        .await
        .map_err(string_error_to_io)?;
    Ok(ReadDir {
        entries: entries.into_iter(),
    })
}

/// Given a path, query the file system to get information about a file, directory, etc.
pub async fn metadata<P: AsRef<Path>>(path: P) -> io::Result<FileMetadata> {
    let fs = get_filesystem()?;
    let meta = fs
        .metadata(path.as_ref())
        .await
        .map_err(string_error_to_io)?;
    Ok(FileMetadata {
        size: meta.size,
        is_dir: meta.is_directory,
        is_file: !meta.is_directory,
    })
}

// ============================================================================
// Types
// ============================================================================

/// Iterator over the entries in a directory
pub struct ReadDir {
    entries: std::vec::IntoIter<PathBuf>,
}

impl ReadDir {
    /// Returns the next entry in the directory stream.
    pub async fn next_entry(&mut self) -> io::Result<Option<DirEntry>> {
        Ok(self.entries.next().map(|path| DirEntry { path }))
    }
}

/// Entries returned by [`ReadDir`]
#[derive(Debug, Clone)]
pub struct DirEntry {
    path: PathBuf,
}

impl DirEntry {
    /// Returns the full path to the file that this entry represents.
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    /// Returns the file name of this directory entry.
    pub fn file_name(&self) -> std::ffi::OsString {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_os_string()
    }

    /// Returns the metadata for the file that this entry points at.
    pub async fn metadata(&self) -> io::Result<FileMetadata> {
        metadata(&self.path).await
    }
}

/// Metadata information about a file.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// Size of the file in bytes
    pub size: u64,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Whether this is a file
    pub is_file: bool,
}

impl FileMetadata {
    /// Returns `true` if this metadata is for a directory.
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Returns `true` if this metadata is for a regular file.
    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns the size of the file, in bytes.
    pub fn len(&self) -> u64 {
        self.size
    }

    /// Returns `true` if the file size is 0.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

// ============================================================================
// File Handle (Simplified)
// ============================================================================

/// A reference to an open file on the filesystem (WASM-specific implementation).
///
/// Note: On WASM, this is a simplified implementation that doesn't support
/// true streaming. Files are read/written entirely in memory.
pub struct File {
    path: PathBuf,
    fs: Rc<dyn WasmFileSystemOps>,
}

impl File {
    /// Attempts to open a file in read-only mode.
    pub async fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let fs = get_filesystem()?;
        let path = path.as_ref().to_path_buf();

        // Verify file exists
        fs.metadata(&path)
            .await
            .map_err(string_error_to_io)?;

        Ok(File { path, fs })
    }

    /// Attempts to create a file in write-only mode.
    pub async fn create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let fs = get_filesystem()?;
        let path = path.as_ref().to_path_buf();

        // Create empty file
        fs.write_file(&path, Bytes::new())
            .await
            .map_err(string_error_to_io)?;

        Ok(File { path, fs })
    }

    /// Read the entire contents of the file into a bytes buffer.
    pub async fn read_to_end(&self) -> io::Result<Vec<u8>> {
        let bytes = self
            .fs
            .read_file(&self.path)
            .await
            .map_err(string_error_to_io)?;
        Ok(bytes.to_vec())
    }

    /// Writes all bytes to the file, replacing any existing content.
    pub async fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        self.fs
            .write_file(&self.path, Bytes::copy_from_slice(buf))
            .await
            .map_err(string_error_to_io)
    }

    /// Queries metadata about the underlying file.
    pub async fn metadata(&self) -> io::Result<FileMetadata> {
        metadata(&self.path).await
    }
}

/// Options and flags which can be used to configure how a file is opened.
///
/// Note: On WASM, this is a simplified implementation. Many options are ignored
/// since IndexedDB doesn't support all traditional file operations.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOptions {
    /// Creates a blank new set of options ready for configuration.
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            create: false,
            truncate: false,
        }
    }

    /// Sets the option for read access.
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    /// Sets the option for write access.
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    /// Sets the option to create a new file, or open it if it already exists.
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    /// Sets the option for truncating a previous file.
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    /// Opens a file at the specified path with the options specified by `self`.
    pub async fn open<P: AsRef<Path>>(&self, path: P) -> io::Result<File> {
        let fs = get_filesystem()?;
        let path = path.as_ref().to_path_buf();

        if self.create || self.write {
            // Create or truncate file
            if self.truncate || !fs.exists(&path).await.map_err(string_error_to_io)? {
                fs.write_file(&path, Bytes::new())
                    .await
                    .map_err(string_error_to_io)?;
            }
        } else if self.read {
            // Verify file exists for read
            fs.metadata(&path)
                .await
                .map_err(string_error_to_io)?;
        }

        Ok(File { path, fs })
    }
}

// ============================================================================
// Stub Implementations for Unsupported APIs
// ============================================================================

/// Stub for create_dir (single directory creation)
///
/// On WASM, always creates parent directories as well.
pub async fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    create_dir_all(path).await
}

/// Stub for remove_dir (single directory removal)
///
/// On WASM, always removes directory recursively.
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    remove_dir_all(path).await
}

/// Stub for copy - not supported on WASM
pub async fn copy<P: AsRef<Path>, Q: AsRef<Path>>(_from: P, _to: Q) -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "copy is not supported on WASM. Use read + write instead.",
    ))
}

/// Stub for rename - not supported on WASM  
pub async fn rename<P: AsRef<Path>, Q: AsRef<Path>>(_from: P, _to: Q) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "rename is not supported on WASM. Use read + write + delete instead.",
    ))
}

/// Stub for hard_link - not supported on WASM
pub async fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(_src: P, _dst: Q) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "hard_link is not supported on WASM (IndexedDB limitation).",
    ))
}

/// Stub for read_link - not supported on WASM
pub async fn read_link<P: AsRef<Path>>(_path: P) -> io::Result<PathBuf> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "read_link is not supported on WASM (IndexedDB limitation).",
    ))
}

/// Stub for symlink_metadata - same as metadata on WASM
pub async fn symlink_metadata<P: AsRef<Path>>(path: P) -> io::Result<FileMetadata> {
    metadata(path).await
}

/// Stub for set_permissions - not supported on WASM
pub async fn set_permissions<P: AsRef<Path>>(_path: P, _perm: Permissions) -> io::Result<()> {
    // No-op on WASM
    Ok(())
}

/// Stub for DirBuilder - same as create_dir_all on WASM
pub struct DirBuilder;

impl DirBuilder {
    /// Creates a new set of options with default mode/security settings.
    pub fn new() -> Self {
        DirBuilder
    }

    /// Indicates that directories should be created recursively.
    pub fn recursive(&mut self, _recursive: bool) -> &mut Self {
        self
    }

    /// Creates the specified directory with the configured options.
    pub async fn create<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        create_dir_all(path).await
    }
}

impl Default for DirBuilder {
    fn default() -> Self {
        Self::new()
    }
}
