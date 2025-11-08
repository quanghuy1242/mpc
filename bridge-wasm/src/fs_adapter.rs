//! Adapter to make WasmFileSystem compatible with core_async::fs::WasmFileSystemOps
//!
//! This module provides the glue layer between the bridge-traits FileSystemAccess
//! implementation (WasmFileSystem) and the core_async::fs API expectations. It allows
//! core-async to expose a Tokio-like filesystem API while using the IndexedDB-backed
//! storage from bridge-wasm.

use bytes::Bytes;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
};

use bridge_traits::storage::FileSystemAccess;
use crate::filesystem::WasmFileSystem;

/// Adapter that implements WasmFileSystemOps for WasmFileSystem
///
/// This allows WasmFileSystem (which implements FileSystemAccess) to be used
/// with core_async::fs which expects WasmFileSystemOps. Uses Rc since WASM is
/// single-threaded and core_async::fs expects Rc<dyn WasmFileSystemOps>.
pub struct WasmFileSystemAdapter {
    inner: Rc<WasmFileSystem>,
}

impl WasmFileSystemAdapter {
    /// Create a new adapter from an Rc to the filesystem
    pub fn new(fs: Rc<WasmFileSystem>) -> Self {
        Self { inner: fs }
    }
}

impl core_async::fs::WasmFileSystemOps for WasmFileSystemAdapter {
    fn read_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Bytes, String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .read_file(&path)
                .await
                .map_err(|e| format!("Failed to read file: {}", e))
        })
    }

    fn write_file<'a>(
        &'a self,
        path: &'a Path,
        data: Bytes,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .write_file(&path, data)
                .await
                .map_err(|e| format!("Failed to write file: {}", e))
        })
    }

    fn delete_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .delete_file(&path)
                .await
                .map_err(|e| format!("Failed to delete file: {}", e))
        })
    }

    fn delete_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .delete_dir_all(&path)
                .await
                .map_err(|e| format!("Failed to delete directory: {}", e))
        })
    }

    fn create_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .create_dir_all(&path)
                .await
                .map_err(|e| format!("Failed to create directory: {}", e))
        })
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PathBuf>, String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .list_directory(&path)
                .await
                .map_err(|e| format!("Failed to list directory: {}", e))
        })
    }

    fn metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<core_async::fs::BridgeFileMetadata, String>> + 'a>>
    {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            let meta = inner
                .metadata(&path)
                .await
                .map_err(|e| format!("Failed to get metadata: {}", e))?;

            Ok(core_async::fs::BridgeFileMetadata {
                size: meta.size,
                created_at: meta.created_at,
                modified_at: meta.modified_at,
                is_directory: meta.is_directory,
            })
        })
    }

    fn exists<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + 'a>> {
        let inner = Rc::clone(&self.inner);
        let path = path.to_path_buf();
        Box::pin(async move {
            inner
                .exists(&path)
                .await
                .map_err(|e| format!("Failed to check existence: {}", e))
        })
    }
}
