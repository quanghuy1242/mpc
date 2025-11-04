# Architecture Patterns and Design Principles

## Key Design Patterns

### 1. Trait-Based Abstraction
All platform-specific functionality uses trait abstractions defined in `bridge-traits`:
- `HttpClient` - HTTP operations with OAuth, retry, TLS
- `FileSystemAccess` - File I/O, caching, offline storage
- `SecureStore` - Credential persistence (Keychain/Keystore)
- `SettingsStore` - Key-value preferences storage
- `NetworkMonitor` - Connectivity and metered network detection
- `BackgroundExecutor` - Task scheduling respecting platform constraints
- `LifecycleObserver` - App foreground/background transitions
- `AudioDecoder` - Pluggable audio decoding backends
- `PlaybackAdapter` - Audio playback engine integration
- `StorageProvider` - Cloud storage provider abstraction

### 2. Repository Pattern
Data access layer abstractions:
- Define repository traits (`TrackRepository`, `AlbumRepository`, etc.)
- Implement with concrete types (`SqliteTrackRepository`)
- Use for all database operations
- Mock for testing

### 3. Event-Driven Architecture
Use event bus (`tokio::sync::broadcast`) for state changes:
```rust
pub enum CoreEvent {
    Auth(AuthEvent),      // SignedOut, SigningIn, SignedIn, TokenRefreshing
    Sync(SyncEvent),      // Progress, Completed, Error
    Library(LibraryEvent),
    Playback(PlaybackEvent),
}
```

### 4. Façade Pattern
`CoreService` provides unified API surface:
- Single entry point for all operations
- Coordinates between modules
- Simplifies client integration

### 5. Builder Pattern
Used for complex configuration:
- `CoreConfig` builder with validation
- `LoggingConfig` builder for flexible setup
- Request builders for HTTP operations

### 6. Newtype Pattern
Type-safe IDs using newtype wrappers:
```rust
pub struct TrackId(uuid::Uuid);
pub struct AlbumId(uuid::Uuid);
pub struct ProfileId(uuid::Uuid);
```

## Error Handling Strategy

### Fail-Fast with Descriptive Errors
When required host bridges are missing, panic with actionable messages:
```rust
let http_client = config.http_client
    .ok_or_else(|| CoreError::CapabilityMissing {
        capability: "HttpClient",
        message: "No HTTP client implementation provided. Desktop: ensure default feature is enabled. Mobile: inject platform-native adapter."
    })?;
```

### Structured Error Types
Each module defines its own error enum with `thiserror`:
- `AuthError` - Authentication failures
- `SyncError` - Sync operation failures
- `PlaybackError` - Playback failures
- `BridgeError` - Platform bridge failures

### Error Propagation
- Use `?` operator for clean propagation
- Wrap errors with context using `anyhow` in applications
- Convert between error types explicitly

## Graceful Degradation

Optional features should degrade gracefully, not crash:
```rust
// Artwork fetch failure should not block sync
match self.artwork_fetcher.fetch_artwork(track).await {
    Ok(artwork) => track.artwork_id = Some(artwork.id),
    Err(e) => {
        tracing::warn!("Artwork fetch failed: {}", e);
        self.metrics.increment_artwork_failure();
        // Continue without artwork
    }
}
```

## Async-First Design

### All I/O is Async
```rust
pub async fn stream_track(&self, track_id: TrackId) -> Result<AudioSource> {
    let token = self.auth.get_valid_token(profile_id).await?;
    let stream = self.provider.download(remote_id, None).await?;
    Ok(AudioSource::Remote(stream))
}
```

### Structured Concurrency
```rust
use tokio::try_join;

let (artwork, lyrics) = try_join!(
    self.fetch_artwork(track),
    self.fetch_lyrics(track),
)?;
```

### Cancellation Support
- Long-running operations support cancellation
- Use `tokio::select!` for cancellable operations
- Store cancellation tokens for jobs

## Modularity & Separation of Concerns

### Crate Organization
- Each domain has its own crate (`core-auth`, `core-sync`, etc.)
- Bridge traits separate from implementations
- Platform-specific code isolated in bridge crates

### Feature Flags for Optional Functionality
- `desktop-shims` - Desktop implementations
- `ffi` - Mobile bindings
- `wasm` - Web bindings
- `lyrics` - Lyrics fetching
- `artwork-remote` - Remote artwork
- `offline-cache` - Encrypted caching

## Platform Capability Matrix

| Capability | Desktop | Android | iOS | Web |
|------------|---------|---------|-----|-----|
| Background sync | ✅ Full | ⚠️ WorkManager | ⚠️ BGTask | ❌ No |
| Offline cache | ✅ Full | ✅ Full | ⚠️ Limited | ⚠️ Quota |
| Secure storage | ✅ Keychain | ✅ Keystore | ✅ Keychain | ⚠️ WebCrypto |
| Push triggers | ✅ Full | ⚠️ Constrained | ⚠️ Limited | ⚠️ Limited |

Legend: ✅ full support, ⚠️ supported with constraints, ❌ unsupported

## Security Principles

### Token Management
- Never log tokens or credentials
- Use `SecureStore` trait for persistence
- Automatic token refresh before expiration
- Secure erasure on sign-out

### Input Validation
- Validate all user inputs
- Check limits and bounds
- Return descriptive errors

### Privacy
- Redact PII in logs (emails, tokens, paths)
- Minimize data collection
- Support user data export/deletion

## Performance Considerations

### Performance Budgets
- Core bootstrap: <1s
- Track start latency (cached): <150ms
- Metadata extraction: <50ms per track
- Sync throughput: >100 tracks/second
- Memory footprint: <50MB base, <200MB during sync
- CPU during playback: <10%

### Optimization Strategies
- Use `Arc` for shared state
- Stream large datasets
- Implement bounded caches (LRU)
- Lazy initialization of heavy components
- Connection pooling for database

### Memory Management
- Don't preload large result sets
- Use pagination for queries
- Stream file downloads
- Reuse buffers where possible
- Use `Bytes` for zero-copy operations

## Testing Strategy

### Test Pyramid
1. **Unit Tests** - Test individual functions and modules
2. **Integration Tests** - Test module interactions
3. **Platform Tests** - Verify bridge implementations
4. **End-to-End Tests** - Full workflow tests

### Mocking Strategy
- Use `mockall` for trait mocking
- Create mock implementations of bridges
- Use in-memory SQLite for tests
- Mock HTTP responses

### Test Organization
- Unit tests in `#[cfg(test)]` modules
- Integration tests in `tests/` directory
- Example code in `examples/` directory
