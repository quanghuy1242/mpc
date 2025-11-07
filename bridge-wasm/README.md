# bridge-wasm

WebAssembly-compatible implementations of bridge traits for the Music Platform Core library.

## Overview

This crate provides WebAssembly implementations of the platform abstraction traits defined in `bridge-traits`. These implementations use browser APIs through `web-sys` and `wasm-bindgen` to provide functionality equivalent to the native `bridge-desktop` implementations.

## Features

### WasmFileSystem

A complete file system abstraction using IndexedDB as the storage backend. This implementation:

- **Persistent Storage**: Uses IndexedDB for reliable, persistent storage in the browser
- **Large File Support**: Automatically chunks files larger than 1MB for efficient storage
- **Full API Compatibility**: Implements all methods from the `FileSystemAccess` trait
- **Path Normalization**: Handles cross-platform path differences automatically
- **Directory Hierarchy**: Supports nested directories with automatic parent creation

#### Architecture

The file system uses two IndexedDB object stores:

1. **`files`**: Stores file and directory metadata, including:
   - Path (used as the primary key)
   - Content (for files < 1MB, base64-encoded)
   - Size, timestamps, and flags
   - Chunking information for large files

2. **`chunks`**: Stores file chunks for files > 1MB:
   - Chunk ID (composite key: `file_path#chunk_index`)
   - Chunk data (base64-encoded)

#### Usage

```rust
use bridge_wasm::WasmFileSystem;
use bridge_traits::storage::FileSystemAccess;
use bytes::Bytes;
use std::path::PathBuf;

#[wasm_bindgen_test]
async fn example() {
    // Initialize the file system
    let fs = WasmFileSystem::new("my-app").await.unwrap();

    // Get standard directories
    let cache_dir = fs.get_cache_directory().await.unwrap();
    let data_dir = fs.get_data_directory().await.unwrap();

    // Write a file
    let file_path = data_dir.join("music/favorites.json");
    let data = Bytes::from(r#"{"tracks": []}"#);
    fs.write_file(&file_path, data).await.unwrap();

    // Read the file back
    let content = fs.read_file(&file_path).await.unwrap();
    println!("File content: {}", String::from_utf8_lossy(&content));

    // List directory contents
    let music_dir = data_dir.join("music");
    let entries = fs.list_directory(&music_dir).await.unwrap();
    for entry in entries {
        println!("Found: {:?}", entry);
    }

    // Delete a file
    fs.delete_file(&file_path).await.unwrap();
}
```

## Limitations

### File System

1. **No Streaming Writes**: The `open_write_stream` method is not supported due to IndexedDB limitations. Use `write_file` for atomic writes instead.

2. **Storage Quota**: IndexedDB has storage limits that vary by browser:
   - Chrome/Edge: ~60% of available disk space
   - Firefox: Up to 2GB in private browsing, unlimited with user permission in normal mode
   - Safari: 1GB limit, asks user permission at 200MB

3. **Performance**: File operations are asynchronous and involve serialization, which is slower than native file I/O. Consider caching frequently accessed data.

4. **Path Limitations**: 
   - No symbolic links or hard links
   - Case-sensitive paths
   - Forward slashes are normalized automatically

## Building for WASM

To build this crate for WebAssembly:

```bash
# Install wasm-pack if you haven't already
cargo install wasm-pack

# Build for the browser
wasm-pack build --target web bridge-wasm

# Or build for node.js
wasm-pack build --target nodejs bridge-wasm
```

## Testing

Tests use `wasm-bindgen-test` and must run in a browser environment:

```bash
# Run tests in headless Chrome
wasm-pack test --headless --chrome bridge-wasm

# Run tests in Firefox
wasm-pack test --headless --firefox bridge-wasm

# Run in actual browser (opens browser window)
wasm-pack test --chrome bridge-wasm
```

## Future Implementations

Additional bridge trait implementations planned for this crate:

- [ ] **WasmHttpClient**: Fetch API-based HTTP client
- [ ] **WasmSecureStore**: WebCrypto + encrypted IndexedDB for credential storage
- [ ] **WasmSettingsStore**: localStorage-based settings persistence
- [ ] **WasmNetworkMonitor**: Navigator connection API for network status

## Dependencies

Key dependencies:

- `wasm-bindgen`: Rust/JavaScript interop
- `web-sys`: Raw bindings to Web APIs
- `js-sys`: Bindings to JavaScript standard library
- `serde-wasm-bindgen`: Efficient serialization between Rust and JS
- `futures`: Async runtime for WASM

## License

Same as the parent workspace: MIT OR Apache-2.0
