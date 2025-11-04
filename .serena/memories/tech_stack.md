# Technology Stack

## Programming Language
- **Rust** (Edition 2021)
- Async-first architecture using Tokio runtime

## Core Dependencies

### Async Runtime
- `tokio` 1.40 - Full async runtime with all features
- `tokio-util` 0.7 - Utility crates for Tokio
- `futures` 0.3 - Async primitives
- `async-trait` 0.1 - Async trait support

### Error Handling
- `thiserror` 1.0 - Error derive macros (for library errors)
- `anyhow` 1.0 - Error context (for application errors)

### Serialization
- `serde` 1.0 - Serialization framework with derive
- `serde_json` 1.0 - JSON serialization

### Database
- `sqlx` 0.8 - Async SQL with compile-time checked queries
  - SQLite backend with migrations support
  - Runtime: tokio
  - Features: macros, migrate

### HTTP
- `reqwest` 0.12 - HTTP client with JSON, streaming, rustls-tls
  - No default features (rustls instead of native TLS)

### Logging & Tracing
- `tracing` 0.1 - Structured logging and instrumentation
- `tracing-subscriber` 0.3 - Log formatting (JSON, pretty-print, env-filter)

### Time & UUID
- `chrono` 0.4 - Date/time with serde support
- `uuid` 1.10 - UUID generation (v4) with serde support

### Cryptography
- `sha2` 0.10 - SHA-256 hashing for content deduplication

### Audio Processing
- `lofty` 0.21 - Audio metadata extraction (ID3, FLAC, Vorbis, MP4)
- `symphonia` 0.5 - Audio decoding (all codecs)

### Image Processing
- `image` 0.25 - Image manipulation for artwork

### Caching
- `lru` 0.12 - LRU cache implementation

### Security (Desktop)
- `keyring` 3.2 - OS keychain access (macOS Keychain, Windows Credential Manager, Linux Secret Service)

### Platform Integration
- `uniffi` 0.28 - FFI binding generation (iOS/Android)
- `wasm-bindgen` 0.2 - WASM bindings for web
- `wasm-bindgen-futures` 0.4 - Async support in WASM
- `web-sys` 0.3 - Web APIs for WASM

### Testing
- `mockall` 0.13 - Mock generation for traits
- Built-in Rust testing framework

### Utilities
- `bytes` 1.7 - Efficient byte buffer handling
- `dirs` - App directory discovery
- `base64` - Base64 encoding/decoding

## Build Profiles

### Development
- Optimization level: 0 (fast compile)
- Debug symbols: enabled

### Release
- Optimization level: 3 (maximum performance)
- LTO: thin (link-time optimization)
- Codegen units: 1 (better optimization)
- Strip symbols: enabled
- Debug: disabled

### Release with Debug
- Inherits from release
- Strip: disabled
- Debug: enabled

## Feature Flags
- `desktop-shims` (default) - Desktop platform implementations
- `ffi` - FFI bindings for iOS/Android
- `wasm` - WebAssembly bindings for web
- `lyrics` - Lyrics fetching from external providers
- `artwork-remote` - Remote artwork fetching
- `offline-cache` - Encrypted offline cache support
- `secure-store` (default in bridge-desktop) - OS keychain integration
