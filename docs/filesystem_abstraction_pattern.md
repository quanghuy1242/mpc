# Filesystem Abstraction Pattern

## Overview

This document describes how we handle file operations in a cross-platform way across native and WASM targets using the `FileSystemAccess` trait from `bridge-traits`.

## Architecture Principle

> **All core business logic modules must be compilable for both native and WASM targets.**

To achieve this, we use platform abstraction traits instead of conditional compilation where possible. File operations are one area where this pattern is critical.

## The FileSystemAccess Trait

Located in `bridge-traits/src/storage.rs`, this trait defines a common interface for file operations:

```rust
pub trait FileSystemAccess: PlatformSendSync {
    async fn read_file(&self, path: &Path) -> BridgeResult<Bytes>;
    async fn write_file(&self, path: &Path, data: &[u8]) -> BridgeResult<()>;
    async fn append_file(&self, path: &Path, data: &[u8]) -> BridgeResult<()>;
    async fn exists(&self, path: &Path) -> BridgeResult<bool>;
    async fn metadata(&self, path: &Path) -> BridgeResult<FileMetadata>;
    async fn create_dir_all(&self, path: &Path) -> BridgeResult<()>;
    async fn list_directory(&self, path: &Path) -> BridgeResult<Vec<DirectoryEntry>>;
    async fn delete_file(&self, path: &Path) -> BridgeResult<()>;
    async fn delete_dir_all(&self, path: &Path) -> BridgeResult<()>;
    async fn get_cache_directory(&self) -> BridgeResult<PathBuf>;
    async fn get_data_directory(&self) -> BridgeResult<PathBuf>;
}
```

## Platform Implementations

### Native Platform
On native platforms (Windows, macOS, Linux), file operations use the actual operating system filesystem via `std::fs` or `core_async::fs`.

**Implementation**: The desktop bridge will provide a `NativeFileSystem` implementation that wraps standard file I/O.

### WASM Platform
On WASM, there is no native filesystem. Instead, we use browser APIs:

**Implementation**: `bridge-wasm/src/filesystem.rs` provides `WasmFileSystem` which:
- Uses **IndexedDB** as storage backend
- Stores file data with Base64 encoding
- Supports chunking for files larger than 1MB
- Provides full directory hierarchy simulation
- Handles up to ~50MB+ of data (IndexedDB quota limits)

Key features:
```rust
pub struct WasmFileSystem {
    db_name: String,
    store_name: String,
}

impl WasmFileSystem {
    pub fn new(db_name: &str) -> Self { ... }
    
    // Full FileSystemAccess trait implementation
    // Uses IndexedDB under the hood
}
```

## Usage Pattern in Core Modules

### Pattern 1: Conditional Compilation (Platform-Specific Entry Points)

For methods that are entry points from the application layer:

```rust
// Native: Read file directly from OS filesystem
#[cfg(not(target_arch = "wasm32"))]
pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedMetadata> {
    let file_data = core_async::fs::read(path).await?;
    self.extract_from_bytes(&file_data, path).await
}

// WASM: Accept filesystem abstraction as parameter
#[cfg(target_arch = "wasm32")]
pub async fn extract_from_file<F>(&self, path: &Path, fs: &F) -> Result<ExtractedMetadata>
where
    F: FileSystemAccess,
{
    let file_data = fs.read_file(path).await?;
    self.extract_from_bytes(file_data.as_ref(), path).await
}

// Shared business logic that works on bytes
async fn extract_from_bytes(&self, file_data: &[u8], path: &Path) -> Result<ExtractedMetadata> {
    // Parse file_data using lofty library
    // This logic is 100% identical on both platforms
}
```

**Key Points**:
1. Platform-specific methods for reading the file
2. Shared implementation for processing the data
3. Native uses `core_async::fs::read()` directly
4. WASM accepts `FileSystemAccess` trait parameter
5. Both call into the same business logic

### Pattern 2: Trait-Based (Internal Services)

For internal services that need filesystem access throughout their lifetime:

```rust
pub struct MetadataProcessor {
    file_system: Arc<dyn FileSystemAccess>,
    metadata_extractor: Arc<MetadataExtractor>,
    // ... other fields
}

impl MetadataProcessor {
    pub fn new(file_system: Arc<dyn FileSystemAccess>, /* ... */) -> Self {
        Self {
            file_system,
            // ...
        }
    }
    
    async fn download_and_cache(&self, work_item: &WorkItem) -> Result<(PathBuf, u64)> {
        // Get cache directory using the abstraction
        let cache_dir = self.file_system.get_cache_directory().await?;
        
        // Create temp directory
        let temp_dir = cache_dir.join("sync_temp");
        self.file_system.create_dir_all(&temp_dir).await?;
        
        // Download and write file
        let data = provider.download(file_id).await?;
        self.file_system.write_file(&temp_path, &data).await?;
        
        Ok((temp_path, data.len() as u64))
    }
}
```

**Key Points**:
1. Service accepts `Arc<dyn FileSystemAccess>` in constructor
2. All file operations use the trait methods
3. Code is identical regardless of platform
4. Caller provides the appropriate implementation (native or WASM)

## Real-World Example: Metadata Extraction

### Before (Conditional Compilation)
```rust
#[cfg(not(target_arch = "wasm32"))]
pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedMetadata> {
    // Full implementation only on native
}

// WASM: No implementation - feature disabled
```

**Problems**:
- ❌ Reduces functionality on WASM
- ❌ Violates "universal business logic" principle
- ❌ Makes testing harder (can't test WASM path)

### After (Abstraction Pattern)
```rust
#[cfg(not(target_arch = "wasm32"))]
pub async fn extract_from_file(&self, path: &Path) -> Result<ExtractedMetadata> {
    let file_data = core_async::fs::read(path).await?;
    self.extract_from_bytes(&file_data, path).await
}

#[cfg(target_arch = "wasm32")]
pub async fn extract_from_file<F>(&self, path: &Path, fs: &F) -> Result<ExtractedMetadata>
where
    F: FileSystemAccess,
{
    let file_data = fs.read_file(path).await?;
    self.extract_from_bytes(file_data.as_ref(), path).await
}

async fn extract_from_bytes(&self, file_data: &[u8], path: &Path) -> Result<ExtractedMetadata> {
    // Shared parsing logic using lofty library
    // Works identically on both platforms
}
```

**Benefits**:
- ✅ Full functionality on both platforms
- ✅ Shared business logic (lofty parsing)
- ✅ Testable on both platforms
- ✅ Clear separation: I/O vs. processing

## Testing Strategy

### Native Tests
```rust
#[tokio::test]
async fn test_extract_from_file() {
    let extractor = MetadataExtractor::new();
    let metadata = extractor.extract_from_file(Path::new("test.flac")).await.unwrap();
    assert!(metadata.duration_ms > 0);
}
```

### WASM Tests
```rust
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
async fn test_extract_from_file_wasm() {
    let fs = WasmFileSystem::new("test_db");
    
    // Setup: Store test file in IndexedDB
    let test_data = include_bytes!("fixtures/test.flac");
    fs.write_file(Path::new("test.flac"), test_data).await.unwrap();
    
    // Test extraction
    let extractor = MetadataExtractor::new();
    let metadata = extractor.extract_from_file(Path::new("test.flac"), &fs).await.unwrap();
    assert!(metadata.duration_ms > 0);
}
```

## When to Use Each Pattern

### Use Conditional Compilation (Pattern 1) When:
- You have a public API that's an entry point from the application layer
- The native platform can optimize by reading directly from OS filesystem
- The method is called infrequently (startup, user actions)
- Examples: `extract_from_file()`, `load_config()`

### Use Trait-Based (Pattern 2) When:
- You have an internal service that needs filesystem throughout its lifetime
- You want to make testing easier by injecting mock filesystems
- The component performs many file operations
- Examples: `MetadataProcessor`, `SyncCoordinator`, `CacheManager`

## Current Status

### Crates Using FileSystemAccess
- ✅ **core-metadata**: Uses Pattern 1 for `extract_from_file()`
- ✅ **core-sync**: Uses Pattern 2 for `MetadataProcessor`
- ✅ **bridge-wasm**: Provides `WasmFileSystem` implementation

### Crates That Don't Need It
- **core-library**: Pure data layer, no file I/O
- **core-async**: Provides async primitives only
- **core-auth**: Token storage uses platform storage traits, not filesystem
- **core-runtime**: Config/logging are platform-specific (desktop-shims feature)

### Crates Not Yet WASM-Compatible
- **core-sync**: Blocked by tokio networking (mio)
- **core-playback**: Needs audio backend implementation
- **provider-***: Need HTTP client for WASM

## Design Principles

1. **Prefer Abstraction Over Conditionals**: Use trait objects when possible
2. **Shared Business Logic**: Keep core algorithms platform-agnostic
3. **Platform-Specific I/O**: Accept differences in how data is read/written
4. **Fail Forward**: Don't disable features for WASM - implement them differently
5. **Test Both Paths**: Ensure both native and WASM code paths are tested

## Future Improvements

1. **File Streaming**: Add streaming APIs to `FileSystemAccess` for large files
2. **Progress Callbacks**: Add progress reporting for long file operations
3. **Caching Layer**: Implement intelligent caching in `WasmFileSystem`
4. **Quota Management**: Handle IndexedDB quota limits gracefully
5. **File Watching**: Add file change notification APIs (native only)

## Related Documentation

- `bridge-wasm/FILESYSTEM.md`: Detailed WasmFileSystem implementation
- `docs/core_architecture.md`: High-level architecture principles
- `docs/WASM_COMPATIBILITY_ACHIEVEMENT.md`: Journey to WASM compatibility
- `docs/database_abstraction.md`: Similar pattern for database access
