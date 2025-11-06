# core-async

Runtime-agnostic async abstraction layer for Music Platform Core.

## Overview

`core-async` provides a unified async API that works across different runtime environments:
- **Native platforms** (Windows, macOS, Linux): Uses Tokio runtime
- **WebAssembly**: Uses browser's event loop with wasm-bindgen-futures

## Purpose

This crate decouples the core library from direct Tokio dependencies, enabling the codebase to compile and run on WebAssembly while maintaining full Tokio functionality on native platforms.

## Architecture

The crate uses conditional compilation (`#[cfg(target_arch = "wasm32")]`) to provide platform-specific implementations:

```rust
// On native: Uses tokio
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::task::spawn;

// On WASM: Uses wasm-bindgen-futures
#[cfg(target_arch = "wasm32")]
pub fn spawn<F>(future: F) where F: Future<Output = ()> + 'static {
    wasm_bindgen_futures::spawn_local(future);
}
```

## Modules

### `task`
Task spawning and execution:
- `spawn` - Spawn a concurrent task
- `spawn_blocking` - Spawn blocking work (native only)
- `yield_now` - Yield to the executor

### `time`
Time-related operations:
- `sleep` - Async sleep
- `timeout` - Timeout for futures
- `interval` - Periodic ticker
- `Instant` - Monotonic time measurement
- `Duration` - Time duration

### `sync`
Synchronization primitives:
- `Mutex` - Async mutual exclusion lock
- `RwLock` - Async reader-writer lock
- `mpsc` - Multi-producer, single-consumer channel
- `oneshot` - One-shot channel
- `broadcast` - Broadcast channel (native only)
- `watch` - Watch channel (native only)
- `Notify` - Task notification

## Usage

Add `core-async` to your dependencies in `Cargo.toml`:

```toml
[dependencies]
core-async = { path = "../core-async" }
```

Then use it in your code:

```rust
use core_async::{task, time, sync};

async fn example() {
    // Spawn a task
    let handle = task::spawn(async {
        time::sleep(time::Duration::from_secs(1)).await;
        42
    });
    
    // Use synchronization primitives
    let mutex = sync::Mutex::new(vec![]);
    let mut guard = mutex.lock().await;
    guard.push(1);
}
```

## Platform Differences

### Native (Tokio)
- Full multi-threading support
- `spawn` returns `JoinHandle<T>` for awaiting results
- `spawn_blocking` available for CPU-intensive work
- All sync primitives are `Send + Sync`
- High-precision timing

### WASM
- Single-threaded execution
- `spawn` returns `()` (fire-and-forget)
- `spawn_blocking` panics (not supported)
- Sync primitives are NOT `Send` (single-threaded)
- Timing uses browser APIs (`performance.now()`, `setTimeout`)

## Testing

Run tests on native:
```bash
cargo test --package core-async
```

Run tests on WASM (requires `wasm-pack`):
```bash
wasm-pack test --headless --firefox core-async
```

## Migration Guide

To migrate existing code from direct Tokio usage:

### Before:
```rust
use tokio::task::spawn;
use tokio::time::sleep;
use tokio::sync::Mutex;
```

### After:
```rust
use core_async::task::spawn;
use core_async::time::sleep;
use core_async::sync::Mutex;
```

## Feature Compatibility Matrix

| Feature | Native | WASM | Notes |
|---------|--------|------|-------|
| `task::spawn` | ✅ | ✅ | WASM returns `()` |
| `task::spawn_blocking` | ✅ | ❌ | Panics on WASM |
| `task::yield_now` | ✅ | ✅ | |
| `time::sleep` | ✅ | ✅ | |
| `time::timeout` | ✅ | ✅ | |
| `time::interval` | ✅ | ✅ | |
| `time::Instant` | ✅ | ✅ | |
| `sync::Mutex` | ✅ | ✅ | |
| `sync::RwLock` | ✅ | ✅ | |
| `sync::mpsc` | ✅ | ✅ | |
| `sync::oneshot` | ✅ | ✅ | |
| `sync::broadcast` | ✅ | ❌ | Panics on WASM |
| `sync::watch` | ✅ | ❌ | Panics on WASM |
| `sync::Notify` | ✅ | ⚠️ | Limited on WASM |
| `sync::Barrier` | ✅ | ❌ | Panics on WASM |
| `sync::Semaphore` | ✅ | ❌ | Panics on WASM |

## License

MIT OR Apache-2.0
