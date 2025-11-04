# Music Platform Core - Project Overview

## Purpose
A cross-platform music playback core library written in Rust, designed to power desktop, mobile (iOS/Android), and web applications. The core provides unified music library management, cloud storage integration (Google Drive, OneDrive), metadata extraction, playback streaming, and lyrics support.

## Key Features
- **Multi-platform**: Desktop, iOS, Android, and Web support
- **Cloud Integration**: Google Drive and OneDrive connectors for music storage
- **Music Library Management**: SQLite-based database with full-text search
- **Metadata Extraction**: ID3v2, FLAC, Vorbis, MP4 tags with artwork and lyrics
- **Audio Playback**: Streaming with caching, offline support, and multiple format decoding
- **Authentication**: OAuth 2.0 with secure token storage
- **Sync Engine**: Full and incremental synchronization with conflict resolution

## Architecture Layers
1. **Core Application Layer**: `CoreService` façade orchestrating all modules
2. **Domain Modules**: Auth, providers, sync, library, metadata, playback, caching, config
3. **Infrastructure Layer**: Async runtime (Tokio), storage (SQLite), logging (tracing), HTTP clients
4. **Host Bridge Layer**: Platform-agnostic traits implemented per platform
5. **Integration Layer**: FFI bridges (Swift/Kotlin/C), WASM bindings, feature flags

## Current Status
- **Phase 0 (Foundation)**: 4/6 tasks completed ✅
  - ✅ Workspace structure initialized (11 crates)
  - ✅ Host bridge traits defined (9 comprehensive traits)
  - ✅ Desktop bridge shims implemented (6 modules)
  - ✅ Logging & tracing infrastructure complete
  - 🚧 Core configuration system (next task)
  - 📋 Event bus system (planned)
- **Phase 1-10**: Planned but not yet started

## Project Structure
- `core-runtime/` - Logging, config, event bus, task scheduler
- `core-auth/` - Authentication & credential management
- `core-sync/` - Sync orchestration & indexing
- `core-library/` - Database & repository layer
- `core-metadata/` - Tag extraction, artwork, lyrics
- `core-playback/` - Streaming & audio decoding
- `provider-google-drive/` - Google Drive connector
- `provider-onedrive/` - OneDrive connector
- `bridge-traits/` - Host platform abstractions
- `bridge-desktop/` - Desktop default implementations
- `core-service/` - Main façade API
