# Codebase Structure

## Workspace Layout

The project is a Cargo workspace with 11 member crates organized by domain:

```
mpc/
├── Cargo.toml                    # Workspace root with shared dependencies
├── Cargo.lock                    # Dependency lock file
├── README.md                     # Project overview
├── .github/
│   └── copilot-instructions.md   # AI assistant guidelines (804 lines)
├── docs/
│   ├── core_architecture.md      # Architecture documentation
│   └── ai_task_list.md          # Implementation task breakdown
└── [11 crates organized below]
```

## Core Crates

### 1. core-runtime
**Purpose**: Foundational runtime infrastructure
**Location**: `core-runtime/`
**Status**: ✅ Partially complete (logging done, config/events pending)

Files:
- `src/lib.rs` - Module exports and re-exports
- `src/config.rs` - `CoreConfig` and builder (🚧 TODO)
- `src/error.rs` - Runtime error types (✅)
- `src/events.rs` - Event bus system (📋 TODO)
- `src/logging.rs` - Structured logging with PII redaction (✅ 458 lines)
- `examples/logging_demo.rs` - Logging demonstration (✅)
- `tests/logging_integration.rs` - Integration tests (✅)
- `LOGGING.md` - Logging documentation (✅)

### 2. core-auth
**Purpose**: Authentication & credential management
**Location**: `core-auth/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Auth-specific errors
- `src/types.rs` - Auth domain types

### 3. core-sync
**Purpose**: Sync orchestration & indexing
**Location**: `core-sync/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Sync-specific errors

### 4. core-library
**Purpose**: Database & repository layer
**Location**: `core-library/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Library-specific errors
- `src/models.rs` - Domain models

### 5. core-metadata
**Purpose**: Tag extraction, artwork, lyrics
**Location**: `core-metadata/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Metadata-specific errors

### 6. core-playback
**Purpose**: Streaming & audio decoding
**Location**: `core-playback/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Playback-specific errors
- `src/traits.rs` - Playback abstractions

### 7. core-service
**Purpose**: Main façade API
**Location**: `core-service/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - `CoreService` façade
- `src/error.rs` - Service-level errors

## Bridge Crates

### 8. bridge-traits
**Purpose**: Host platform abstractions (traits)
**Location**: `bridge-traits/`
**Status**: ✅ Complete with comprehensive implementations

Files:
- `src/lib.rs` - Re-exports and overview documentation (124 lines)
- `src/error.rs` - `BridgeError` type
- `src/http.rs` - `HttpClient` trait with request/response types
- `src/storage.rs` - `FileSystemAccess`, `SecureStore`, `SettingsStore` traits
- `src/network.rs` - `NetworkMonitor` trait
- `src/background.rs` - `BackgroundExecutor`, `LifecycleObserver` traits
- `src/time.rs` - `Clock`, `LoggerSink` traits with default implementations

Key traits defined:
- 9 comprehensive trait definitions
- All traits have `Send + Sync` bounds
- Extensive documentation with platform-specific notes
- Helper types and enums for each domain
- 9 unit tests covering core functionality

### 9. bridge-desktop
**Purpose**: Desktop default implementations
**Location**: `bridge-desktop/`
**Status**: ✅ Complete with 6 implementations

Files:
- `src/lib.rs` - Module exports and re-exports
- `src/http.rs` - `ReqwestHttpClient` with retry logic
- `src/filesystem.rs` - `TokioFileSystem` with async I/O
- `src/secure_store.rs` - `KeyringSecureStore` (OS keychain)
- `src/settings.rs` - `SqliteSettingsStore` with transactions
- `src/network.rs` - `DesktopNetworkMonitor`
- `src/background.rs` - `TokioBackgroundExecutor` and `DesktopLifecycleObserver`
- `README.md` - Documentation

Implementations:
- 6 bridge trait implementations
- 19 unit tests (all passing)
- Feature-gated `secure-store` (default enabled)
- Zero clippy warnings

## Provider Crates

### 10. provider-google-drive
**Purpose**: Google Drive connector
**Location**: `provider-google-drive/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Provider-specific errors

### 11. provider-onedrive
**Purpose**: OneDrive connector
**Location**: `provider-onedrive/`
**Status**: 📋 Planned (skeleton only)

Files:
- `src/lib.rs` - Module exports
- `src/error.rs` - Provider-specific errors

## Build Artifacts

### target/
Generated build artifacts (not version controlled):
- `target/debug/` - Debug builds
- `target/release/` - Release builds
- `target/tmp/` - Temporary files
- `CACHEDIR.TAG` - Cache marker

## Module Dependency Graph

```
┌─────────────────┐
│  core-service   │  ← Main API façade
└────────┬────────┘
         │
         ├─────────────────────────────┐
         │                             │
┌────────▼────────┐          ┌────────▼────────┐
│  core-runtime   │          │  bridge-traits  │
│  (config,       │◄─────────┤  (platform      │
│   events,       │          │   abstractions) │
│   logging)      │          └────────┬────────┘
└────────┬────────┘                   │
         │                    ┌───────▼────────┐
         │                    │ bridge-desktop │
         │                    │ (desktop impls)│
         │                    └────────────────┘
         │
         ├─────┬─────┬─────┬─────┬─────┬─────┐
         │     │     │     │     │     │     │
    ┌────▼──┐ │  ┌──▼──┐ │  ┌──▼──┐ │  ┌───▼────┐
    │ auth  │ │  │sync │ │  │meta │ │  │provider│
    │       │ │  │     │ │  │data │ │  │  -gd   │
    └───────┘ │  └──┬──┘ │  └─────┘ │  └────────┘
              │     │    │          │
         ┌────▼──┐  │ ┌──▼────┐ ┌───▼────┐
         │library│  │ │playback│ │provider│
         │       │  │ │        │ │  -od   │
         └───────┘  │ └────────┘ └────────┘
```

## File Patterns

### Rust Source Files
- `lib.rs` - Crate entry point with module declarations
- `error.rs` - Error types using `thiserror`
- `types.rs` - Domain types and data structures
- `config.rs` - Configuration structs
- `*_impl.rs` or feature modules - Implementation files

### Documentation Files
- `README.md` - Crate/project overview
- `*.md` in `docs/` - Architecture and design docs
- `LOGGING.md` - Module-specific documentation

### Test Files
- `tests/*.rs` - Integration tests
- `src/*_test.rs` or `#[cfg(test)] mod tests` - Unit tests
- `examples/*.rs` - Runnable examples

## Important Configuration Files

### Cargo.toml (Workspace Root)
- Workspace member declarations
- Shared dependency versions (`[workspace.dependencies]`)
- Build profiles (dev, release, release-with-debug)
- Workspace-level metadata

### Crate-level Cargo.toml
- Crate metadata
- Feature flags
- Dependencies (referencing workspace versions)

## Current Implementation Status

✅ **Complete**:
- Workspace structure (11 crates)
- Bridge traits (9 comprehensive traits)
- Desktop bridge implementations (6 modules)
- Logging infrastructure with PII redaction
- Error types for foundation modules

🚧 **In Progress**:
- Core runtime (config and events pending)

📋 **Planned** (Skeletons exist):
- Authentication module
- Sync module
- Library/database module
- Metadata extraction
- Playback/streaming
- Cloud provider connectors
- Core service façade
