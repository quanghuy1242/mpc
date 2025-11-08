# Music Platform Core

A cross-platform music playback core library written in Rust, designed to power desktop, mobile (iOS/Android), and web applications.

## Workspace Structure

This is a multi-crate workspace with the following modules:

### Core Modules
- **core-async** - Runtime-agnostic async primitives (task, sync, fs, time)
- **core-async-macros** - Proc macros for async runtime abstraction
- **core-runtime** - Logging, config, event bus, task scheduler
- **core-auth** - Authentication & credential management
- **core-sync** - Sync orchestration & indexing
- **core-library** - Database & repository layer
- **core-metadata** - Tag extraction, artwork, lyrics
- **core-playback** - Streaming & audio decoding
- **core-service** - Main façade API

### Storage Provider Connectors
- **provider-google-drive** - Google Drive connector
- **provider-onedrive** - OneDrive connector

### Platform Bridge Layer
- **bridge-traits** - Host platform abstractions (filesystem, database, HTTP, storage)
- **bridge-desktop** - Native desktop implementations (Tokio, SQLite, reqwest)
- **bridge-wasm** - WebAssembly implementations (IndexedDB, Fetch API)

## Platform Support Matrix

| Module | Native | WASM | Notes |
|--------|--------|------|-------|
| core-async | ✅ | ✅ | Full async runtime parity |
| core-async-macros | ✅ | ✅ | Proc macro crate |
| core-runtime | ✅ | ✅ | Event bus, logging |
| core-auth | ✅ | ✅ | OAuth flows |
| core-library | ✅ | ✅ | Database abstraction |
| core-metadata | ✅ | ✅ | Tag extraction |
| core-playback | 🚧 | 🚧 | Audio decoding in progress |
| core-sync | ✅ | ✅ | Sync coordinator |
| core-service | 🚧 | 🚧 | Minor compilation issue |
| bridge-traits | ✅ | ✅ | Platform abstractions |
| bridge-desktop | ✅ | ❌ | Native-only |
| bridge-wasm | ❌ | ✅ | WASM-only |
| provider-google-drive | ✅ | ✅ | HTTP-based |
| provider-onedrive | 🚧 | 🚧 | To do |

**Legend**: ✅ Fully supported | 🚧 In progress | ⚠️ Compiles with warnings | ❌ Not applicable

## Building

```bash
# Build all crates
cargo build --workspace

# Build with all features
cargo build --workspace --all-features

# Build for release
cargo build --workspace --release

# Run tests
cargo test --workspace
```

## Building for Different Targets

```bash
# Native desktop build
cargo build --workspace --release

# WebAssembly build (requires wasm-pack)
wasm-pack build bridge-wasm --target web
wasm-pack build core-library --target web

# Run tests
cargo test --workspace

# WASM-specific tests
cargo test -p bridge-wasm --target wasm32-unknown-unknown
cargo test -p core-async --target wasm32-unknown-unknown
```

## Features

The workspace supports the following feature flags:

- `desktop-shims` (default) - Desktop platform implementations
- `ffi` - FFI bindings for iOS/Android
- `wasm` - WebAssembly bindings for web
- `lyrics` - Lyrics fetching from external providers
- `artwork-remote` - Remote artwork fetching
- `offline-cache` - Encrypted offline cache support

## Development Status

This project is currently in initial development. See `docs/ai_task_list.md` for the complete implementation roadmap.

## License

Don't know, updating...
