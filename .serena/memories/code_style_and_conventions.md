# Code Style and Conventions

## Naming Conventions

### Types
- **PascalCase**: `CoreService`, `TrackRepository`, `SyncJob`, `HttpClient`
- **Traits**: Descriptive nouns - `StorageProvider`, `AudioDecoder`, `SecureStore`

### Functions & Methods
- **snake_case**: `start_sync`, `query_tracks`, `get_valid_token`
- Async functions always use `async fn`
- Prefer `Result<T>` return type over panicking

### Constants
- **SCREAMING_SNAKE_CASE**: `DEFAULT_CACHE_SIZE`, `MAX_RETRY_ATTEMPTS`

### Modules
- **snake_case**: `sync_coordinator`, `metadata_extractor`

## Module Organization

Each crate follows this structure:
```rust
// lib.rs
pub mod error;      // Error types first
pub mod types;      // Public types and traits
pub mod config;     // Configuration structs
mod internal;       // Private implementation
pub mod api;        // Public API surface

// Re-export key items
pub use error::{Error, Result};
pub use types::{TrackId, AlbumId};
pub use api::CoreService;
```

## Error Handling

### Use `thiserror` for Error Types
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Provider {provider} authentication failed: {reason}")]
    AuthenticationFailed { provider: String, reason: String },
    
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, AuthError>;
```

### Error Propagation
- Use `?` operator for propagation
- Always provide actionable error messages
- Include context in error types

### Fail-Fast Strategy
- Panic with descriptive messages when required bridges are missing
- Include remediation steps in panic messages

## Async Patterns

### Async-First
- All I/O operations must be async
- Use Tokio runtime throughout
- Use `#[async_trait]` macro for trait methods

### Structured Concurrency
```rust
use tokio::try_join;

// Run operations concurrently
let (artwork, lyrics) = try_join!(
    self.fetch_artwork(track),
    self.fetch_lyrics(track),
)?;
```

### Cancellation & Timeout
```rust
use tokio::time::{timeout, Duration};

timeout(duration, self.perform_sync())
    .await
    .map_err(|_| SyncError::Timeout)?
```

## Memory Management

### Shared State with Arc
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

### Stream Large Data
- Don't load entire result sets into memory
- Use `Stream` trait for large queries
- Implement pagination

### Use Newtype Pattern for IDs
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(uuid::Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}
```

## Documentation Standards

### Module Documentation
- Use `//!` for module-level docs
- Include overview, usage examples, and key concepts

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
/// - A sync is already in progress
///
/// # Examples
///
/// ```no_run
/// let job_id = coordinator.start_incremental_sync(profile_id, Some(cursor)).await?;
/// ```
pub async fn start_incremental_sync(
    &self,
    profile_id: ProfileId,
    cursor: Option<String>,
) -> Result<SyncJobId>
```

## Logging & Tracing

### Use `tracing` for Structured Logging
```rust
use tracing::{info, warn, error, instrument, Span};

#[instrument(skip(self), fields(track_id = %track_id))]
pub async fn download_track(&self, track_id: TrackId) -> Result<Bytes> {
    info!("Starting track download");
    // ... implementation
}
```

### PII Protection
- Never log tokens, passwords, or sensitive credentials
- Redact email addresses (first char + ***@[REDACTED])
- Strip file paths to basename only in logs

## Testing

### Unit Tests
- Use `#[cfg(test)]` modules
- Use `mockall` for mocking traits
- Test both success and error paths

### Integration Tests
- Place in `tests/` directory
- Use in-memory SQLite for database tests
- Mock HTTP responses with test fixtures

## Trait Design

### All Platform Bridges Use Traits
- Define in `bridge-traits` crate
- Require `Send + Sync` bounds
- Use `async-trait` for async methods
- Provide mock implementations for testing

### Event-Driven Architecture
```rust
pub enum CoreEvent {
    Auth(AuthEvent),
    Sync(SyncEvent),
    Library(LibraryEvent),
    Playback(PlaybackEvent),
}
```

## Code Quality

### Before Committing
- Run `cargo fmt` - Format code
- Run `cargo clippy -- -D warnings` - No clippy warnings allowed
- Run `cargo test --workspace` - All tests must pass
- Run `cargo doc` - Documentation must build
