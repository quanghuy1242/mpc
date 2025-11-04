# Desktop Bridge Implementations

Production-ready implementations of all bridge traits for desktop platforms (macOS, Windows, Linux).

## Overview

This crate provides desktop-specific implementations of the platform abstraction traits defined in `bridge-traits`. These implementations use mature, well-tested libraries that are appropriate for desktop environments.

## Implementations

### HttpClient - `ReqwestHttpClient`

HTTP client implementation using the `reqwest` library.

**Features:**
- Connection pooling and keep-alive
- Automatic retry with exponential backoff
- TLS support (rustls by default)
- Streaming downloads
- Configurable timeouts

**Usage:**
```rust
use bridge_desktop::ReqwestHttpClient;
use bridge_traits::http::{HttpClient, HttpRequest, HttpMethod};

let client = ReqwestHttpClient::new();

let request = HttpRequest::new(HttpMethod::Get, "https://api.example.com/data")
    .bearer_token("my-token");

let response = client.execute(request).await?;
```

### FileSystemAccess - `TokioFileSystem`

Async file system operations using Tokio.

**Features:**
- Async file I/O with `tokio::fs`
- Platform-specific app directories (cache and data)
- Streaming reads and writes
- Directory size calculations
- Proper error mapping

**Usage:**
```rust
use bridge_desktop::TokioFileSystem;
use bridge_traits::storage::FileSystemAccess;
use bytes::Bytes;

let fs = TokioFileSystem::new();

// Write file
let data = Bytes::from("Hello, World!");
fs.write_file(&path, data).await?;

// Read file
let contents = fs.read_file(&path).await?;
```

### SecureStore - `KeyringSecureStore`

Secure credential storage using OS-specific keychains.

**Platform Support:**
- **macOS**: Keychain
- **Windows**: Credential Manager (DPAPI)
- **Linux**: Secret Service (libsecret)

**Features:**
- Hardware-backed encryption where available
- Base64 encoding for binary data
- Secure credential deletion

**Usage:**
```rust
use bridge_desktop::KeyringSecureStore;
use bridge_traits::storage::SecureStore;

let store = KeyringSecureStore::new();

// Store secret
store.set_secret("oauth_token", token.as_bytes()).await?;

// Retrieve secret
let token = store.get_secret("oauth_token").await?;

// Delete secret
store.delete_secret("oauth_token").await?;
```

**Note**: Requires the `secure-store` feature (enabled by default).

### SettingsStore - `SqliteSettingsStore`

Persistent key-value settings storage using SQLite.

**Features:**
- Type-safe value storage (string, bool, i64, f64)
- Transactional updates
- In-memory mode for testing
- Async operations with sqlx

**Usage:**
```rust
use bridge_desktop::SqliteSettingsStore;
use bridge_traits::storage::SettingsStore;

let store = SqliteSettingsStore::new(db_path).await?;

// Store settings
store.set_string("theme", "dark").await?;
store.set_bool("sync_on_wifi_only", true).await?;

// Retrieve settings
let theme = store.get_string("theme").await?;
let wifi_only = store.get_bool("sync_on_wifi_only").await?;
```

### NetworkMonitor - `DesktopNetworkMonitor`

Basic network connectivity monitoring.

**Features:**
- Connection status detection
- Network change subscriptions
- Simple polling-based implementation

**Usage:**
```rust
use bridge_desktop::DesktopNetworkMonitor;
use bridge_traits::network::NetworkMonitor;

let monitor = DesktopNetworkMonitor::new();

let info = monitor.get_network_info().await?;
if info.status == NetworkStatus::Connected {
    // Proceed with network operations
}
```

**Note**: This is a basic implementation. Platform-specific implementations using native APIs (Linux netlink, macOS SystemConfiguration, Windows Network List Manager) would be more robust but require additional dependencies.

### BackgroundExecutor - `TokioBackgroundExecutor`

Task scheduling using the Tokio runtime.

**Features:**
- In-memory task tracking
- Simple scheduling interface
- No resource constraints (desktop always has resources)

**Usage:**
```rust
use bridge_desktop::TokioBackgroundExecutor;
use bridge_traits::background::{BackgroundExecutor, TaskConstraints};
use std::time::Duration;

let executor = TokioBackgroundExecutor::new();

let task_id = executor.schedule_task(
    "sync_job",
    Duration::from_secs(3600),
    TaskConstraints::default(),
).await?;
```

**Note**: This is a simplified implementation. A production version would execute user-defined task functions and persist task state across restarts.

### LifecycleObserver - `DesktopLifecycleObserver`

Lifecycle observer for desktop (no-op implementation).

**Behavior:**
- Always returns `LifecycleState::Foreground`
- Change stream never emits (desktop apps don't background)

**Usage:**
```rust
use bridge_desktop::DesktopLifecycleObserver;
use bridge_traits::background::LifecycleObserver;

let observer = DesktopLifecycleObserver::new();
let state = observer.get_state().await?; // Always Foreground
```

## Feature Flags

- `default`: Enables `secure-store`
- `secure-store`: Enables OS keychain integration via `keyring` crate

## Dependencies

### Core
- `tokio`: Async runtime and file I/O
- `async-trait`: Async trait support
- `reqwest`: HTTP client
- `sqlx`: Database operations (SQLite)

### Platform-Specific
- `keyring`: OS keychain access (optional, default)
- `dirs`: Platform-specific directory paths

### Utilities
- `bytes`: Efficient byte handling
- `base64`: Binary data encoding
- `futures-util`: Stream utilities
- `tokio-util`: Tokio stream utilities

## Testing

Run all tests:
```bash
cargo test --package bridge-desktop
```

Run tests without keyring (for CI environments):
```bash
cargo test --package bridge-desktop --no-default-features
```

Run clippy:
```bash
cargo clippy --package bridge-desktop -- -D warnings
```

## Platform Notes

### Linux
- Keyring requires `libsecret` (GNOME Keyring or KWallet)
- Install: `sudo apt-get install libsecret-1-dev` (Ubuntu/Debian)

### macOS
- Keyring uses native Keychain
- No additional dependencies required

### Windows
- Keyring uses Credential Manager
- No additional dependencies required

## Examples

See the integration tests in each module for usage examples.

## License

MIT OR Apache-2.0
