#![cfg(target_arch = "wasm32")]
//! Integration tests for WasmFileSystem
//!
//! These tests verify the complete functionality of the IndexedDB-based
//! file system implementation.
//!
use bridge_traits::storage::FileSystemAccess;
use bridge_wasm::WasmFileSystem;
use bytes::Bytes;
use futures::future::join_all;
use std::path::PathBuf;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Test basic file system initialization
#[wasm_bindgen_test]
async fn test_initialization() {
    let fs = WasmFileSystem::new("test-init").await.unwrap();

    let cache_dir = fs.get_cache_directory().await.unwrap();
    let data_dir = fs.get_data_directory().await.unwrap();

    assert_eq!(cache_dir, PathBuf::from("/cache"));
    assert_eq!(data_dir, PathBuf::from("/data"));

    // Verify directories were created
    assert!(fs.exists(&cache_dir).await.unwrap());
    assert!(fs.exists(&data_dir).await.unwrap());
}

/// Test file write and read operations
#[wasm_bindgen_test]
async fn test_file_write_and_read() {
    let fs = WasmFileSystem::new("test-file-ops").await.unwrap();
    let test_file = PathBuf::from("/data/test-file.txt");

    // Write file
    let content = Bytes::from("Hello, WebAssembly!");
    fs.write_file(&test_file, content.clone()).await.unwrap();

    // Verify file exists
    assert!(fs.exists(&test_file).await.unwrap());

    // Read file back
    let read_content = fs.read_file(&test_file).await.unwrap();
    assert_eq!(content, read_content);
}

/// Test writing and reading large files (chunked storage)
#[wasm_bindgen_test]
async fn test_large_file_operations() {
    let fs = WasmFileSystem::new("test-large-file").await.unwrap();
    let large_file = PathBuf::from("/cache/large-file.bin");

    // Create a file larger than 1MB to trigger chunking
    let size = 2 * 1024 * 1024; // 2MB
    let mut large_data = Vec::with_capacity(size);
    for i in 0..size {
        large_data.push((i % 256) as u8);
    }
    let content = Bytes::from(large_data);

    // Write large file
    fs.write_file(&large_file, content.clone()).await.unwrap();

    // Read it back
    let read_content = fs.read_file(&large_file).await.unwrap();
    assert_eq!(content.len(), read_content.len());
    assert_eq!(content, read_content);

    // Verify metadata
    let metadata = fs.metadata(&large_file).await.unwrap();
    assert_eq!(metadata.size, size as u64);
    assert!(!metadata.is_directory);
}

/// Test file metadata retrieval
#[wasm_bindgen_test]
async fn test_file_metadata() {
    let fs = WasmFileSystem::new("test-metadata").await.unwrap();
    let test_file = PathBuf::from("/data/metadata-test.txt");

    let content = Bytes::from("test content");
    fs.write_file(&test_file, content.clone()).await.unwrap();

    let metadata = fs.metadata(&test_file).await.unwrap();

    assert_eq!(metadata.size, content.len() as u64);
    assert!(!metadata.is_directory);
    assert!(metadata.created_at.is_some());
    assert!(metadata.modified_at.is_some());
}

/// Test directory creation
#[wasm_bindgen_test]
async fn test_directory_creation() {
    let fs = WasmFileSystem::new("test-dir-create").await.unwrap();
    let nested_dir = PathBuf::from("/data/level1/level2/level3");

    // Create nested directories
    fs.create_dir_all(&nested_dir).await.unwrap();

    // Verify all levels exist
    assert!(fs.exists(&PathBuf::from("/data/level1")).await.unwrap());
    assert!(fs
        .exists(&PathBuf::from("/data/level1/level2"))
        .await
        .unwrap());
    assert!(fs.exists(&nested_dir).await.unwrap());

    // Verify it's a directory
    let metadata = fs.metadata(&nested_dir).await.unwrap();
    assert!(metadata.is_directory);
}

/// Test directory listing
#[wasm_bindgen_test]
async fn test_directory_listing() {
    let fs = WasmFileSystem::new("test-dir-list").await.unwrap();
    let test_dir = PathBuf::from("/data/list-test");

    fs.create_dir_all(&test_dir).await.unwrap();

    // Create several files
    let files = vec!["file1.txt", "file2.txt", "file3.txt"];
    for file_name in &files {
        let file_path = test_dir.join(file_name);
        fs.write_file(&file_path, Bytes::from("content"))
            .await
            .unwrap();
    }

    // Create a subdirectory
    let subdir = test_dir.join("subdir");
    fs.create_dir_all(&subdir).await.unwrap();

    // List directory
    let entries = fs.list_directory(&test_dir).await.unwrap();

    // Should have 3 files + 1 directory = 4 entries
    assert_eq!(entries.len(), 4);
}

/// Test file append operation
#[wasm_bindgen_test]
async fn test_file_append() {
    let fs = WasmFileSystem::new("test-append").await.unwrap();
    let test_file = PathBuf::from("/cache/append-test.txt");

    // Write initial content
    fs.write_file(&test_file, Bytes::from("Hello "))
        .await
        .unwrap();

    // Append more content
    fs.append_file(&test_file, Bytes::from("World!"))
        .await
        .unwrap();

    // Read and verify
    let content = fs.read_file(&test_file).await.unwrap();
    assert_eq!(content, Bytes::from("Hello World!"));
}

/// Test file deletion
#[wasm_bindgen_test]
async fn test_file_deletion() {
    let fs = WasmFileSystem::new("test-delete-file").await.unwrap();
    let test_file = PathBuf::from("/data/delete-me.txt");

    // Create file
    fs.write_file(&test_file, Bytes::from("data"))
        .await
        .unwrap();
    assert!(fs.exists(&test_file).await.unwrap());

    // Delete file
    fs.delete_file(&test_file).await.unwrap();
    assert!(!fs.exists(&test_file).await.unwrap());
}

/// Test recursive directory deletion
#[wasm_bindgen_test]
async fn test_directory_deletion() {
    let fs = WasmFileSystem::new("test-delete-dir").await.unwrap();
    let test_dir = PathBuf::from("/data/delete-dir-test");

    // Create directory with files and subdirectories
    fs.create_dir_all(&test_dir).await.unwrap();
    fs.write_file(&test_dir.join("file1.txt"), Bytes::from("data1"))
        .await
        .unwrap();
    fs.write_file(&test_dir.join("file2.txt"), Bytes::from("data2"))
        .await
        .unwrap();

    let subdir = test_dir.join("subdir");
    fs.create_dir_all(&subdir).await.unwrap();
    fs.write_file(&subdir.join("nested.txt"), Bytes::from("nested data"))
        .await
        .unwrap();

    // Delete entire directory tree
    fs.delete_dir_all(&test_dir).await.unwrap();

    // Verify everything is gone
    assert!(!fs.exists(&test_dir).await.unwrap());
    assert!(!fs.exists(&test_dir.join("file1.txt")).await.unwrap());
    assert!(!fs.exists(&subdir).await.unwrap());
}

/// Test directory size calculation
#[wasm_bindgen_test]
async fn test_directory_size() {
    let fs = WasmFileSystem::new("test-dir-size").await.unwrap();
    let test_dir = PathBuf::from("/data/size-test");

    fs.create_dir_all(&test_dir).await.unwrap();

    // Create files with known sizes
    fs.write_file(&test_dir.join("file1.txt"), Bytes::from("12345"))
        .await
        .unwrap(); // 5 bytes
    fs.write_file(&test_dir.join("file2.txt"), Bytes::from("1234567890"))
        .await
        .unwrap(); // 10 bytes

    let subdir = test_dir.join("subdir");
    fs.create_dir_all(&subdir).await.unwrap();
    fs.write_file(&subdir.join("file3.txt"), Bytes::from("123"))
        .await
        .unwrap(); // 3 bytes

    // Calculate total size
    let total_size = fs.directory_size(&test_dir).await.unwrap();

    // Should be 5 + 10 + 3 = 18 bytes
    assert_eq!(total_size, 18);
}

/// Test overwriting existing file
#[wasm_bindgen_test]
async fn test_file_overwrite() {
    let fs = WasmFileSystem::new("test-overwrite").await.unwrap();
    let test_file = PathBuf::from("/cache/overwrite.txt");

    // Write initial content
    fs.write_file(&test_file, Bytes::from("original content"))
        .await
        .unwrap();

    // Overwrite with new content
    let new_content = Bytes::from("new content");
    fs.write_file(&test_file, new_content.clone())
        .await
        .unwrap();

    // Verify new content
    let read_content = fs.read_file(&test_file).await.unwrap();
    assert_eq!(read_content, new_content);
}

/// Test file operations with nested paths
#[wasm_bindgen_test]
async fn test_nested_file_operations() {
    let fs = WasmFileSystem::new("test-nested").await.unwrap();
    let nested_file = PathBuf::from("/data/a/b/c/d/file.txt");

    // Write file (should create parent directories automatically)
    fs.write_file(&nested_file, Bytes::from("nested data"))
        .await
        .unwrap();

    // Verify file and all parents exist
    assert!(fs.exists(&nested_file).await.unwrap());
    assert!(fs.exists(&PathBuf::from("/data/a")).await.unwrap());
    assert!(fs.exists(&PathBuf::from("/data/a/b")).await.unwrap());
    assert!(fs.exists(&PathBuf::from("/data/a/b/c")).await.unwrap());
    assert!(fs.exists(&PathBuf::from("/data/a/b/c/d")).await.unwrap());
}

/// Test error handling for non-existent files
#[wasm_bindgen_test]
async fn test_error_file_not_found() {
    let fs = WasmFileSystem::new("test-errors").await.unwrap();
    let non_existent = PathBuf::from("/data/does-not-exist.txt");

    // Reading non-existent file should error
    let result = fs.read_file(&non_existent).await;
    assert!(result.is_err());

    // Getting metadata for non-existent file should error
    let result = fs.metadata(&non_existent).await;
    assert!(result.is_err());
}

/// Test error handling for invalid operations
#[wasm_bindgen_test]
async fn test_error_invalid_operations() {
    let fs = WasmFileSystem::new("test-invalid-ops").await.unwrap();
    let test_dir = PathBuf::from("/data/test-dir");

    fs.create_dir_all(&test_dir).await.unwrap();

    // Try to read a directory as a file
    let result = fs.read_file(&test_dir).await;
    assert!(result.is_err());

    // Try to delete a directory as a file
    let result = fs.delete_file(&test_dir).await;
    assert!(result.is_err());
}

/// Test path normalization
#[wasm_bindgen_test]
async fn test_path_normalization() {
    let fs = WasmFileSystem::new("test-paths").await.unwrap();

    // Different path representations for the same location
    let path1 = PathBuf::from("/data/test.txt");
    let path2 = PathBuf::from("/data//test.txt"); // Double slash
    let path3 = PathBuf::from("data/test.txt"); // No leading slash

    // Write using one path
    fs.write_file(&path1, Bytes::from("data")).await.unwrap();

    // Should be accessible via normalized paths
    assert!(fs.exists(&path1).await.unwrap());
    assert!(fs.exists(&path2).await.unwrap());
    assert!(fs.exists(&path3).await.unwrap());

    // Note: Exact behavior depends on path normalization implementation
}

/// Test concurrent file operations
#[wasm_bindgen_test]
async fn test_concurrent_operations() {
    let fs = WasmFileSystem::new("test-concurrent").await.unwrap();
    let test_dir = PathBuf::from("/cache/concurrent");

    fs.create_dir_all(&test_dir).await.unwrap();

    // Create multiple files concurrently
    let mut futures = Vec::new();
    for i in 0..10 {
        let file_path = test_dir.join(format!("file{}.txt", i));
        let content = Bytes::from(format!("content {}", i));
        let fs_ref = &fs;

        futures.push(async move { fs_ref.write_file(&file_path, content).await });
    }

    join_all(futures).await.into_iter().for_each(|res| {
        res.unwrap();
    });

    // Verify all files were created
    let entries = fs.list_directory(&test_dir).await.unwrap();
    assert_eq!(entries.len(), 10);
}

/// Test empty file operations
#[wasm_bindgen_test]
async fn test_empty_file() {
    let fs = WasmFileSystem::new("test-empty").await.unwrap();
    let empty_file = PathBuf::from("/data/empty.txt");

    // Write empty file
    fs.write_file(&empty_file, Bytes::new()).await.unwrap();

    // Verify it exists and has zero size
    assert!(fs.exists(&empty_file).await.unwrap());

    let metadata = fs.metadata(&empty_file).await.unwrap();
    assert_eq!(metadata.size, 0);

    // Read should return empty bytes
    let content = fs.read_file(&empty_file).await.unwrap();
    assert_eq!(content.len(), 0);
}

/// Test binary file operations
#[wasm_bindgen_test]
async fn test_binary_file() {
    let fs = WasmFileSystem::new("test-binary").await.unwrap();
    let binary_file = PathBuf::from("/cache/binary.bin");

    // Create binary data with all byte values
    let mut binary_data = Vec::with_capacity(256);
    for i in 0..256 {
        binary_data.push(i as u8);
    }
    let content = Bytes::from(binary_data);

    // Write and read back
    fs.write_file(&binary_file, content.clone()).await.unwrap();
    let read_content = fs.read_file(&binary_file).await.unwrap();

    assert_eq!(content, read_content);
}
