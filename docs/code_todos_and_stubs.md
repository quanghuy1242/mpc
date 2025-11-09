# Code TODOs, Stubs, and Incomplete Implementations

This document catalogs all TODO comments, stub implementations, simplified code, and areas requiring completion across the codebase.

**Last Updated:** 2025-11-09

---

## 🚨 Critical TODOs (High Priority)

### core-sync

#### 1. **Incremental Sync Logic Missing** 🔴
- **File:** `core-sync/src/coordinator.rs:959`
- **Issue:** Incremental sync is not properly implemented
- **Reference:** See `docs/immediate_todo.md` for detailed fix steps
```rust
// TODO: Consider querying database for all known provider file IDs
// to detect deletions more reliably
```

#### 2. **Playlist Reference Updates** 🟡
- **File:** `core-sync/src/conflict_resolver.rs:691`
```rust
// TODO: Update playlist references to point to primary track
```

#### 3. **Metadata Processor - Disabled Test** 🟡
- **File:** `core-sync/tests/incremental_sync_tests.rs:337`
```rust
// Note: This test would require setting up authentication first
```

---

### core-playback

#### 4. **Streaming Tests Missing** 🔴
- **File:** `core-playback/tests/streaming_tests.rs:1`
```rust
// TODO: Comprehensive streaming tests will be added after HTTP client integration is complete
```
- **Action Required:** Implement full streaming tests once HttpClient is integrated

#### 5. **Cache Tests Disabled** 🟡
- **File:** `core-playback/tests/cache_tests.rs:27`
```rust
// TODO: Re-enable tests once mock implementations are available in bridge-traits
```

#### 6. **WASM Advanced Bindings** 🟡
- **File:** `core-playback/src/wasm.rs:489`
```rust
// TODO: Advanced Bindings
// - JsPlaybackEngine wrapper
// - Event listeners for playback state changes
// - Buffer monitoring APIs
```

#### 7. **Symphonia Metadata Extraction** 🟡
- **File:** `core-playback/src/decoder/symphonia.rs:121`
```rust
let _metadata = probe_result.metadata; // TODO: Extract tags when API is clarified
```

- **File:** `core-playback/src/decoder/symphonia.rs:189`
```rust
let tags = HashMap::new(); // TODO: Extract from format_reader.metadata() properly
```

#### 8. **HTTP Streaming Support** 🔴
- **File:** `core-playback/src/decoder/symphonia.rs:303`
```rust
// TODO: This needs HttpClient passed through context or AudioSource
// Currently we panic if trying to decode from HTTP URL
```
- **Action Required:** Pass HttpClient through AudioSource or context for HTTP streaming

---

### core-metadata

#### 9. **Genius Lyrics API** 🟡
- **File:** `core-metadata/src/lyrics.rs:742`
```rust
// TODO: Consider using official Genius lyrics API when available
// Current implementation is a stub that returns NotFound
```
- **Note:** Current GeniusProvider is a stub due to API limitations

#### 10. **Remote Artwork Providers (Stubs)** 🟡
- **Files:** 
  - `core-metadata/src/providers/musicbrainz_coverart.rs` (stub)
  - `core-metadata/src/providers/lastfm.rs` (stub)
- **Reference:** Task 402 in `docs/ai_task_list.md:1596`
- **Action Required:** Replace stub implementations with real API integration

---

### core-library

#### 11. **WASM Database Pagination** 🟡
- **File:** `core-library/src/wasm.rs:568`
```rust
// Get first 1000 artists (TODO: add pagination parameters to JS API)
let page_request = PageRequest::new(0, 1000);
```
- **Action Required:** Expose pagination parameters to JavaScript API

#### 12. **Database Mutation through Arc** 🟡
- **File:** `core-library/src/wasm.rs:520`
```rust
// NOTE: Mutations need to be handled through Arc properly or use interior mutability
// For now, initialization should be done by the JavaScript bridge
```

---

### core-runtime

#### 13. **Keyring Test Disabled** 🟡
- **File:** `core-runtime/src/config.rs:1068-1069`
```rust
// TODO(#TASK-005): Re-enable once desktop keyring support is available in CI
#[ignore = "TODO: Enable once desktop environment is available"]
#[core_async::test]
async fn test_keyring_integration() { ... }
```

---

### core-auth

#### 14. **Documentation Examples Use `todo!()`** ℹ️
- **Files:** Multiple files in `core-auth/src/`
- **Pattern:** Doc examples use `let http_client: Arc<dyn HttpClient> = todo!();`
- **Files affected:**
  - `oauth.rs:30, 178, 219, 293, 424`
  - `manager.rs:39, 157, 209, 282, 382, 531, 614, 762`
- **Status:** Not an issue - these are documentation examples

#### 15. **Placeholder Client IDs in Tests** ℹ️
- **File:** `core-auth/src/manager.rs:792, 806`
```rust
.unwrap_or_else(|_| "placeholder_client_id".to_string())
```
- **Status:** Acceptable for tests

---

## 🔧 Stub Implementations

### 1. **Core-Auth Test Stub**
- **File:** `core-auth/src/oauth.rs:564-567`
```rust
struct StubHttpClient;

impl HttpClient for StubHttpClient {
    // Minimal implementation for tests
}
```
- **Purpose:** Test-only stub, acceptable

### 2. **Encryption Stubs (Feature-Gated)**
- **File:** `core-playback/src/cache/encryption.rs`
- **Functions:**
  - `generate_key()` - Line 30: Returns dummy key when encryption disabled
  - `encrypt_data()` - Line 119: Returns unencrypted data as-is
  - `decrypt_data()` - Line 157: Returns data as-is
- **Status:** Intentional feature-gated stubs, acceptable

### 3. **Core-Async WASM Stubs**
- **File:** `core-async/src/wasm/fs.rs:440-500`
- **Functions:**
  - `create_dir()` - Single directory (unsupported on WASM)
  - `remove_dir()` - Single directory removal
  - `copy()` - File copy
  - `rename()` - File rename
  - `hard_link()` - Hard links
  - `read_link()` - Symlink reading
  - `symlink_metadata()` - Same as metadata
  - `set_permissions()` - Permissions
  - `DirBuilder` - Directory builder
- **Status:** Intentional WASM limitations, documented

### 4. **Bridge-WASM Filesystem Example Disabled**
- **File:** `bridge-wasm/examples/filesystem_demo.rs:6`
```rust
//! TODO: This example is temporarily commented out until Task 5 in docs/immediate_todo.md
//! (Fix WASM Trait Compatibility Issues) is completed.
```
- **Action Required:** Re-enable once Task 5 is complete

---

## 📝 Simplified/Demo Code

### 1. **Desktop Background Task Manager (Simplified)**
- **File:** `bridge-desktop/README.md:169`
- **Note:** "This is a simplified implementation. A production version would execute user-defined task functions and persist task state across restarts."

### 2. **SQLite Batch Execute (Simplified)**
- **File:** `core-library/src/adapters/sqlite_native.rs:369`
```rust
// NOTE: This simplified implementation executes statements sequentially
// A production implementation might use transactions more efficiently
```

### 3. **Core-Async WASM Interval (Simplified)**
- **File:** `core-async/src/time.rs:241`
```rust
/// This is a simplified version for WASM that doesn't support all features
/// of tokio::time::Interval
```

### 4. **WASM File Handle (Simplified)**
- **File:** `core-async/src/wasm/fs.rs:300-365`
```rust
// File Handle (Simplified)
/// Note: On WASM, this is a simplified implementation that doesn't support
/// all file operations
```

### 5. **WASM OpenOptions (Simplified)**
- **File:** `core-async/src/wasm/fs.rs:365`
```rust
/// Note: On WASM, this is a simplified implementation. Many options are ignored
/// because the underlying File System Access API has limited configuration
```

### 6. **Playback Examples (Demo Code)**
- **File:** `core-playback/examples/playback_demo.rs`
```rust
// Simple In-Memory Audio Decoder (for demonstration)
// Simple Console Playback Adapter (for demonstration)
```
- **Status:** Intentional demo code, acceptable

### 7. **Logging Demo Example**
- **File:** `core-runtime/examples/logging_demo.rs`
- **Status:** Demo/example code, acceptable

---

## 🔍 Notable Comments & Limitations

### Core-Playback

#### Symphonia Decoder Limitations
- **File:** `core-playback/src/decoder/symphonia.rs:156`
```rust
// Note: Bitrate may not be available from codec params, it's calculated during decode
```

- **File:** `core-playback/src/decoder/symphonia.rs:187`
```rust
// Note: Metadata extraction simplified - advanced metadata handling
// would require proper tag parsing from Symphonia's metadata API
```

#### Buffer Ownership Note
- **File:** `core-playback/src/decoder/symphonia.rs:467`
```rust
// Note: We convert the decoded buffer to owned data immediately because
// Symphonia's AudioBuffer lifetime is tied to the format reader
```

### Core-Async

#### WASM Task Limitations
- **File:** `core-async/src/wasm/task.rs:98`
```rust
/// Note: On WASM, we cannot actually abort a running task since it's
/// on a single-threaded event loop
```

- **File:** `core-async/src/wasm/task.rs:108`
```rust
/// Note: On WASM, we cannot check if a task is finished without consuming
/// the future
```

- **File:** `core-async/src/wasm/task.rs:177`
```rust
// Note: Panic handling in WASM is limited - panics will propagate to console
```

#### WASM block_on Limitation
- **File:** `core-async/tests/wasm_tests.rs:279`
```rust
// NOTE: block_on on WASM only works for immediate futures that don't depend
// on the event loop. For real async operations, use spawn_local.
```

### Core-Library

#### In-Memory Database WAL Mode
- **File:** `core-library/src/db.rs:437`
```rust
// Note: In-memory databases use "memory" mode instead of WAL
```

#### Transaction Rollback Note
- **File:** `core-library/src/adapters/sqlite_native.rs:562`
```rust
// NOTE: Due to connection pooling, the rollback might not affect the same connection.
// In production, proper transaction handling would use BEGIN/COMMIT/ROLLBACK
```

### Core-Metadata

#### Artist Enrichment Test Note
- **File:** `core-metadata/tests/artist_enrichment_tests.rs:64`
```rust
// Note: NOT calling .with_artist_enrichment()
// Testing base service without artist provider
```

#### Enrichment Service Note
- **File:** `core-metadata/src/enrichment_service.rs:226`
```rust
// Note: updated_at is managed by the database layer
```

#### MusicBrainz ID Note
- **File:** `core-metadata/src/enrichment_service.rs:291`
```rust
// Note: mbid is not currently stored in our models, pass None
```

#### Stub Method Documentation
- **File:** `core-metadata/src/enrichment_service.rs:307`
```rust
/// Fetch and store artwork for a track (stub when artwork-remote feature is disabled)
```

### Core-Sync

#### Rename Detection Note
- **File:** `core-sync/src/conflict_resolution_orchestrator.rs:277`
```rust
// Note: Full rename detection requires comparing hashes from the provider
```

### Core-Runtime

#### WASM LoggerSink Limitation
- **File:** `core-runtime/src/logging.rs:237`
```rust
// Note: LoggerSink integration on WASM is limited by tracing-subscriber's
// single global subscriber limitation
```

#### PII Redaction Placeholder
- **File:** `core-runtime/src/logging.rs:404`
```rust
// This is a placeholder for more advanced redaction if needed.
```

### Bridge Traits

#### Documentation Examples
- **File:** `bridge-traits/src/lib.rs:97, 102`
```rust
//!         todo!()
```
- **Status:** Documentation examples, acceptable

### Bridge-Desktop

#### Secure Store Limitations
- **File:** `bridge-desktop/src/secure_store.rs:118, 125`
```rust
// Note: Keyring doesn't provide a way to list all keys
// Note: Keyring doesn't provide a way to enumerate and delete all entries
```

- **File:** `bridge-desktop/src/secure_store.rs:164`
```rust
// Note: This test might fail if keyring is not available (e.g., headless systems, CI)
```

#### Network Monitor Note
- **File:** `bridge-desktop/src/network.rs:18`
```rust
/// Note: Platform-specific implementations (Linux netlink, macOS SystemConfiguration,
/// Windows Network List Manager) would provide more accurate detection
```

---

## 📊 Summary Statistics

### By Priority
- 🔴 **Critical (Blocking):** 3 items
- 🟡 **High (Important):** 13 items
- ℹ️ **Low (Documentation/Examples):** 15 items

### By Category
- **Missing Implementations:** 8 items
- **Stub Functions:** 4 groups
- **Simplified Code:** 7 areas
- **Feature Limitations:** 15+ documented limitations
- **Disabled Tests:** 3 tests

### By Module
- `core-sync`: 3 TODOs
- `core-playback`: 8 TODOs
- `core-metadata`: 2 TODOs
- `core-library`: 2 TODOs
- `core-runtime`: 1 TODO
- `core-auth`: Documentation examples (non-blocking)
- `core-async`: Intentional WASM stubs
- `bridge-*`: Documented limitations

---

## 🎯 Recommended Action Plan

### Phase 1: Critical Blockers (Must Fix)
1. Implement incremental sync logic (core-sync)
2. Add HTTP streaming support to playback decoder
3. Implement comprehensive streaming tests

### Phase 2: Important Features (Should Fix)
1. Add remote artwork provider implementations (MusicBrainz, Last.fm)
2. Re-enable cache tests with proper mocks
3. Add WASM pagination to library API
4. Add advanced WASM playback bindings
5. Implement proper Symphonia metadata extraction

### Phase 3: Polish & Optimization (Nice to Have)
1. Add Genius lyrics API support (if available)
2. Improve batch execute in SQLite adapter
3. Add playlist reference updates to conflict resolver
4. Re-enable keyring test for CI

### Phase 4: Documentation & Examples
1. Update documentation examples to not use `todo!()`
2. Ensure all demo code is clearly marked
3. Document all intentional limitations

---

## 📖 References

- **Critical Tasks:** See `docs/immediate_todo.md`
- **Full Task List:** See `docs/ai_task_list.md`
- **Architecture:** See `docs/core_architecture.md`

---

*This document should be updated whenever TODOs are added or resolved.*
