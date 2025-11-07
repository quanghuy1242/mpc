//! WebAssembly File System Implementation using IndexedDB
//!
//! This module provides a file system abstraction for WebAssembly environments
//! using IndexedDB as the storage backend. It implements the `FileSystemAccess`
//! trait from `bridge-traits`.
//!
//! # Architecture
//!
//! The file system is implemented using IndexedDB with the following structure:
//!
//! - **Database Name**: `{app_name}-filesystem`
//! - **Object Stores**:
//!   - `files`: Stores file metadata and content
//!   - `directories`: Stores directory metadata and hierarchy
//!
//! # File Storage
//!
//! Files are stored as JSON objects containing:
//! - `path`: Full path of the file
//! - `content`: Base64-encoded file content (for small files) or chunked storage
//! - `size`: File size in bytes
//! - `created_at`: Creation timestamp
//! - `modified_at`: Modification timestamp
//! - `is_directory`: Boolean flag
//!
//! # Path Handling
//!
//! Paths are normalized to use forward slashes and are stored as strings.
//! The root directory is represented as "/".
//!
//! # Limitations
//!
//! - Maximum file size is limited by IndexedDB storage quota (typically 50MB+)
//! - Large files may need chunking (implemented for files > 1MB)
//! - Path operations are case-sensitive
//! - No symbolic links or hard links

use async_trait::async_trait;
use bridge_traits::{
    error::Result as BridgeResult,
    storage::{FileMetadata, FileSystemAccess},
    DynAsyncRead, DynAsyncWrite,
};
use bytes::Bytes;
use futures::channel::oneshot;
use js_sys::{Array, Object, Reflect};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    IdbDatabase, IdbObjectStore, IdbObjectStoreParameters, IdbOpenDbRequest, IdbRequest,
    IdbTransaction, IdbTransactionMode, IdbVersionChangeEvent,
};

use crate::error::{WasmError, WasmResult};

/// Chunk size for large file storage (1MB)
const CHUNK_SIZE: usize = 1024 * 1024;

/// Maximum file size for non-chunked storage (1MB)
const MAX_INLINE_SIZE: usize = 1024 * 1024;

fn serde_io_error(msg: impl Into<String>) -> serde_json::Error {
    serde_json::Error::io(io::Error::new(io::ErrorKind::Other, msg.into()))
}

/// File entry stored in IndexedDB
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileEntry {
    /// Full path of the file
    path: String,
    /// File content (base64-encoded for small files)
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    /// Whether this file is chunked
    is_chunked: bool,
    /// Number of chunks (if chunked)
    chunk_count: usize,
    /// File size in bytes
    size: u64,
    /// Creation timestamp (Unix timestamp in milliseconds)
    created_at: i64,
    /// Modification timestamp (Unix timestamp in milliseconds)
    modified_at: i64,
    /// Whether this is a directory
    is_directory: bool,
}

/// File chunk stored separately for large files
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileChunk {
    /// File path this chunk belongs to
    file_path: String,
    /// Chunk index
    chunk_index: usize,
    /// Chunk data (base64-encoded)
    data: String,
}

/// WebAssembly file system implementation using IndexedDB
pub struct WasmFileSystem {
    /// IndexedDB database instance
    db: Arc<IdbDatabase>,
    /// Application name for namespacing (kept for debugging/future expansion)
    _app_name: String,
    /// Cache directory path
    cache_dir: PathBuf,
    /// Data directory path
    data_dir: PathBuf,
}

impl WasmFileSystem {
    /// Database version
    const DB_VERSION: f64 = 1.0;

    /// Create a new WebAssembly file system
    ///
    /// # Arguments
    ///
    /// * `app_name` - Application name used for database namespacing
    ///
    /// # Errors
    ///
    /// Returns an error if IndexedDB is not available or database creation fails
    pub async fn new(app_name: &str) -> WasmResult<Self> {
        let db = Self::open_database(app_name).await?;

        let cache_dir = PathBuf::from("/cache");
        let data_dir = PathBuf::from("/data");

        let fs = Self {
            db: Arc::new(db),
            _app_name: app_name.to_string(),
            cache_dir: cache_dir.clone(),
            data_dir: data_dir.clone(),
        };

        // Ensure root directories exist
        fs.ensure_directory_exists(&cache_dir).await?;
        fs.ensure_directory_exists(&data_dir).await?;

        Ok(fs)
    }

    /// Open or create the IndexedDB database
    async fn open_database(app_name: &str) -> WasmResult<IdbDatabase> {
        let window = web_sys::window().ok_or(WasmError::JavaScript(
            "No window object available".to_string(),
        ))?;

        let idb_factory = window
            .indexed_db()
            .map_err(|e| WasmError::from(e))?
            .ok_or(WasmError::IndexedDb("IndexedDB not available".to_string()))?;

        let db_name = format!("{}-filesystem", app_name);
        let open_request: IdbOpenDbRequest = idb_factory
            .open_with_f64(&db_name, Self::DB_VERSION)
            .map_err(|e| WasmError::from(e))?;

        // Setup upgrade handler
        let (upgrade_tx, upgrade_rx) = oneshot::channel();
        let upgrade_tx = Arc::new(std::sync::Mutex::new(Some(upgrade_tx)));

        let onupgradeneeded = Closure::once(move |event: IdbVersionChangeEvent| {
            let target = event.target().expect("Event should have a target");
            let request = target
                .dyn_ref::<IdbOpenDbRequest>()
                .expect("Target should be IdbOpenDbRequest");
            let db = request.result().unwrap().dyn_into::<IdbDatabase>().unwrap();

            // Create object stores if they don't exist
            if !db.object_store_names().contains("files") {
                let options = IdbObjectStoreParameters::new();
                options.set_key_path(&JsValue::from_str("path"));
                let _ = db.create_object_store_with_optional_parameters("files", &options);
            }

            if !db.object_store_names().contains("chunks") {
                let options = IdbObjectStoreParameters::new();
                options.set_key_path(&JsValue::from_str("id"));
                let _ = db.create_object_store_with_optional_parameters("chunks", &options);
            }

            if let Some(tx) = upgrade_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
        });

        open_request.set_onupgradeneeded(Some(onupgradeneeded.as_ref().unchecked_ref()));
        onupgradeneeded.forget();

        // Wait for success
        let promise = Self::request_to_promise(&open_request)?;
        let result = JsFuture::from(promise).await?;

        // Wait for upgrade to complete if needed
        let _ = upgrade_rx.await;

        let db = result.dyn_into::<IdbDatabase>().map_err(|_| {
            WasmError::IndexedDb("Failed to cast result to IdbDatabase".to_string())
        })?;

        Ok(db)
    }

    /// Convert an IDB request to a Promise
    fn request_to_promise(request: &IdbRequest) -> WasmResult<js_sys::Promise> {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let request_clone = request.clone();
            let onsuccess = Closure::once(move || {
                let result = request_clone.result().unwrap();
                resolve.call1(&JsValue::NULL, &result).unwrap();
            });
            request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
            onsuccess.forget();

            let request_clone = request.clone();
            let onerror = Closure::once(move || {
                let error = request_clone.error().unwrap().unwrap();
                reject.call1(&JsValue::NULL, &error).unwrap();
            });
            request.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            onerror.forget();
        });

        Ok(promise)
    }

    /// Get a transaction for the given store
    fn get_transaction(
        &self,
        store_name: &str,
        mode: IdbTransactionMode,
    ) -> WasmResult<IdbTransaction> {
        let store_names = Array::new();
        store_names.push(&JsValue::from_str(store_name));

        self.db
            .transaction_with_str_sequence_and_mode(&store_names, mode)
            .map_err(|e| WasmError::from(e))
    }

    /// Get an object store from a transaction
    fn get_store<'a>(
        &self,
        transaction: &'a IdbTransaction,
        store_name: &str,
    ) -> WasmResult<IdbObjectStore> {
        transaction
            .object_store(store_name)
            .map_err(|e| WasmError::from(e))
    }

    /// Normalize a path to use forward slashes and remove redundant separators
    fn normalize_path(path: &Path) -> String {
        let path_str = path.to_string_lossy();
        let normalized = path_str.replace('\\', "/");

        // Remove redundant slashes
        let parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    /// Get a file entry from the database
    async fn get_file_entry(&self, path: &Path) -> WasmResult<Option<FileEntry>> {
        let normalized_path = Self::normalize_path(path);

        let transaction = self.get_transaction("files", IdbTransactionMode::Readonly)?;
        let store = self.get_store(&transaction, "files")?;

        let request = store
            .get(&JsValue::from_str(&normalized_path))
            .map_err(|e| WasmError::from(e))?;

        let promise = Self::request_to_promise(&request)?;
        let result = JsFuture::from(promise).await?;

        if result.is_undefined() || result.is_null() {
            return Ok(None);
        }

        let entry: FileEntry = serde_wasm_bindgen::from_value(result)
            .map_err(|e| WasmError::Serialization(serde_io_error(e.to_string())))?;

        Ok(Some(entry))
    }

    /// Put a file entry into the database
    async fn put_file_entry(&self, entry: &FileEntry) -> WasmResult<()> {
        let transaction = self.get_transaction("files", IdbTransactionMode::Readwrite)?;
        let store = self.get_store(&transaction, "files")?;

        let js_entry = serde_wasm_bindgen::to_value(entry)
            .map_err(|e| WasmError::Serialization(serde_io_error(e.to_string())))?;

        let request = store.put(&js_entry).map_err(|e| WasmError::from(e))?;

        let promise = Self::request_to_promise(&request)?;
        JsFuture::from(promise).await?;

        Ok(())
    }

    /// Delete a file entry from the database
    async fn delete_file_entry(&self, path: &Path) -> WasmResult<()> {
        let normalized_path = Self::normalize_path(path);

        let transaction = self.get_transaction("files", IdbTransactionMode::Readwrite)?;
        let store = self.get_store(&transaction, "files")?;

        let request = store
            .delete(&JsValue::from_str(&normalized_path))
            .map_err(|e| WasmError::from(e))?;

        let promise = Self::request_to_promise(&request)?;
        JsFuture::from(promise).await?;

        Ok(())
    }

    /// Store file chunks for large files
    async fn store_chunks(&self, file_path: &str, data: &[u8]) -> WasmResult<usize> {
        let chunk_count = (data.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let transaction = self.get_transaction("chunks", IdbTransactionMode::Readwrite)?;
        let store = self.get_store(&transaction, "chunks")?;

        for (i, chunk_data) in data.chunks(CHUNK_SIZE).enumerate() {
            let chunk = FileChunk {
                file_path: file_path.to_string(),
                chunk_index: i,
                data: base64::encode(chunk_data),
            };

            let chunk_id = format!("{}#{}", file_path, i);
            let js_chunk = Object::new();
            Reflect::set(&js_chunk, &"id".into(), &chunk_id.into())?;
            Reflect::set(&js_chunk, &"file_path".into(), &chunk.file_path.into())?;
            Reflect::set(
                &js_chunk,
                &"chunk_index".into(),
                &(chunk.chunk_index as f64).into(),
            )?;
            Reflect::set(&js_chunk, &"data".into(), &chunk.data.into())?;

            let request = store.put(&js_chunk).map_err(|e| WasmError::from(e))?;
            let promise = Self::request_to_promise(&request)?;
            JsFuture::from(promise).await?;
        }

        Ok(chunk_count)
    }

    /// Load file chunks for large files
    async fn load_chunks(&self, file_path: &str, chunk_count: usize) -> WasmResult<Vec<u8>> {
        let transaction = self.get_transaction("chunks", IdbTransactionMode::Readonly)?;
        let store = self.get_store(&transaction, "chunks")?;

        let mut result = Vec::new();

        for i in 0..chunk_count {
            let chunk_id = format!("{}#{}", file_path, i);
            let request = store
                .get(&JsValue::from_str(&chunk_id))
                .map_err(|e| WasmError::from(e))?;

            let promise = Self::request_to_promise(&request)?;
            let js_result = JsFuture::from(promise).await?;

            if js_result.is_undefined() || js_result.is_null() {
                return Err(WasmError::IndexedDb(format!(
                    "Chunk {} not found for file {}",
                    i, file_path
                )));
            }

            let data_str: String = Reflect::get(&js_result, &"data".into())?
                .as_string()
                .ok_or_else(|| WasmError::IndexedDb("Invalid chunk data".to_string()))?;

            let chunk_data = base64::decode(&data_str)
                .map_err(|e| WasmError::Io(format!("Failed to decode chunk: {}", e)))?;

            result.extend_from_slice(&chunk_data);
        }

        Ok(result)
    }

    /// Delete all chunks for a file
    async fn delete_chunks(&self, file_path: &str, chunk_count: usize) -> WasmResult<()> {
        let transaction = self.get_transaction("chunks", IdbTransactionMode::Readwrite)?;
        let store = self.get_store(&transaction, "chunks")?;

        for i in 0..chunk_count {
            let chunk_id = format!("{}#{}", file_path, i);
            let request = store
                .delete(&JsValue::from_str(&chunk_id))
                .map_err(|e| WasmError::from(e))?;

            let promise = Self::request_to_promise(&request)?;
            JsFuture::from(promise).await?;
        }

        Ok(())
    }

    /// Ensure a directory exists in the database
    async fn ensure_directory_exists(&self, path: &Path) -> WasmResult<()> {
        if path == Path::new("") || path == Path::new("/") {
            return Ok(());
        }

        let mut pending = Vec::new();
        let mut cursor = path.to_path_buf();

        loop {
            if cursor.as_os_str().is_empty() || cursor == Path::new("/") {
                break;
            }

            match self.get_file_entry(cursor.as_path()).await? {
                Some(entry) => {
                    if !entry.is_directory {
                        return Err(WasmError::NotADirectory(cursor.display().to_string()));
                    }
                    break;
                }
                None => pending.push(cursor.clone()),
            }

            if !cursor.pop() {
                break;
            }
        }

        for dir in pending.iter().rev() {
            let now = js_sys::Date::now() as i64;
            let entry = FileEntry {
                path: Self::normalize_path(dir),
                content: None,
                is_chunked: false,
                chunk_count: 0,
                size: 0,
                created_at: now,
                modified_at: now,
                is_directory: true,
            };

            self.put_file_entry(&entry).await?;
        }

        Ok(())
    }

    /// List all entries with a given path prefix
    async fn list_entries_with_prefix(&self, prefix: &str) -> WasmResult<Vec<FileEntry>> {
        let transaction = self.get_transaction("files", IdbTransactionMode::Readonly)?;
        let store = self.get_store(&transaction, "files")?;

        let request = store.open_cursor().map_err(|e| WasmError::from(e))?;

        let mut entries = Vec::new();
        let prefix_normalized = if prefix.ends_with('/') {
            prefix.to_string()
        } else {
            format!("{}/", prefix)
        };

        loop {
            let promise = Self::request_to_promise(&request)?;
            let result = JsFuture::from(promise).await?;

            if result.is_null() || result.is_undefined() {
                break;
            }

            let cursor = result
                .dyn_into::<web_sys::IdbCursorWithValue>()
                .map_err(|_| WasmError::IndexedDb("Failed to cast to cursor".to_string()))?;

            let value = cursor.value().map_err(|e| WasmError::from(e))?;
            let entry: FileEntry = serde_wasm_bindgen::from_value(value)
                .map_err(|e| WasmError::Serialization(serde_io_error(e.to_string())))?;

            if entry.path.starts_with(&prefix_normalized) || entry.path == prefix {
                entries.push(entry);
            }

            cursor.continue_().map_err(|e| WasmError::from(e))?;
        }

        Ok(entries)
    }
}

#[async_trait(?Send)]
impl FileSystemAccess for WasmFileSystem {
    async fn get_cache_directory(&self) -> BridgeResult<PathBuf> {
        Ok(self.cache_dir.clone())
    }

    async fn get_data_directory(&self) -> BridgeResult<PathBuf> {
        Ok(self.data_dir.clone())
    }

    async fn exists(&self, path: &Path) -> BridgeResult<bool> {
        match self.get_file_entry(path).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn metadata(&self, path: &Path) -> BridgeResult<FileMetadata> {
        let entry = self
            .get_file_entry(path)
            .await?
            .ok_or_else(|| WasmError::FileNotFound(path.display().to_string()))?;

        Ok(FileMetadata {
            size: entry.size,
            created_at: Some(entry.created_at / 1000), // Convert ms to seconds
            modified_at: Some(entry.modified_at / 1000),
            is_directory: entry.is_directory,
        })
    }

    async fn create_dir_all(&self, path: &Path) -> BridgeResult<()> {
        self.ensure_directory_exists(path).await?;
        Ok(())
    }

    async fn read_file(&self, path: &Path) -> BridgeResult<Bytes> {
        let entry = self
            .get_file_entry(path)
            .await?
            .ok_or_else(|| WasmError::FileNotFound(path.display().to_string()))?;

        if entry.is_directory {
            return Err(WasmError::NotAFile(path.display().to_string()).into());
        }

        let data = if entry.is_chunked {
            self.load_chunks(&entry.path, entry.chunk_count).await?
        } else if let Some(content) = &entry.content {
            base64::decode(content)
                .map_err(|e| WasmError::Io(format!("Failed to decode file content: {}", e)))?
        } else {
            Vec::new()
        };

        Ok(Bytes::from(data))
    }

    async fn write_file(&self, path: &Path, data: Bytes) -> BridgeResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            self.ensure_directory_exists(parent).await?;
        }

        let now = js_sys::Date::now() as i64;
        let normalized_path = Self::normalize_path(path);
        let size = data.len() as u64;

        // Delete old entry and chunks if it exists
        if let Some(old_entry) = self.get_file_entry(path).await? {
            if old_entry.is_chunked {
                self.delete_chunks(&old_entry.path, old_entry.chunk_count)
                    .await?;
            }
        }

        let (is_chunked, chunk_count, content) = if data.len() > MAX_INLINE_SIZE {
            // Store as chunks
            let chunk_count = self.store_chunks(&normalized_path, &data).await?;
            (true, chunk_count, None)
        } else {
            // Store inline
            (false, 0, Some(base64::encode(&data)))
        };

        let entry = FileEntry {
            path: normalized_path,
            content,
            is_chunked,
            chunk_count,
            size,
            created_at: now,
            modified_at: now,
            is_directory: false,
        };

        self.put_file_entry(&entry).await?;
        Ok(())
    }

    async fn append_file(&self, path: &Path, data: Bytes) -> BridgeResult<()> {
        // Read existing content if file exists
        let existing_data = if self.exists(path).await? {
            self.read_file(path).await?
        } else {
            Bytes::new()
        };

        // Concatenate and write
        let mut combined = existing_data.to_vec();
        combined.extend_from_slice(&data);

        self.write_file(path, Bytes::from(combined)).await
    }

    async fn delete_file(&self, path: &Path) -> BridgeResult<()> {
        let entry = self
            .get_file_entry(path)
            .await?
            .ok_or_else(|| WasmError::FileNotFound(path.display().to_string()))?;

        if entry.is_directory {
            return Err(WasmError::NotAFile(path.display().to_string()).into());
        }

        // Delete chunks if file is chunked
        if entry.is_chunked {
            self.delete_chunks(&entry.path, entry.chunk_count).await?;
        }

        self.delete_file_entry(path).await?;
        Ok(())
    }

    async fn delete_dir_all(&self, path: &Path) -> BridgeResult<()> {
        let entry = self
            .get_file_entry(path)
            .await?
            .ok_or_else(|| WasmError::DirectoryNotFound(path.display().to_string()))?;

        if !entry.is_directory {
            return Err(WasmError::NotADirectory(path.display().to_string()).into());
        }

        // List all entries under this directory
        let normalized_path = Self::normalize_path(path);
        let entries = self.list_entries_with_prefix(&normalized_path).await?;

        // Delete all entries
        for entry in entries {
            if entry.is_chunked {
                self.delete_chunks(&entry.path, entry.chunk_count).await?;
            }
            self.delete_file_entry(&PathBuf::from(&entry.path)).await?;
        }

        // Delete the directory itself
        self.delete_file_entry(path).await?;
        Ok(())
    }

    async fn list_directory(&self, path: &Path) -> BridgeResult<Vec<PathBuf>> {
        let entry = self
            .get_file_entry(path)
            .await?
            .ok_or_else(|| WasmError::DirectoryNotFound(path.display().to_string()))?;

        if !entry.is_directory {
            return Err(WasmError::NotADirectory(path.display().to_string()).into());
        }

        let normalized_path = Self::normalize_path(path);
        let entries = self.list_entries_with_prefix(&normalized_path).await?;

        // Filter to direct children only
        let path_depth = normalized_path.matches('/').count();
        let children: Vec<PathBuf> = entries
            .into_iter()
            .filter(|e| e.path != normalized_path && e.path.matches('/').count() == path_depth + 1)
            .map(|e| PathBuf::from(e.path))
            .collect();

        Ok(children)
    }

    async fn open_read_stream(&self, path: &Path) -> BridgeResult<Box<DynAsyncRead>> {
        // For WASM, we read the entire file into memory and wrap it in a cursor
        let data = self.read_file(path).await?;
        let cursor = futures::io::Cursor::new(data.to_vec());
        Ok(Box::new(cursor) as Box<DynAsyncRead>)
    }

    async fn open_write_stream(&self, _path: &Path) -> BridgeResult<Box<DynAsyncWrite>> {
        // WASM doesn't support true streaming writes to IndexedDB
        // This would require a more complex implementation with buffering
        Err(WasmError::Unsupported(
            "Streaming writes not supported in WASM. Use write_file instead.".to_string(),
        )
        .into())
    }
}

// Base64 encoding/decoding utilities using a pure Rust implementation
mod base64 {
    const BASE64_CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(data: &[u8]) -> String {
        let mut result = String::new();
        let mut i = 0;

        while i < data.len() {
            let b1 = data[i];
            let b2 = if i + 1 < data.len() { data[i + 1] } else { 0 };
            let b3 = if i + 2 < data.len() { data[i + 2] } else { 0 };

            let c1 = (b1 >> 2) as usize;
            let c2 = (((b1 & 0x03) << 4) | (b2 >> 4)) as usize;
            let c3 = (((b2 & 0x0F) << 2) | (b3 >> 6)) as usize;
            let c4 = (b3 & 0x3F) as usize;

            result.push(BASE64_CHARS[c1] as char);
            result.push(BASE64_CHARS[c2] as char);

            if i + 1 < data.len() {
                result.push(BASE64_CHARS[c3] as char);
            } else {
                result.push('=');
            }

            if i + 2 < data.len() {
                result.push(BASE64_CHARS[c4] as char);
            } else {
                result.push('=');
            }

            i += 3;
        }

        result
    }

    pub fn decode(encoded: &str) -> Result<Vec<u8>, String> {
        let encoded = encoded.trim_end_matches('=');
        let mut result = Vec::new();
        let chars: Vec<u8> = encoded.bytes().collect();

        let decode_char = |c: u8| -> Result<u8, String> {
            match c {
                b'A'..=b'Z' => Ok(c - b'A'),
                b'a'..=b'z' => Ok(c - b'a' + 26),
                b'0'..=b'9' => Ok(c - b'0' + 52),
                b'+' => Ok(62),
                b'/' => Ok(63),
                _ => Err(format!("Invalid base64 character: {}", c as char)),
            }
        };

        let mut i = 0;
        while i < chars.len() {
            let c1 = decode_char(chars[i])?;
            let c2 = if i + 1 < chars.len() {
                decode_char(chars[i + 1])?
            } else {
                0
            };
            let c3 = if i + 2 < chars.len() {
                decode_char(chars[i + 2])?
            } else {
                0
            };
            let c4 = if i + 3 < chars.len() {
                decode_char(chars[i + 3])?
            } else {
                0
            };

            result.push((c1 << 2) | (c2 >> 4));

            if i + 2 < chars.len() {
                result.push(((c2 & 0x0F) << 4) | (c3 >> 2));
            }

            if i + 3 < chars.len() {
                result.push(((c3 & 0x03) << 6) | c4);
            }

            i += 4;
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_filesystem_creation() {
        let fs = WasmFileSystem::new("test-app").await.unwrap();
        let cache_dir = fs.get_cache_directory().await.unwrap();
        assert_eq!(cache_dir, PathBuf::from("/cache"));
    }

    #[wasm_bindgen_test]
    async fn test_write_and_read_small_file() {
        let fs = WasmFileSystem::new("test-write-read").await.unwrap();
        let test_path = PathBuf::from("/cache/test.txt");

        let data = Bytes::from("Hello, WASM!");
        fs.write_file(&test_path, data.clone()).await.unwrap();

        let read_data = fs.read_file(&test_path).await.unwrap();
        assert_eq!(data, read_data);
    }

    #[wasm_bindgen_test]
    async fn test_directory_operations() {
        let fs = WasmFileSystem::new("test-dirs").await.unwrap();
        let dir_path = PathBuf::from("/data/test-dir");

        fs.create_dir_all(&dir_path).await.unwrap();
        assert!(fs.exists(&dir_path).await.unwrap());

        let metadata = fs.metadata(&dir_path).await.unwrap();
        assert!(metadata.is_directory);
    }

    #[wasm_bindgen_test]
    async fn test_list_directory() {
        let fs = WasmFileSystem::new("test-list").await.unwrap();
        let dir_path = PathBuf::from("/data/list-test");
        fs.create_dir_all(&dir_path).await.unwrap();

        // Create some files
        let file1 = dir_path.join("file1.txt");
        let file2 = dir_path.join("file2.txt");
        fs.write_file(&file1, Bytes::from("content1"))
            .await
            .unwrap();
        fs.write_file(&file2, Bytes::from("content2"))
            .await
            .unwrap();

        let entries = fs.list_directory(&dir_path).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[wasm_bindgen_test]
    async fn test_delete_file() {
        let fs = WasmFileSystem::new("test-delete").await.unwrap();
        let test_path = PathBuf::from("/cache/delete-me.txt");

        fs.write_file(&test_path, Bytes::from("data"))
            .await
            .unwrap();
        assert!(fs.exists(&test_path).await.unwrap());

        fs.delete_file(&test_path).await.unwrap();
        assert!(!fs.exists(&test_path).await.unwrap());
    }
}
