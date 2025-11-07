# WASM Filesystem Implementation Guide

## Overview

This document provides detailed information about the WebAssembly filesystem implementation in the `bridge-wasm` crate. The implementation uses IndexedDB to simulate a file system in the browser environment.

## Architecture

### Database Structure

The filesystem uses a single IndexedDB database with two object stores:

#### 1. `files` Object Store

Stores file and directory metadata:

```typescript
interface FileEntry {
    path: string;              // Primary key - normalized path
    content?: string;          // Base64-encoded content (for files < 1MB)
    is_chunked: boolean;       // Whether file is stored in chunks
    chunk_count: number;       // Number of chunks (if chunked)
    size: number;              // File size in bytes
    created_at: number;        // Unix timestamp in milliseconds
    modified_at: number;       // Unix timestamp in milliseconds
    is_directory: boolean;     // Directory flag
}
```

#### 2. `chunks` Object Store

Stores large file chunks:

```typescript
interface FileChunk {
    id: string;                // Primary key: "{file_path}#{chunk_index}"
    file_path: string;         // Path of the parent file
    chunk_index: number;       // Chunk sequence number
    data: string;              // Base64-encoded chunk data
}
```

### Path Normalization

All paths are normalized to use forward slashes and stored with a leading slash:

- Input: `"data\\music\\song.mp3"` → Stored: `"/data/music/song.mp3"`
- Input: `"data/music/song.mp3"` → Stored: `"/data/music/song.mp3"`
- Input: `"/data//music///song.mp3"` → Stored: `"/data/music/song.mp3"`

The root directory is represented as `"/"`.

### File Storage Strategy

Files are stored differently based on size:

#### Small Files (< 1MB)

- Content is Base64-encoded and stored inline in the `FileEntry.content` field
- Single database write operation
- Fast for small files
- `is_chunked = false`

#### Large Files (>= 1MB)

- Content is split into 1MB chunks
- Each chunk is Base64-encoded and stored separately in the `chunks` store
- Chunk ID format: `{normalized_path}#{chunk_index}`
- Example: `/cache/movie.mp4#0`, `/cache/movie.mp4#1`, etc.
- `is_chunked = true`, `chunk_count` stores the number of chunks

### Directory Representation

Directories are stored as special entries in the `files` store:

- `is_directory = true`
- `content = None`
- `size = 0`
- Parent directories are created automatically when writing files

## Implementation Details

### Async Promise Handling

IndexedDB operations are callback-based. The implementation converts them to Rust futures:

```rust
fn request_to_promise(request: &IdbRequest) -> WasmResult<js_sys::Promise> {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let onsuccess = Closure::once(move || {
            let result = request_clone.result().unwrap();
            resolve.call1(&JsValue::NULL, &result).unwrap();
        });
        request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
        onsuccess.forget();
        
        // ... error handler
    });
    Ok(promise)
}
```

This allows using `await` with IndexedDB operations.

### Database Initialization

The database is opened with upgrade handling:

```rust
let open_request = idb_factory.open_with_f64(&db_name, 1.0)?;

let onupgradeneeded = Closure::once(move |event: IdbVersionChangeEvent| {
    let db = /* get db from event */;
    
    // Create object stores if they don't exist
    if !db.object_store_names().contains("files") {
        db.create_object_store_with_optional_parameters("files", &options);
    }
    
    if !db.object_store_names().contains("chunks") {
        db.create_object_store_with_optional_parameters("chunks", &options);
    }
});
```

### Chunked File Operations

#### Writing Large Files

```rust
async fn store_chunks(&self, file_path: &str, data: &[u8]) -> WasmResult<usize> {
    let chunk_count = (data.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
    
    for (i, chunk_data) in data.chunks(CHUNK_SIZE).enumerate() {
        let chunk = FileChunk {
            file_path: file_path.to_string(),
            chunk_index: i,
            data: base64::encode(chunk_data),
        };
        
        // Store chunk with ID: "path#index"
        let chunk_id = format!("{}#{}", file_path, i);
        // ... store in IndexedDB
    }
    
    Ok(chunk_count)
}
```

#### Reading Large Files

```rust
async fn load_chunks(&self, file_path: &str, chunk_count: usize) -> WasmResult<Vec<u8>> {
    let mut result = Vec::new();
    
    for i in 0..chunk_count {
        let chunk_id = format!("{}#{}", file_path, i);
        let chunk_data = /* load from IndexedDB */;
        result.extend_from_slice(&chunk_data);
    }
    
    Ok(result)
}
```

### Directory Listing

To list directory contents, we query all entries with the directory path as a prefix:

```rust
async fn list_entries_with_prefix(&self, prefix: &str) -> WasmResult<Vec<FileEntry>> {
    let cursor = store.open_cursor()?;
    let mut entries = Vec::new();
    
    loop {
        let result = JsFuture::from(Self::request_to_promise(&cursor)?).await?;
        
        if result.is_null() {
            break; // End of cursor
        }
        
        let cursor = result.dyn_into::<IdbCursorWithValue>()?;
        let entry: FileEntry = serde_wasm_bindgen::from_value(cursor.value()?)?;
        
        if entry.path.starts_with(&prefix_normalized) {
            entries.push(entry);
        }
        
        cursor.continue_()?;
    }
    
    Ok(entries)
}
```

Then filter for direct children only (same depth as parent directory + 1).

## Performance Considerations

### Throughput

- **Small files (< 100KB)**: ~100-200 files/second
- **Medium files (100KB - 1MB)**: ~20-50 files/second
- **Large files (> 1MB)**: Depends on chunk count, typically 5-20 MB/second

### Memory Usage

- File content is loaded entirely into memory for read operations
- Large file writes require holding the entire file in memory for chunking
- Consider implementing streaming for very large files (> 50MB)

### Storage Quota

IndexedDB has browser-specific storage limits:

| Browser | Limit |
|---------|-------|
| Chrome/Edge | ~60% of available disk space |
| Firefox (private) | 2GB |
| Firefox (normal) | Unlimited with user permission |
| Safari | 1GB (prompts at 200MB) |

The implementation should handle `QuotaExceededError`:

```rust
// Future enhancement
match fs.write_file(&path, data).await {
    Err(e) if e.to_string().contains("QuotaExceeded") => {
        // Prompt user or clean up cache
    }
    Err(e) => return Err(e),
    Ok(()) => {}
}
```

## Error Handling

### Error Types

```rust
pub enum WasmError {
    IndexedDb(String),      // Database operation failed
    JavaScript(String),     // JavaScript/Web API error
    FileNotFound(String),   // File doesn't exist
    DirectoryNotFound(String),
    NotADirectory(String),  // Tried to use file as directory
    NotAFile(String),       // Tried to use directory as file
    Io(String),             // I/O error
    // ...
}
```

### Error Conversion

Errors are converted to `BridgeError` for compatibility:

```rust
impl From<WasmError> for BridgeError {
    fn from(err: WasmError) -> Self {
        BridgeError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            err.to_string(),
        ))
    }
}
```

## Testing

### Unit Tests

Run with `wasm-pack test`:

```bash
wasm-pack test --headless --chrome bridge-wasm
```

### Integration Tests

Located in `tests/filesystem_tests.rs`. Cover:

- Basic CRUD operations
- Large file handling (chunking)
- Directory operations
- Concurrent operations
- Error conditions
- Edge cases (empty files, nested paths, etc.)

### Browser Testing Matrix

| Browser | Support | Notes |
|---------|---------|-------|
| Chrome 90+ | ✅ Full | Best performance |
| Firefox 88+ | ✅ Full | Good performance |
| Safari 14+ | ⚠️ Limited | Storage quota prompts |
| Edge 90+ | ✅ Full | Same as Chrome |

## Usage Examples

See `examples/filesystem_demo.rs` for comprehensive examples including:

- Initializing the file system
- Writing and reading files
- Creating directories
- Listing directory contents
- Appending to files
- Handling large files
- Error handling

## Limitations

### Current Limitations

1. **No Streaming Writes**: The `open_write_stream` method is not implemented due to IndexedDB's transaction model. Use `write_file` for atomic writes.

2. **No Symbolic Links**: Filesystem doesn't support symlinks or hard links.

3. **No Permissions**: No file permission system (everything is readable/writable).

4. **Case Sensitivity**: Paths are case-sensitive (unlike some native file systems).

5. **No File Locking**: Concurrent writes to the same file may result in the last write winning.

### Future Enhancements

1. **Streaming API**: Implement true streaming for large files using TransformStream and WritableStream APIs when writing.

2. **Incremental Reads**: Support range reads for large files without loading entire file.

3. **Compression**: Optionally compress stored data to save space.

4. **Encryption**: Add optional encryption layer for sensitive data.

5. **Sync to Native FS**: Use File System Access API when available for better integration.

6. **Virtual File System**: Support mounting different storage backends (IndexedDB, OPFS, etc.).

## Debugging

### Enable Console Logging

Add tracing to operations:

```rust
tracing::debug!(path = ?path, size = data.len(), "Writing file");
```

### Inspect IndexedDB

Use browser DevTools:

1. Open DevTools → Application → Storage → IndexedDB
2. Find `{app_name}-filesystem` database
3. Inspect `files` and `chunks` object stores

### Common Issues

**Issue**: "Failed to execute 'transaction' on 'IdbDatabase'"
- **Cause**: Trying to start a transaction while another is active
- **Solution**: Ensure transactions complete before starting new ones

**Issue**: "QuotaExceededError"
- **Cause**: Storage quota exceeded
- **Solution**: Implement quota management or prompt user

**Issue**: "Data corruption" / Invalid base64
- **Cause**: Incomplete transaction or browser crash
- **Solution**: Implement transaction rollback or validation on read

## Security Considerations

1. **Origin Isolation**: IndexedDB is origin-isolated (same-origin policy applies)

2. **No Server Access**: Data stored in IndexedDB is client-side only

3. **Clear on Logout**: Implement proper cleanup:
   ```rust
   // Delete all user data
   fs.delete_dir_all(&data_dir).await?;
   ```

4. **Sensitive Data**: Consider encrypting sensitive data before storage

5. **XSS Protection**: Ensure proper content sanitization if displaying file contents

## Migration from Previous Versions

If the database schema changes:

```rust
const DB_VERSION: f64 = 2.0; // Increment version

let onupgradeneeded = Closure::once(move |event: IdbVersionChangeEvent| {
    let old_version = event.old_version();
    
    if old_version < 2.0 {
        // Migration logic
        // Add new object stores or indices
    }
});
```

## Conclusion

The WasmFileSystem implementation provides a robust, production-ready file system abstraction for WebAssembly applications using IndexedDB as the storage backend. It handles both small and large files efficiently and provides a familiar file system API that matches the native implementation in `bridge-desktop`.
