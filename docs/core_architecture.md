# Core Music Platform Architecture Plan

## Goals
- Provide a reusable Rust core that powers music playback apps across desktop and mobile platforms.
- Support authentication with multiple cloud providers (Google Drive, OneDrive, future integrations).
- Index, organize, and stream user-owned music with rich metadata and lyrics support.
- Offer an ergonomic API surface for UI layers via idiomatic Rust and optional FFI bindings.

## Non-Goals (Short Term)
- Building UI components.
- Handling DRM-protected or subscription streaming services.
- Implementing server-side components beyond optional companion services (e.g., lyrics provider).

## Architectural Layers
- **Core Application Layer**: Orchestrates modules through a `CoreService` façade.
- **Domain Modules**: Auth, provider connectors, sync/indexing, library management, metadata/lyrics, playback/streaming, caching, configuration.
- **Infrastructure Layer**: Async runtime, storage (SQLite + object cache), logging/telemetry, HTTP clients, queues.
- **Host Bridge Layer**: Traits for HTTP, file system, secure storage, network reachability, background execution implemented per platform.
- **Integration Layer**: FFI bridges (C/Swift/Kotlin) or WASM bindings, feature flags, platform-specific adaptors.

```
UI / Host App -> CoreService -> Domain Modules -> Host Bridges -> Storage/Providers -> Cloud APIs
```

## Host Platform Abstractions
- **HttpClient**: Trait exposing async request execution, TLS pinning, proxy policy, and shared cookie/token handling. Default implementation may wrap `reqwest` on desktop; mobile/web hosts can inject platform-native stacks.
- **FileSystemAccess**: Abstraction over local file IO for cache directories, temporary files, and offline downloads. Supports directory handles provided by host (Android SAF, iOS sandbox, web `FileSystem` API).
- **SecureStore**: Already referenced by Auth module for credential persistence; allows delegation to Keychain, Keystore, or browser storage.
- **SettingsStore**: Key-value store bridge (Android DataStore, iOS `UserDefaults`, browser `localStorage`) with transactional semantics.
- **NetworkMonitor**: Provides connectivity and metered/unmetered hints so sync/playback can adapt.
- **BackgroundExecutor**: Allows scheduling work respecting platform constraints (Android WorkManager, iOS BGTaskScheduler, web Service Worker).
- **LifecycleObserver**: Notifies the core about foreground/background transitions to pause jobs or release resources.
- **Clock/TimeSource**: Injectable to support deterministic testing and host-specified timezones.
- **LoggerSink**: Forwards `tracing` events to host logging pipelines.

> Each supported platform must ship concrete adapters for every required bridge trait. Desktop may bundle default Rust shims (e.g., `reqwest`, POSIX FS), while mobile/web builds wire in native SDKs. The core should fail fast with descriptive errors when a dependency is missing.

## Platform Awareness & Constraints
- **Web (WASM)**: Integration relies on WASM + JS glue instead of FFI. Browser environments lack long-running background execution and expose quota-limited storage; `BackgroundExecutor` and `FileSystemAccess` should gracefully degrade (e.g., in-memory buffers, user-triggered sync).
- **Mobile Sandboxes**: iOS/Android restrict direct filesystem writes. Use host-provided document directories or SAF handles for cache paths, and fall back to streaming buffers if persistent storage is unavailable.
- **Desktop**: Default shims can use POSIX/Win32 APIs but still honor injected traits so behavior stays overridable (e.g., corporate proxies).
- **Background Policy Divergence**: Task scheduling must respect platform-specific limits (iOS BGTaskScheduler windows, Android WorkManager constraints, macOS app nap). Queue state should persist so paused jobs resume cleanly.
- **Security Guarantees**: Confirm that injected secure stores expose the expected isolation (Keychain accessibility classes, Android hardware-backed keys, browser storage encryption) before enabling provider auth; log sanitization must follow host privacy policies.

## Platform Capability Matrix

| Capability | Desktop (macOS/Windows/Linux) | Android | iOS/iPadOS | Web (WASM) |
|------------|-------------------------------|---------|------------|------------|
| Background sync jobs | ✅ Full support via host scheduler or daemon; handle sleep/hibernation resume | ⚠️ WorkManager; delayed under Doze/Idle, needs constraints | ⚠️ BGTaskScheduler windows; user opt-in for frequent runs | ❌ No persistent background; rely on foreground actions or Service Worker pushes |
| Offline cache storage | ✅ Persistent disk cache with configurable size | ✅ App sandbox storage; respect SAF/Scoped Storage | ⚠️ Limited app sandbox; large caches may trigger system eviction | ⚠️ IndexedDB/OPFS quota limits; fall back to streaming |
| Secure credential storage | ✅ OS keychain/DPAPI | ✅ Android Keystore (hardware-backed where possible) | ✅ Keychain with chosen accessibility | ⚠️ WebCrypto + storage; ensure HTTPS and user consent |
| Push/on-demand sync triggers | ✅ System schedulers, custom services | ⚠️ Firebase JobDispatcher/WorkManager integration | ⚠️ Requires silent push entitlements | ⚠️ Service Worker push (requires user permission, limited reliability) |
| Metadata enrichment (lyrics/artwork APIs) | ✅ Unrestricted HTTP access | ✅ Requires network permission, proxying allowed | ✅ Must declare background fetch capability | ⚠️ CORS and API key exposure concerns; may require proxy |
| Playback buffering & prefetch | ✅ Full file system + RAM buffering | ✅ Limited by memory/foreground status | ⚠️ Foreground-only background audio APIs; memory pressure | ⚠️ Memory constrained; rely on MediaSource APIs |
| Audio codec support | ✅ Host decoders + optional `symphonia` | ⚠️ Mix of platform codecs; bundle extras as needed | ⚠️ AAC/ALAC native; additional codecs via core decoding | ⚠️ WASM decoding increases bundle size; rely on browser-supported formats when possible |

Legend: ✅ full support, ⚠️ supported with constraints/degradation, ❌ unsupported.

## Implementation Readiness Checklist
- Confirm final trait signatures for every host bridge (`HttpClient`, `FileSystemAccess`, `SecureStore`, `SettingsStore`, `NetworkMonitor`, `BackgroundExecutor`, `LifecycleObserver`, `Clock`, `LoggerSink`) and document expected error handling semantics.
- Identify which platforms ship first-class adapters in v1 and create issues for remaining platforms; ensure stub adapters panic with actionable messages when capability is missing.
- Lock initial cloud provider scope (Google Drive + OneDrive) and reconcile database schema/metadata assumptions accordingly.
- Finalize audio decoding strategy per platform (host-native vs. core `symphonia`), including codec licensing implications and feature-flag defaults.
- Choose FFI/WASM binding tooling and cross-platform build pipeline (CI matrix, artifact packaging, symbol stripping).
- Define secure storage requirements per platform and verify product/security stakeholders approve the posture.
- Produce developer onboarding docs covering environment setup, mock provider usage, and local database inspection.
- Schedule test harness development (unit, integration, platform capability checks) alongside feature implementation.
- Align on configuration/secret management workflow (per-app OAuth keys, environment injection, CI integration) and document rotation procedures.
- Capture performance budgets (startup time, memory footprint, CPU during sync/playback) to guide profiling and optimization.

## Platform Bridge Responsibilities

| Bridge Trait | Responsibilities | Default Shim (Desktop) | Mobile Owner | Web Owner | Fail-Fast Strategy |
|--------------|------------------|-------------------------|--------------|-----------|--------------------|
| `HttpClient` | OAuth signing, retry/backoff, TLS pinning, bandwidth hints | `reqwest` + middleware | Wrap OkHttp/NSURLSession | JS `fetch` wrapper | Panic with `CapabilityMissing::HttpClient` listing required features |
| `FileSystemAccess` | Persistent cache dirs, temp files, stream handles | POSIX/Win32 wrapper | SAF/document directory adapters | OPFS/IndexedDB-backed store | Downgrade to streaming-only + warning event |
| `SecureStore` | Refresh tokens, provider secrets | Keyring/DPAPI | Keystore/Keychain | WebCrypto + storage | Fail initialization with remediation steps |
| `SettingsStore` | Preferences, feature flags | SQLite-backed KV | DataStore/SharedPreferences | LocalStorage/IndexedDB | Promote read-only defaults if write fails |
| `NetworkMonitor` | Connectivity changes, metered hints | Platform netlink APIs | ConnectivityManager/Reachability | Navigator API + heuristics | Send `NetworkEvent::Indeterminate` |
| `BackgroundExecutor` | Schedule deferred jobs, report execution windows | Systemd/launchd tasks | WorkManager | Service Worker/alarm fallback | Disable background queue and note in diagnostics |
| `LifecycleObserver` | Foreground/background transitions | Window events | Activity/App lifecycle hooks | Visibility API | Switch core to low-power mode |
| `Clock` | Time source, timezone offsets | Std time + chrono | Same | JS Date API | N/A |
| `LoggerSink` | Forward structured logs | Tracing subscriber | Logcat/OSLog | Console/Remote logging | Revert to stdout logging |

## Modularity & Bundling Strategy
- **Crate Graph**: Split the core into focused crates (e.g., `core-runtime`, `core-auth`, `core-sync`, `core-library`, `core-playback`, `provider-google-drive`, `provider-onedrive`, `metadata-services`) so binary consumers can depend only on what they use.
- **Feature Flags**: Gate optional capabilities (`lyrics`, `artwork-remote`, `offline-cache`, `ffi`, `wasm`) with Cargo features. Ensure disabled features remove dependencies to minimize binary size and audit surface.
- **Provider Plugins**: Register storage providers through a trait factory discovered via Cargo features or dynamic registration (e.g., `ProviderRegistry::register(Box<dyn StorageProviderFactory>)`). Hosts compile in only the connectors they need.
- **Lazy Initialization**: Defer creation of heavy components (HTTP clients, metadata pipelines, database connections) until first use. Use `OnceCell`/`async_once_cell` to avoid startup cost when modules are unused.
- **Streaming APIs**: Avoid preloading large datasets; expose async streams/paginators so UI layers fetch on demand, allowing tree-shaking in JS/WASM builds.
- **Binary Size Budgets**: Track symbol size in CI (e.g., `cargo bloat`) and set thresholds per platform. Strip debug info in release artifacts; leverage LTO where appropriate.
- **WASM Bundling**: Use `wasm-bindgen` features for tree-shaking and instruct consumers to import only needed bindings. Provide modular JS entrypoints (`core.wasm`, `core.sync.js`, `core.playback.js`) for splitting in bundlers.
- **FFI Packaging**: Produce lightweight dynamic libraries per feature set (e.g., `libcore_base`, `libcore_sync`, `libcore_playback`). Document how host apps load only required modules to reduce app size.
- **Configurable Pipelines**: Allow hosts to disable expensive jobs (e.g., lyrics fetch) via `CoreConfig` to avoid pulling in extra dependencies when unnecessary.

## Resilience & Error Handling
- **Error Taxonomy**: Standardize error domains (`AuthError`, `ProviderError`, `SyncError`, `PlaybackError`, `CodecError`) with machine-readable codes and user-display strings supplied via localization tables.
- **Retry Policies**: Centralize exponential backoff, jitter, and circuit breaker logic in infrastructure layer; respect provider-specific rate limit headers and host network constraints.
- **Graceful Degradation**: When optional capabilities fail (lyrics fetch, artwork proxy, advanced codecs), emit events and continue core functionality without crashing.
- **State Recovery**: Persist in-flight sync progress and playback positions so unexpected termination (app kill, power loss) can resume cleanly.
- **Observability Hooks**: Emit structured events/metrics on error occurrences for analytics dashboards; include correlation IDs to trace multi-module flows.
- **User Messaging**: Provide APIs (`CoreService::get_health()`) to expose actionable status summaries (e.g., “Google Drive token expired”, “Lyrics provider unavailable”).

## Performance & Resource Management
- **Profiling Targets**: Establish budgets (e.g., <1s core bootstrap, <150ms track start latency with cached data, <20% CPU during metadata extraction).
- **Adaptive Throttling**: Monitor device thermal status/battery via host callbacks; reduce sync/decoding intensity on low-power states.
- **Memory Management**: Use bounded caches/streaming to avoid OOM on low-end devices; reuse buffers and prefer `Bytes`/`Arc` where sharing is viable.
- **Concurrency Model**: Leverage Tokio + structured concurrency; ensure long-running jobs are cancellable and respect host lifecycle events.
- **Instrumentation**: Integrate `tracing` spans for sync phases, decoding, and DB queries; feed into performance dashboards.
- **Load Shedding**: When providers throttle or network degrades, scale down concurrent requests and surface progress feedback to UI.

## Provider Rollout Plan
- **Milestone 1 (MVP)**: Google Drive connector (full sync, streaming, metadata fetch) + foundational library database. Requires OAuth consent screen approval and Drive change-list integration.
- **Milestone 2**: OneDrive connector leveraging Microsoft Graph, reusing shared OAuth/token refresh stack. Add provider-specific throttling rules and conflict policies.
- **Milestone 3**: Optional Dropbox/WebDAV connectors based on demand; ensure abstraction coverage (path-based listing vs. Drive-like IDs).
- Document provider-specific scopes, quotas, rate limits, and map them to telemetry counters for proactive monitoring.

## Web/WASM Strategy
- Use `wasm-bindgen` + JS glue layer to expose the core API; bundle host bridge implementations in TypeScript to satisfy trait requirements (HTTP via `fetch`, storage via OPFS/IndexedDB, background execution via Service Worker events).
- Service Worker handles push notifications and periodic sync where supported; when unavailable, UI prompts users to trigger sync manually.
- Cache policy: store small metadata blobs in IndexedDB; large audio segments stream through `ReadableStream` with optional Media Source Extension buffering. Implement cache size guardrails and eviction policy, surfacing warnings in diagnostics.
- Audio decoding: prefer browser-native decoders (MediaSource/AudioContext); fall back to compiling `symphonia` to WASM for formats not supported natively, guarded behind feature flags due to bundle size impact.
- Address CORS and API key exposure by routing third-party metadata requests through a companion proxy or using public APIs with restrictive scopes. Evaluate token rotation or signed requests if direct browser calls are unavoidable.

## Integration & Build Tooling
- Recommended binding stack:
  - Desktop (macOS/Windows/Linux): `cxx` or `ffi-support` for C ABI, with Swift/Kotlin wrappers generated per platform.
  - Mobile: `uniffi` for iOS/Android shared definitions, enabling typed bindings.
  - Web: `wasm-bindgen` + `wasm-pack` pipeline producing NPM package.
- Establish CI jobs for each target:
  - Rust unit/integration tests.
  - Android/iOS binding generation + smoke tests.
  - WASM build with headless browser tests verifying bridge integration.
- Define versioning strategy (semantic releases) and artifact distribution (e.g., Maven, Swift Package Manager, NPM).

## Security Requirements & Compliance
- All providers require OAuth client secrets stored outside the binary; inject via host configuration and avoid hard-coding.
- Enforce HTTPS for every HTTP call; enable certificate pinning where providers permit.
- Require host secure storage to provide:
  - Hardware-backed keys when available.
  - Automatic lock when device is locked (configurable accessibility for background sync).
  - Export protections to prevent tokens leaving the device without explicit user action.
- Log filtering: redact tokens, email addresses, file paths; provide structured event codes for diagnostics instead of raw payloads.
- Implement privacy review checklist covering data residency, third-party API compliance, and user consent flows.

## Module Breakdown

### 1. Authentication Module
- Unified credential manager with pluggable providers implementing an `AuthProvider` trait.
- Supports OAuth 2.0 (Google Drive) and MSAL (OneDrive) flows via embedded/browser-based auth.
- Persists refresh tokens securely (delegates to host platform secure storage via trait `SecureStore`).
- Emits auth state events: `SignedOut`, `SigningIn`, `SignedIn`, `TokenRefreshing`.
- Public API: `CoreService::list_providers()`, `CoreService::sign_in(provider_id)`, `CoreService::sign_out(profile_id)`, `CoreService::current_session()`.

### 2. Provider Connector Module
- Abstraction around cloud file APIs with `StorageProvider` trait:
  - `list_media(start_page_token) -> Stream<Vec<RemoteFile>>`
  - `download(remote_id, range) -> AsyncRead`
  - `get_changes(cursor)` for incremental sync.
- Provider-specific implementations:
  - `GoogleDriveConnector` (Drive API v3).
  - `OneDriveConnector` (Microsoft Graph).
  - Future connectors (Dropbox, S3-compatible, WebDAV).
- Handles rate limiting, exponential backoff, retry policies.
- Uses host-supplied `HttpClient` implementation (or default desktop shim) via `ProviderHttpClient` wrapper to centralize OAuth middleware and rate limiting policies.
- Persists temporary download chunks through `FileSystemAccess` to avoid loading whole tracks into memory, falling back to streaming buffers when hosts expose only transient storage (iOS, web).

### 3. Sync & Indexing Module
- Drives initial scan and incremental updates using `SyncCoordinator`.
- Workflow:
  1. Acquire access token via Auth module.
  2. Walk provider files, filter audio types (by MIME, extension).
  3. Stage metadata extraction tasks (ID3/FLAC tags) before database insert.
  4. Persist library entities (Tracks, Artists, Albums, Playlists, Folders).
- Components:
  - `SyncJob` entity (provider_id, started_at, status, cursor).
  - `ScanQueue` for work items, persisted for resumability.
  - `ConflictResolver` for duplicates, renames, deleted files.
- Events: `SyncProgress`, `SyncCompleted`, `SyncError`.
- Supports resumable sync via stored cursors (Drive change tokens).

### 4. Library Management Module
- Owns the canonical database (SQLite via `sqlx` or `sea-orm`).
- Tables/entities:
  - `tracks` (id, provider_file_id, hash, title, album_id, artist_id, duration, bitrate, format, lyrics_status).
  - `artists` (id, name, normalized_name).
  - `albums` (id, name, artist_id, year, artwork_id).
  - `playlists` (id, name, owner_type, sort_order).
  - `playlist_tracks` (playlist_id, track_id, position).
  - `folders` (id, provider_id, name, parent_id).
  - `providers` (id, type, display_name, sync_cursor).
  - `artworks` (id, hash, binary_blob/blob_ref, width, height, dominant_color).
  - `lyrics` (track_id, source, synced, body, last_checked_at).
- Query APIs support browsing by songs, album, artist, playlist, folder with filtering/sorting.
- Exposes unified data models for UI layers via `LibraryRepository` trait returning paginated result streams.
- Implements search indexing (FTS5) for titles, artists, albums, lyrics (when available).

### 5. Metadata & Lyrics Module
- `MetadataExtractor` reads tags (ID3v2, Vorbis, MP4) using `lofty` crate, receiving file handles from `FileSystemAccess`.
- Normalizes metadata (title case, trimming, track numbers).
- Artwork pipeline:
  - Extract embedded artwork.
  - Optionally fetch remote artwork via external APIs (MusicBrainz, Last.fm).
  - Cache in filesystem/object store with dedup hash.
- Lyrics service:
  - `LyricsProvider` trait with implementations (e.g., Musixmatch, LRCLib) behind feature flags.
  - Supports synced lyrics (LRC) and fallback plain text.
  - Retries and caching based on track fingerprint (AcoustID) or metadata.

### 6. Playback & Streaming Module
- Provides streaming API returning `AudioSource` (local path or remote reader).
- Integrates with host audio engine via `PlaybackAdapter` trait and uses `FileSystemAccess` for buffered segments/offline caches.
- Supports pluggable decoding paths: host-managed decoding via `PlaybackAdapter` or core-managed decoding via `AudioDecoder` trait (default implementation using `symphonia` for formats not handled natively).
- Supports adaptive buffering, prefetch, and gapless track transitions.
- Enforces access controls (only signed-in providers, ensures tokens valid).
- Optional offline download manager storing encrypted files with license checks (future).

### 7. Audio Decoding & Analysis Module
- `AudioDecoder` trait exposes `probe`, `decode_frames`, and `seek` operations, allowing multiple decoding backends (core `symphonia`, platform codecs, hardware-accelerated decoders).
- Provides format support matrix (MP3, AAC, FLAC, Ogg Vorbis, WAV, ALAC) with feature flags enabling/disabling codecs to manage licensing and binary size.
- Supplies waveform/peak analysis helpers for UI visualizations and volume normalization (ReplayGain).
- Works in tandem with playback module and metadata extractor for consistent format handling.

### 8. Configuration & Preferences
- `SettingsStore` trait abstracting key-value storage (user preferences, feature flags) implemented by host.
- Handles quality preferences, sync scheduling, metadata fetch toggles.
- Supports profile-scoped settings and defaults.

### 9. Telemetry & Diagnostics
- Structured logging facade (`tracing`) with sinks to host app.
- Metrics hook for sync timings, API call counts, failure rates.
- Crash/issue reporting integration points.
- User-facing diagnostics surface (e.g., status dashboard) to expose sync errors, quota issues, and codec availability to host apps.

## Core API Surface (Rust Crate)

```rust
pub struct CoreService {
    inner: Arc<CoreContext>,
}

impl CoreService {
    pub async fn bootstrap(config: CoreConfig) -> Result<Self>;
    pub async fn sign_in(&self, provider: ProviderKind) -> Result<ProfileId>;
    pub async fn sign_out(&self, profile: ProfileId) -> Result<()>;
    pub async fn start_sync(&self, profile: ProfileId) -> Result<SyncJobId>;
    pub async fn get_sync_status(&self, job: SyncJobId) -> Result<SyncStatus>;
    pub async fn query_tracks(&self, filter: TrackFilter) -> Result<TrackPage>;
    pub async fn stream_track(&self, track: TrackId) -> Result<AudioSource>;
    pub fn subscribe_events(&self) -> EventStream<CoreEvent>;
}
```

- `CoreContext` wires modules together (auth, providers, repository, task scheduler).
- `CoreConfig` loads provider keys, database path, cache directories, feature flags, and host bridge implementations (`HttpClient`, `FileSystemAccess`, `SecureStore`, etc.), defaulting only where the core bundles a cross-platform shim.
- All async operations run on Tokio; provide `spawn_task` helper for host runtime integration.
- Provide opt-in `ffi` feature generating bindings (via `uniffi` or `cxx`) for iOS/Android/desktop.

## Background Jobs & Scheduling
- Dedicated `TaskScheduler` built on top of `tokio::task`, with persistent queue (SQLite) and host `BackgroundExecutor` integration for deferred runs.
- Job categories:
  - `SyncFullScan`
  - `SyncIncremental`
  - `MetadataEnrichment`
  - `ArtworkFetch`
  - `LyricsFetch`
  - `CacheCleanup`
- Queue supports prioritization, backoff, and resumability (store payload + attempt count).
- Background workers respect network constraints (Wi-Fi only) via host-provided callbacks and query `BackgroundExecutor` for allowable windows per platform.

## Data Storage & Caching Strategy
- Primary DB: SQLite (libsql optional) stored under app data directory.
- Binary assets (artwork, cached audio) stored in content-addressed file cache.
- Configurable cache size with LRU eviction.
- Expose migrations via `migrate()` API; support schema versioning.

## Eventing & State Observability
- Event bus built on `tokio::sync::broadcast` or `async-stream`.
- Core events:
  - `CoreEvent::Auth(AuthEvent)`
  - `CoreEvent::Sync(SyncEvent)`
  - `CoreEvent::Library(LibraryEvent)` (e.g., playlist created)
  - `CoreEvent::Playback(PlaybackEvent)`
- Supports snapshot queries + event subscriptions for reactive UIs.

## Security Considerations
- Delegate token storage to platform secure module via `SecureStore`.
- Validate secure-store capabilities at startup (hardware-backed keys, accessibility classes) and surface actionable diagnostics if requirements are unmet.
- Encrypt on-disk caches (optional) with platform-provided keys.
- Sanitize metadata before logs; avoid storing PII beyond IDs and display names.
- Minimize requested OAuth scopes; leverage incremental auth (e.g., Drive appdata scope for settings).
- Document data retention policy for cached content, respecting provider ToS and user privacy preferences.
- Provide opt-in telemetry and ensure compliance with GDPR/CCPA (ability to export/delete user data, consent prompts for analytics).

## Testing Strategy
- Unit tests per module (mocking provider connectors and secure store).
- Integration tests using in-memory SQLite and fake storage provider.
- Contract tests per provider to validate API changes (run via feature flags).
- End-to-end smoke test harness: spin up CoreService, seed fake provider, verify sync and queries.
- Platform harness tests verifying WASM/FFI bridge compliance (mock host traits, ensure fail-fast diagnostics when capabilities missing).
- Stress/regression tests covering intermittent network, provider throttling, offline/online transitions, and cache eviction policies.
- Performance benchmarks for sync throughput, playback start latency, and decoding CPU usage across representative devices.

## Open Questions
- Should lyrics and artwork services run locally or via a companion backend? (impacts API keys).
- How to share authentication sessions across multiple host apps securely?
- What quota constraints exist per provider, and how do we expose them to users?
- Which FFI technology best balances ergonomics and binary size across platforms?
- How do we surface user-controlled privacy/telemetry settings consistently across platforms while honoring legal requirements?
- Should we offer shared services (e.g., centralized metadata proxy) to mitigate CORS/API-key restrictions for web clients?
- What is the long-term strategy for codec licensing (e.g., AAC patents) and distributing optional decoders per region?
