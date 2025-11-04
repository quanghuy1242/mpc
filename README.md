# Music Platform Core

A cross-platform music playback core library written in Rust, designed to power desktop, mobile (iOS/Android), and web applications.

## Workspace Structure

This is a multi-crate workspace with the following modules:

- **core-runtime** - Logging, config, event bus, task scheduler
- **core-auth** - Authentication & credential management
- **core-sync** - Sync orchestration & indexing
- **core-library** - Database & repository layer
- **core-metadata** - Tag extraction, artwork, lyrics
- **core-playback** - Streaming & audio decoding
- **provider-google-drive** - Google Drive connector
- **provider-onedrive** - OneDrive connector
- **bridge-traits** - Host platform abstractions
- **bridge-desktop** - Desktop default implementations
- **core-service** - Main façade API

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

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
