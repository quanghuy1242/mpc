# GitHub Copilot Instructions for Music Platform Core (Rust)

If you are struggling to implement a feature or complete a task, don't ever simplify the solution or code.
Say explicitly that you are unable to help if you cannot find a solution that meets all the requirements.
Or at least put TODO comments in the code where you are unsure.
Or even terminate the implementation if you cannot find a solution that meets all the requirements.
Don't workaround or simplify the requirements.
This is not a toy project, this is a production-grade cross-platform library that will be used in real world applications.
Don't use any simpler approaches or libraries that do not meet the performance, security, or cross-platform requirements.

## Context7 Usage

Always use context7 when I need code generation, setup or configuration steps, or
library/API documentation. This means you should automatically use the Context7 MCP
tools to resolve library id and get library docs without me having to explicitly ask.

## Update Task List status and high level architecture

Always update the task list status in the markdown file docs\ai_task_list.md when you complete a task.
Always follow the high level architecture and design principles outlined in docs/core_architecture.md
when generating code or implementing features.

## Project Overview
This is a cross-platform music playback core library written in Rust, designed to power desktop, mobile (iOS/Android), and web applications. The core provides unified music library management, cloud storage integration (Google Drive, OneDrive), metadata extraction, playback streaming, and lyrics support.

## Architecture Principles

### Layered Architecture
```
UI / Host App → CoreService → Domain Modules → Host Bridges → Storage/Providers → Cloud APIs
```

**Core Layers:**
1. **Core Application Layer**: `CoreService` façade orchestrating all modules
2. **Domain Modules**: Auth, providers, sync, library, metadata, playback, caching, config
3. **Infrastructure Layer**: Async runtime (Tokio), storage (SQLite), logging (`tracing`), HTTP clients, queues
4. **Host Bridge Layer**: Platform-agnostic traits implemented per platform
5. **Integration Layer**: FFI bridges (Swift/Kotlin/C), WASM bindings, feature flags

### Key Design Patterns

#### 1. **Trait-Based Abstraction**
All platform-specific functionality must use trait abstractions:
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

#### 2. **Fail-Fast with Descriptive Errors**
When required host bridges are missing, panic with actionable messages:
```rust
// Good example
pub fn new(config: CoreConfig) -> Result<Self> {
    let http_client = config.http_client
        .ok_or_else(|| CoreError::CapabilityMissing {
            capability: "HttpClient",
            message: "No HTTP client implementation provided. Desktop: ensure default feature is enabled. Mobile: inject platform-native adapter."
        })?;
    // ...
}
```

#### 3. **Event-Driven Architecture**
Use event bus (`tokio::sync::broadcast`) for state changes:
```rust
pub enum CoreEvent {
    Auth(AuthEvent),      // SignedOut, SigningIn, SignedIn, TokenRefreshing
    Sync(SyncEvent),      // Progress, Completed, Error
    Library(LibraryEvent),
    Playback(PlaybackEvent),
}

// Emit events
self.event_bus.send(CoreEvent::Sync(SyncEvent::Progress { 
    percent: 45, 
    items_processed: 1200 
}))?;
```

#### 4. **Async-First with Tokio**
All I/O operations must be async:
```rust
pub async fn stream_track(&self, track_id: TrackId) -> Result<AudioSource> {
    let token = self.auth.get_valid_token(profile_id).await?;
    let stream = self.provider.download(remote_id, None).await?;
    Ok(AudioSource::Remote(stream))
}
```

#### 5. **Graceful Degradation**
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

## Rust Best Practices

### Code Style & Organization

#### Crate Structure
```
workspace/
├── core-runtime/       # Logging, config, event bus, task scheduler
├── core-auth/          # Authentication & credential management
├── core-sync/          # Sync orchestration & indexing
├── core-library/       # Database & repository layer
├── core-metadata/      # Tag extraction, artwork, lyrics
├── core-playback/      # Streaming & audio decoding
├── provider-google-drive/
├── provider-onedrive/
└── bridge-*/           # Platform adapters (iOS, Android, web, desktop)
```

#### Module Organization
```rust
// In each crate's lib.rs
pub mod error;      // Error types first
pub mod types;      // Public types and traits
pub mod config;     // Configuration structs
mod internal;       // Private implementation
pub mod api;        // Public API surface

// Re-export key items
pub use error::{Error, Result};
pub use types::{TrackId, AlbumId, PlaylistId};
pub use api::CoreService;
```

### Naming Conventions

- **Types**: `PascalCase` - `CoreService`, `TrackRepository`, `SyncJob`
- **Traits**: Descriptive nouns - `StorageProvider`, `AudioDecoder`, `SecureStore`
- **Functions**: `snake_case` - `start_sync`, `query_tracks`, `get_valid_token`
- **Constants**: `SCREAMING_SNAKE_CASE` - `DEFAULT_CACHE_SIZE`, `MAX_RETRY_ATTEMPTS`
- **Modules**: `snake_case` - `sync_coordinator`, `metadata_extractor`

### Error Handling

#### Define Structured Errors
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Provider {provider} authentication failed: {reason}")]
    AuthenticationFailed { provider: String, reason: String },
    
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(#[from] reqwest::Error),
    
    #[error("Secure storage unavailable: {0}")]
    SecureStorageUnavailable(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;
```

#### Error Propagation
```rust
// Use ? operator for propagation
pub async fn sign_in(&self, provider: ProviderKind) -> Result<ProfileId> {
    let auth_url = self.build_auth_url(provider)?;
    let code = self.launch_browser_flow(auth_url).await?;
    let tokens = self.exchange_code(code).await?;
    self.store_tokens(tokens).await?;
    Ok(self.create_profile())
}
```

### Async Patterns

#### Structured Concurrency
```rust
use tokio::try_join;

// Run multiple async operations concurrently
pub async fn enrich_metadata(&self, track: &Track) -> Result<()> {
    let (artwork, lyrics) = try_join!(
        self.fetch_artwork(track),
        self.fetch_lyrics(track),
    )?;
    
    self.store_metadata(track.id, artwork, lyrics).await
}
```

#### Cancellation & Timeout
```rust
use tokio::time::{timeout, Duration};

pub async fn sync_with_timeout(&self, duration: Duration) -> Result<()> {
    timeout(duration, self.perform_sync())
        .await
        .map_err(|_| SyncError::Timeout)?
}
```

### Memory Management

#### Use `Arc` for Shared State
```rust
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct CoreContext {
    pub auth: Arc<AuthManager>,
    pub library: Arc<LibraryRepository>,
    pub cache: Arc<RwLock<Cache>>,
}
```

#### Stream Large Data
```rust
use futures::stream::Stream;

pub async fn query_tracks(&self, filter: TrackFilter) 
    -> Result<impl Stream<Item = Result<Track>>> 
{
    self.repository.stream_tracks(filter).await
}
```

#### Bounded Caches with LRU
```rust
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ArtworkCache {
    cache: LruCache<ArtworkId, Bytes>,
    max_size: usize,
}

impl ArtworkCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(max_entries).unwrap()),
            max_size: max_entries * AVG_ARTWORK_SIZE,
        }
    }
}
```

### Testing Guidelines

#### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use mockall::mock;
    
    mock! {
        pub StorageProvider {}
        
        #[async_trait]
        impl StorageProvider for StorageProvider {
            async fn list_media(&self, cursor: Option<String>) 
                -> Result<(Vec<RemoteFile>, Option<String>)>;
        }
    }
    
    #[tokio::test]
    async fn test_sync_lists_files() {
        let mut mock = MockStorageProvider::new();
        mock.expect_list_media()
            .returning(|_| Ok((vec![/* test data */], None)));
            
        let coordinator = SyncCoordinator::new(Arc::new(mock));
        let result = coordinator.sync().await;
        
        assert!(result.is_ok());
    }
}
```

#### Integration Tests
```rust
// tests/integration_test.rs
use music_core::*;
use sqlx::SqlitePool;

#[tokio::test]
async fn test_full_sync_workflow() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    let core = CoreService::bootstrap(test_config(pool)).await.unwrap();
    
    let profile = core.sign_in(ProviderKind::GoogleDrive).await.unwrap();
    let job = core.start_sync(profile).await.unwrap();
    
    // Wait for completion
    wait_for_sync_completion(&core, job).await;
    
    let tracks = core.query_tracks(TrackFilter::default()).await.unwrap();
    assert!(!tracks.items.is_empty());
}
```

## Feature Flags

Define features in `Cargo.toml` for optional capabilities:
```toml
[features]
default = ["desktop-shims"]
desktop-shims = ["reqwest", "keyring"]
ffi = ["uniffi"]
wasm = ["wasm-bindgen", "web-sys"]
lyrics = ["lrclib"]
artwork-remote = ["musicbrainz-api"]
offline-cache = ["aes-gcm"]
```

Use conditional compilation:
```rust
#[cfg(feature = "lyrics")]
pub mod lyrics {
    pub use lrclib::LyricsProvider;
}

#[cfg(not(feature = "lyrics"))]
pub mod lyrics {
    pub struct LyricsProvider;
    impl LyricsProvider {
        pub fn new() -> Self {
            panic!("Lyrics feature not enabled. Enable with --features lyrics")
        }
    }
}
```

## Database Patterns

### Migrations
```rust
// migrations/001_initial_schema.sql
CREATE TABLE tracks (
    id TEXT PRIMARY KEY,
    provider_file_id TEXT NOT NULL,
    title TEXT NOT NULL,
    album_id TEXT,
    artist_id TEXT,
    duration_ms INTEGER,
    bitrate INTEGER,
    format TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums(id),
    FOREIGN KEY (artist_id) REFERENCES artists(id)
);

CREATE INDEX idx_tracks_album ON tracks(album_id);
CREATE INDEX idx_tracks_artist ON tracks(artist_id);
CREATE INDEX idx_tracks_provider ON tracks(provider_file_id);
```

### Repository Pattern
```rust
use sqlx::{SqlitePool, FromRow};
use async_trait::async_trait;

#[async_trait]
pub trait TrackRepository: Send + Sync {
    async fn find_by_id(&self, id: &TrackId) -> Result<Option<Track>>;
    async fn insert(&self, track: &Track) -> Result<()>;
    async fn query(&self, filter: TrackFilter) -> Result<Vec<Track>>;
}

pub struct SqliteTrackRepository {
    pool: SqlitePool,
}

#[async_trait]
impl TrackRepository for SqliteTrackRepository {
    async fn find_by_id(&self, id: &TrackId) -> Result<Option<Track>> {
        let track = sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(track)
    }
    
    async fn insert(&self, track: &Track) -> Result<()> {
        sqlx::query(
            "INSERT INTO tracks (id, title, album_id, artist_id, duration_ms) 
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&track.id)
        .bind(&track.title)
        .bind(&track.album_id)
        .bind(&track.artist_id)
        .bind(track.duration_ms)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

## Logging & Observability

### Tracing Integration
```rust
use tracing::{info, warn, error, instrument, Span};

#[instrument(skip(self), fields(track_id = %track_id))]
pub async fn download_track(&self, track_id: TrackId) -> Result<Bytes> {
    let span = Span::current();
    span.record("provider", &self.provider_name);
    
    info!("Starting track download");
    
    match self.fetch_from_cache(track_id).await {
        Some(data) => {
            info!(size = data.len(), "Served from cache");
            return Ok(data);
        }
        None => {
            warn!("Cache miss, downloading from provider");
        }
    }
    
    let data = self.provider.download(track_id).await
        .map_err(|e| {
            error!("Download failed: {}", e);
            e
        })?;
    
    self.cache.insert(track_id, data.clone()).await;
    Ok(data)
}
```

### Metrics
```rust
pub struct Metrics {
    sync_duration: histogram::Histogram,
    api_calls: counter::Counter,
    cache_hits: counter::Counter,
}

impl Metrics {
    pub fn record_sync_duration(&self, duration: Duration) {
        self.sync_duration.record(duration.as_secs_f64());
    }
    
    pub fn increment_api_call(&self, provider: &str) {
        self.api_calls.increment(1, &[("provider", provider)]);
    }
}
```

## Security Best Practices

### Token Management
```rust
// NEVER log tokens
#[instrument(skip(token))]
pub async fn refresh_token(&self, token: RefreshToken) -> Result<AccessToken> {
    tracing::info!("Refreshing token for provider");
    // Implementation
}

// Store tokens securely
pub async fn store_token(&self, token: AccessToken) -> Result<()> {
    self.secure_store.set_secret(
        &format!("token_{}", self.provider_id),
        token.as_bytes(),
    ).await
}
```

### Input Validation
```rust
pub fn validate_track_filter(filter: &TrackFilter) -> Result<()> {
    if let Some(limit) = filter.limit {
        if limit > MAX_QUERY_LIMIT {
            return Err(Error::InvalidInput {
                field: "limit",
                message: format!("Limit exceeds maximum {}", MAX_QUERY_LIMIT),
            });
        }
    }
    Ok(())
}
```

## Platform-Specific Commands

### Build Commands

```bash
# Desktop builds
cargo build --release --features desktop-shims

# Mobile FFI generation (iOS/Android)
cargo build --release --target aarch64-apple-ios --features ffi
cargo build --release --target aarch64-linux-android --features ffi

# WASM build
wasm-pack build --target web --features wasm core-wasm/

# Run tests
cargo test --workspace
cargo test --features lyrics,artwork-remote

# Clippy & formatting
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# Generate docs
cargo doc --no-deps --all-features --open
```

### Database Commands

```bash
# Run migrations
sqlx migrate run --database-url sqlite://local.db

# Create new migration
sqlx migrate add <migration_name>

# Check migrations without running
sqlx migrate info --database-url sqlite://local.db
```

### FFI Binding Generation

```bash
# Generate Swift bindings (iOS/macOS)
cargo run --bin uniffi-bindgen generate \
    --library target/release/libcore.dylib \
    --language swift \
    --out-dir bindings/swift

# Generate Kotlin bindings (Android)
cargo run --bin uniffi-bindgen generate \
    --library target/release/libcore.so \
    --language kotlin \
    --out-dir bindings/kotlin
```

## Common Patterns & Utilities

### Retry with Exponential Backoff
```rust
use tokio::time::{sleep, Duration};

pub async fn retry_with_backoff<T, F, Fut>(
    mut operation: F,
    max_attempts: u32,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt >= max_attempts => return Err(e),
            Err(e) => {
                let delay = Duration::from_millis(100 * 2u64.pow(attempt));
                tracing::warn!(
                    attempt = attempt + 1,
                    max_attempts,
                    delay_ms = delay.as_millis(),
                    "Operation failed, retrying: {}",
                    e
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}
```

### Pagination Helper
```rust
#[derive(Debug, Clone)]
pub struct PageRequest {
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub total_pages: u32,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, total: u64, request: PageRequest) -> Self {
        let total_pages = (total as f64 / request.page_size as f64).ceil() as u32;
        Self {
            items,
            total,
            page: request.page,
            total_pages,
        }
    }
}
```

### Deduplication by Hash
```rust
use sha2::{Sha256, Digest};

pub fn compute_content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

pub async fn store_artwork_with_dedup(
    &self,
    data: Bytes,
) -> Result<ArtworkId> {
    let hash = compute_content_hash(&data);
    
    // Check if already exists
    if let Some(id) = self.find_by_hash(&hash).await? {
        return Ok(id);
    }
    
    // Store new
    let id = ArtworkId::new();
    self.insert(id, hash, data).await?;
    Ok(id)
}
```

## Documentation Standards

### Module Documentation
```rust
//! # Sync Coordinator Module
//!
//! Orchestrates full and incremental synchronization with cloud storage providers.
//!
//! ## Overview
//!
//! The sync coordinator manages the lifecycle of sync jobs, including:
//! - Listing remote files via `StorageProvider`
//! - Filtering audio files by MIME type and extension
//! - Extracting metadata from downloaded files
//! - Resolving conflicts (renames, duplicates, deletions)
//! - Persisting library entries to the database
//!
//! ## Usage
//!
//! ```no_run
//! use music_core::sync::SyncCoordinator;
//!
//! let coordinator = SyncCoordinator::new(provider, repository);
//! let job_id = coordinator.start_full_sync(profile_id).await?;
//! ```
```

### Function Documentation
```rust
/// Starts an incremental sync for the given profile.
///
/// # Arguments
///
/// * `profile_id` - The profile identifier to sync
/// * `cursor` - Optional continuation token from previous sync
///
/// # Returns
///
/// Returns the sync job ID on success, or an error if:
/// - The profile is not authenticated
/// - A sync is already in progress for this profile
/// - The provider is unavailable
///
/// # Examples
///
/// ```no_run
/// let job_id = coordinator.start_incremental_sync(profile_id, Some(cursor)).await?;
/// let status = coordinator.get_status(job_id).await?;
/// ```
pub async fn start_incremental_sync(
    &self,
    profile_id: ProfileId,
    cursor: Option<String>,
) -> Result<SyncJobId> {
    // Implementation
}
```

## Performance Considerations

### Benchmark Critical Paths
```rust
// benches/sync_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_metadata_extraction(c: &mut Criterion) {
    let extractor = MetadataExtractor::new();
    let sample = load_test_mp3();
    
    c.bench_function("extract_mp3_metadata", |b| {
        b.iter(|| {
            extractor.extract(black_box(&sample))
        });
    });
}

criterion_group!(benches, benchmark_metadata_extraction);
criterion_main!(benches);
```

### Performance Budgets
- **Core bootstrap**: <1s
- **Track start latency** (cached): <150ms
- **Metadata extraction**: <50ms per track
- **Sync throughput**: >100 tracks/second
- **Memory footprint**: <50MB base, <200MB during active sync
- **CPU during playback**: <10% on reference device

## Project-Specific Conventions

### ID Types
Use newtype pattern for type-safe IDs:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(uuid::Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
    
    pub fn from_string(s: &str) -> Result<Self> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}
```

### Configuration
```rust
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub database_path: PathBuf,
    pub cache_dir: PathBuf,
    pub cache_size_mb: usize,
    pub http_client: Option<Arc<dyn HttpClient>>,
    pub file_system: Option<Arc<dyn FileSystemAccess>>,
    pub secure_store: Arc<dyn SecureStore>,
    pub features: FeatureFlags,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureFlags {
    pub enable_lyrics: bool,
    pub enable_artwork_remote: bool,
    pub enable_offline_cache: bool,
}
```

## Code Review Checklist

Before submitting code, ensure:
- [ ] All public APIs are documented with examples
- [ ] Error types use `thiserror` with descriptive messages
- [ ] Async functions are properly instrumented with `#[instrument]`
- [ ] Tests cover success and error paths
- [ ] No sensitive data (tokens, emails) in logs
- [ ] Platform abstractions used instead of direct platform calls
- [ ] Feature flags properly gate optional dependencies
- [ ] Memory allocations bounded (no unbounded collections)
- [ ] Cancellation is handled for long-running operations
- [ ] Database queries use prepared statements
- [ ] HTTP requests include retry logic and timeouts
- [ ] FFI/WASM boundaries are safe (no panics across boundaries)

## Additional Resources

- **Architecture Docs**: `docs/core_architecture.md`
- **Phase Planning**: `docs/phase_ticket_breakdown.md`
- **Tokio Guide**: https://tokio.rs/tokio/tutorial
- **Async Book**: https://rust-lang.github.io/async-book/
- **API Guidelines**: https://rust-lang.github.io/api-guidelines/

## Questions & Support

When encountering issues:
1. Check trait implementations for required host bridges
2. Verify feature flags are correctly enabled
3. Inspect tracing logs for detailed execution flow
4. Review capability matrix for platform-specific constraints
5. Consult phase ticket breakdown for implementation status

---

**Remember**: This is a cross-platform library. Always consider mobile constraints (memory, battery, background execution) and web limitations (CORS, storage quotas, no persistent background tasks) when implementing features.
