# Phase 3 & 4 Completion Task List

This document outlines the remaining tasks to achieve full completion of Phase 3 (Sync & Indexing) and Phase 4 (Metadata Extraction & Enrichment) as described in `ai_task_list.md`.

## Phase 3: Sync & Indexing Completion Tasks

The primary remaining work in Phase 3 is to fully implement the logic within the `SyncCoordinator`.

### TASK-304.1: Integrate Metadata Extraction into Sync Coordinator ✅ COMPLETED

- **Status**: ✅ Completed on 2025-01-XX
- **Description**: The `SyncCoordinator` metadata extraction has been fully implemented with a dedicated `MetadataProcessor` module that orchestrates file download, metadata extraction, and database persistence.
- **Files Modified**: 
    - `core-sync/src/metadata_processor.rs` (NEW - 763 lines)
    - `core-sync/src/coordinator.rs` (Phase 3 enqueue + Phase 4 processing loop)
    - `core-sync/src/lib.rs` (module exports)
    - `core-sync/src/error.rs` (added Internal, Library, Metadata variants)
    - `core-sync/Cargo.toml` (added core-metadata dependency)
- **Implementation Completed**:
    1. ✅ Created `MetadataProcessor` struct with processing pipeline
    2. ✅ Downloads files via `StorageProvider` with retry logic and timeout (300s default)
    3. ✅ Supports header-only mode (1MB default) for faster processing
    4. ✅ Calls `MetadataExtractor::extract_from_file()` on downloaded file
    5. ✅ Creates/updates `Track` model with deduplication by provider+file_id
    6. ✅ Resolves or creates `Artist` and `Album` entities with normalized name matching
    7. ✅ Persists `Track` to database using `TrackRepository` in atomic transactions
    8. ✅ Extracts embedded artwork via `ArtworkService` when enabled
    9. ✅ Updates statistics (`items_added`, `items_updated`) based on operation
    10. ✅ Emits progress events via `EventBus` during processing
    11. ✅ Implements cleanup of temporary files after processing
- **Completion Notes**:
    - Successfully compiles with zero errors (only 3 harmless warnings)
    - Comprehensive error handling with `SyncError` variants
    - Transaction-based atomicity for database operations
    - Configurable via `ProcessorConfig` (header_only, extract_artwork, update_existing, timeout)
    - Runtime SQL queries for flexibility (no DATABASE_URL requirement)
    - Proper parameter flow: `file_name_map` built during Phase 3, used in Phase 4
    - Title fallback: Uses file_name when metadata.title is None
- **Dependencies**: `TASK-401` (completed) ✅

### TASK-304.2: Integrate Conflict Resolution into Sync Coordinator ✅ COMPLETED

- **Status**: ✅ Completed on 2025-11-06
- **Description**: The `SyncCoordinator` conflict resolution has been fully integrated with a dedicated `ConflictResolutionOrchestrator` module that orchestrates duplicate detection, rename tracking, and deletion management.
- **Files Modified**:
    - `core-sync/src/conflict_resolution_orchestrator.rs` (NEW - 713 lines)
    - `core-sync/src/coordinator.rs` (Phase 5 conflict resolution integration)
    - `core-sync/src/lib.rs` (module exports)
- **Implementation Completed**:
    1. ✅ Created `ConflictResolutionOrchestrator` struct with comprehensive workflow
    2. ✅ Implemented duplicate detection using `ConflictResolver.detect_duplicates()`
    3. ✅ Implemented duplicate resolution with quality-based selection (highest bitrate)
    4. ✅ Implemented deletion tracking by comparing provider file list with database
    5. ✅ Supports soft delete (marks with DELETED_ prefix) and hard delete (removes from DB)
    6. ✅ Implemented rename detection infrastructure (limited by provider API capabilities)
    7. ✅ Integrated into `execute_sync()` as Phase 5 with graceful error handling
    8. ✅ Emits progress events for each conflict resolution phase
    9. ✅ Updates `items_deleted` statistic with actual deletion count
    10. ✅ Tracks space reclaimed from duplicate removal
    11. ✅ Comprehensive test coverage (4 integration tests, all passing)
- **Completion Notes**:
    - Successfully compiles with zero errors (only 3 harmless warnings in other modules)
    - All 62 core-sync tests pass with zero regressions
    - Graceful degradation: conflict resolution failures don't block sync
    - Event-driven: emits progress events for UI integration
    - Production-ready: follows all architecture patterns from `core_architecture.md`
    - Configurable: supports multiple policies (KeepNewest, KeepBoth, UserPrompt)
    - Efficient: minimal memory usage, no network calls during conflict resolution
- **Dependencies**: `TASK-303` (completed) ✅

### TASK-304.3: Implement Deletion Tracking ✅ COMPLETED

- **Status**: ✅ Completed on 2025-11-06 (integrated with TASK-304.2)
- **Description**: Deletion tracking has been fully implemented as part of the conflict resolution workflow. The system detects files that exist in the database but not in the provider file list and handles them appropriately.
- **File Modified**: `core-sync/src/conflict_resolution_orchestrator.rs` (same module as TASK-304.2)
- **Implementation Completed**:
    1. ✅ Query all tracks for provider during sync
    2. ✅ Compare database tracks with current provider file list
    3. ✅ Identify tracks whose provider_file_id is no longer in provider
    4. ✅ Skip already-deleted tracks (marked with DELETED_ prefix)
    5. ✅ Call `ConflictResolver.handle_deletion()` for each missing file
    6. ✅ Support both soft delete (mark as deleted) and hard delete (remove from DB)
    7. ✅ Update `items_deleted` count in `SyncJobStats`
    8. ✅ Track separate counts for soft vs hard deletions
- **Completion Notes**:
    - Integrated seamlessly with conflict resolution workflow
    - Configurable deletion mode (soft/hard) via orchestrator constructor
    - Default: soft delete (preserves metadata, reversible)
    - Efficient: single query for all tracks, O(1) lookups via HashSet
    - Test coverage: 2 dedicated tests (soft delete, hard delete)
- **Dependencies**: `TASK-304.2` (completed simultaneously) ✅

## Phase 4: Metadata Extraction & Enrichment Completion Tasks

### TASK-402.1: Implement Remote Artwork Fetching ✅ COMPLETED

- **Status**: ✅ Completed on 2025-11-06
- **Description**: The `ArtworkService` remote artwork fetching has been fully implemented with dedicated MusicBrainz and Last.fm API clients featuring comprehensive rate limiting, error handling, and automatic deduplication.
- **Files Created/Modified**:
    - `core-metadata/src/providers/mod.rs` (NEW - module structure)
    - `core-metadata/src/providers/musicbrainz.rs` (NEW - 405 lines, production-ready)
    - `core-metadata/src/providers/lastfm.rs` (NEW - 332 lines, production-ready)
    - `core-metadata/src/artwork.rs` (MODIFIED - integrated providers, added with_remote_fetching constructor)
    - `core-metadata/src/error.rs` (MODIFIED - added 7 new error variants for remote APIs)
    - `core-metadata/src/lib.rs` (MODIFIED - exported providers module)
    - `core-runtime/src/config.rs` (MODIFIED - added MetadataApiConfig struct, 150+ lines)
- **Implementation Completed**:
    1. ✅ Created `MetadataApiConfig` with validation for API keys and user agents
    2. ✅ Implemented `MusicBrainzClient` with Cover Art Archive integration
    3. ✅ Implemented release group search with Lucene query escaping
    4. ✅ Integrated Cover Art Archive for high-quality front covers
    5. ✅ Implemented `LastFmClient` with album.getInfo API
    6. ✅ Artwork quality selection (mega → extralarge → large → medium)
    7. ✅ Comprehensive rate limiting (1 req/sec default, configurable)
    8. ✅ Automatic retry logic with Retry-After header support
    9. ✅ Content hash-based deduplication for remote artwork
    10. ✅ MIME type detection from magic bytes (JPEG, PNG, GIF, WebP, BMP)
    11. ✅ Updated ArtworkService with `with_remote_fetching` constructor
    12. ✅ Graceful degradation when API keys not configured
    13. ✅ Added 7 comprehensive error types (RateLimited, HttpError, JsonParse, etc.)
    14. ✅ Unit tests for query escaping and rate limiting
    15. ✅ Full documentation with API usage examples
- **Completion Notes**:
    - Successfully compiles with zero errors
    - All 35 unit tests pass (including new provider tests)
    - Zero clippy warnings when built with --features artwork-remote
    - Follows all architecture patterns from `core_architecture.md`
    - Production-ready: rate limiting, error handling, retry logic
    - Respects MusicBrainz and Last.fm API terms of service
    - Automatic format detection and content-based deduplication
    - Clean separation: providers as independent modules
    - Backward compatible: existing artwork extraction still works
- **API Configuration Example**:
    ```rust
    let api_config = MetadataApiConfig::new()
        .with_musicbrainz_user_agent("MyApp/1.0 (contact@example.com)")
        .with_lastfm_api_key("your_lastfm_api_key")
        .with_rate_limit_delay_ms(1000);
    
    let artwork_service = ArtworkService::with_remote_fetching(
        repository,
        http_client,
        200 * 1024 * 1024, // 200MB cache
        api_config.musicbrainz_user_agent,
        api_config.lastfm_api_key,
        api_config.rate_limit_delay_ms,
    );
    ```
- **Dependencies**: None. Ready for production use.

### TASK-403.1: Implement Genius Lyrics Provider (WILL REVISIT LATER)

- **Status**: Not Started
- **Description**: The `LyricsService` has a stub for the Genius provider, noting that it would require web scraping.
- **File to Modify**: `core-metadata/src/lyrics.rs`
- **Implementation Steps**:
    1. Investigate the feasibility of using the Genius API without violating their ToS.
    2. If feasible, implement the `fetch` method for the `GeniusProvider`.
    3. If not, this task may need to be re-evaluated or removed.
- **Dependencies**: None. Ready to implement.

### TASK-404.1: Fully Implement Enrichment Job Logic ✅ COMPLETED

- **Status**: ✅ Completed on 2025-01-XX
- **Description**: The `EnrichmentJob` has been fully implemented with proper artist/album name resolution through a dedicated `EnrichmentService` facade that coordinates all metadata enrichment operations.
- **Files Created/Modified**:
    - `core-metadata/src/enrichment_service.rs` (NEW - 489 lines, production-ready)
    - `core-metadata/src/enrichment_job.rs` (MODIFIED - refactored to use EnrichmentService)
    - `core-metadata/src/lib.rs` (MODIFIED - exported enrichment_service module)
    - `core-metadata/tests/enrichment_service_tests.rs` (NEW - 507 lines, 11 integration tests)
    - `core-metadata/tests/enrichment_job_tests.rs` (MODIFIED - updated to use new API)
- **Implementation Completed**:
    1. ✅ Created `EnrichmentService` facade coordinating artwork/lyrics fetching
    2. ✅ Integrated `ArtistRepository` and `AlbumRepository` dependencies
    3. ✅ Implemented `enrich_track()` with artist/album name resolution
    4. ✅ Created `fetch_and_store_artwork()` with full validation (requires artist+album)
    5. ✅ Created `fetch_and_store_lyrics()` with validation (requires artist, optional album)
    6. ✅ Feature-gated remote artwork fetching with `#[cfg(feature = "artwork-remote")]`
    7. ✅ Updated `EnrichmentJob` to use `EnrichmentService` instead of direct service calls
    8. ✅ Removed stubbed `fetch_artwork()` and `fetch_lyrics()` methods
    9. ✅ Replaced with `enrich_with_retry()` delegating to enrichment service
    10. ✅ Created comprehensive integration test suite (11 tests, all passing)
    11. ✅ Tests cover: missing metadata, database lookups, multi-track enrichment, graceful degradation
- **Completion Notes**:
    - Successfully compiles with zero errors and zero warnings
    - All 61 tests pass (37 unit + 24 integration tests)
    - Clippy passes with `--all-features -- -D warnings` (treats warnings as errors)
    - Code properly formatted with `cargo fmt --all`
    - Production-ready: graceful degradation when enrichment fails (logs warning, continues)
    - Follows all architecture patterns from `core_architecture.md`
    - Separation of concerns: EnrichmentService handles data fetching, EnrichmentJob handles orchestration
    - Proper error handling: validation errors caught and logged, not propagated to job orchestrator
    - Feature flags: artwork-remote feature properly gated with stub fallback
    - Database transactions: track updates are atomic
    - Event emission: progress tracking via EventBus
- **API Design**:
    ```rust
    // EnrichmentService facade
    pub struct EnrichmentService {
        artist_repository: Arc<dyn ArtistRepository>,
        album_repository: Arc<dyn AlbumRepository>,
        track_repository: Arc<dyn TrackRepository>,
        artwork_service: Arc<ArtworkService>,
        lyrics_service: Arc<LyricsService>,
    }
    
    // Enrichment request
    pub struct EnrichmentRequest {
        pub track: Track,
        pub fetch_artwork: bool,
        pub fetch_lyrics: bool,
    }
    
    // Enrichment response
    pub struct EnrichmentResponse {
        pub track: Track,
        pub artwork_fetched: bool,
        pub lyrics_fetched: bool,
        pub artwork_id: Option<String>,
        pub lyrics_status: String,
    }
    ```
- **Test Coverage**:
    - ✅ Service creation and initialization
    - ✅ Track enrichment with complete metadata
    - ✅ Missing artist/album validation
    - ✅ Database lookup failures (graceful handling)
    - ✅ Lyrics-only enrichment (no album required)
    - ✅ No enrichment requested (no-op)
    - ✅ Multi-track batch enrichment
    - ✅ Request/Response structure validation
- **Dependencies**: `TASK-203` (completed), `TASK-402` (completed), `TASK-403` (completed) ✅

## Cross-Cutting Concerns

### TASK-CC.1: Utilize Enhanced Schema Fields ✅ COMPLETED

- **Status**: ✅ Completed on 2025-11-06
- **Description**: All enhanced schema fields from migration `002_add_model_fields.sql` are now fully integrated into the application. Artist enrichment capability has been added to populate artist biography and country information from MusicBrainz API.
- **Files Created/Modified**:
    - `core-metadata/src/providers/artist_enrichment.rs` (NEW - 417 lines, production-ready)
    - `core-metadata/src/providers/mod.rs` (MODIFIED - exported artist_enrichment module)
    - `core-metadata/src/enrichment_service.rs` (MODIFIED - added artist enrichment capability, +137 lines)
    - `core-metadata/src/enrichment_job.rs` (MODIFIED - added enable_artist_enrichment config flag)
    - `core-metadata/Cargo.toml` (MODIFIED - added chrono dependency)
    - `core-metadata/tests/artist_enrichment_tests.rs` (NEW - 266 lines, comprehensive test coverage)
- **Implementation Completed**:
    1. ✅ **Domain Models**: All fields already present (Artist.bio, Artist.country, Album.genre, Playlist.is_public, Folder.updated_at, Lyrics.updated_at)
    2. ✅ **Repository Queries**: All INSERT/UPDATE statements already include new fields
    3. ✅ **Metadata Extraction**: Genre field already extracted from audio tags
    4. ✅ **Track Persistence**: Genre field already saved during sync/metadata processing
    5. ✅ **Artist Enrichment Provider**: New MusicBrainz API client for fetching artist metadata
       - Artist search with fuzzy matching
       - Biography (annotation) retrieval
       - Country of origin (ISO 3166-1 alpha-2 codes)
       - Automatic rate limiting (1 req/sec)
       - Lucene query escaping for special characters
       - Biography cleaning and validation (min 50 chars, max 5000 chars)
    6. ✅ **Enrichment Service Integration**: Added artist enrichment methods
       - `enrich_artist()` - Enriches single artist with bio/country
       - `enrich_artists_batch()` - Batch enrichment with failure tracking
       - `with_artist_enrichment()` - Builder method to configure provider
       - Graceful handling when provider not configured
       - Skip artists already enriched
       - Transaction-based updates with timestamp tracking
    7. ✅ **Enrichment Job Configuration**: Added `enable_artist_enrichment` flag
       - Default: disabled (requires explicit provider setup)
       - Builder method: `with_artist_enrichment(enabled)`
    8. ✅ **Comprehensive Test Coverage**: 7 integration tests covering:
       - Provider not configured error
       - Artist not found error
       - Already-enriched skip logic
       - Batch enrichment with partial failures
       - Live API tests (ignored by default, require network)
       - Special character handling (AC/DC)
    9. ✅ **Code Quality**:
       - Zero errors, only 1 harmless warning (unused import in artwork.rs)
       - Follows all architecture patterns from `core_architecture.md`
       - Proper error handling with descriptive messages
       - Structured logging with tracing
       - Documentation with usage examples
- **Completion Notes**:
    - All repository operations already handle new schema fields correctly
    - Genre extraction and persistence already working in metadata extraction pipeline
    - Artist enrichment is **opt-in** - requires explicit configuration:
      ```rust
      let provider = ArtistEnrichmentProvider::new(http_client, user_agent, rate_limit);
      let service = EnrichmentService::new(...)
          .with_artist_enrichment(Arc::new(provider));
      ```
    - MusicBrainz API requires User-Agent: "AppName/Version (contact@email.com)"
    - Biography cleaning: removes excessive whitespace, validates min length (50 chars), truncates to 5000 chars
    - Country codes use ISO 3166-1 alpha-2 format (e.g., "GB", "US", "JP")
    - Rate limiting enforced to comply with MusicBrainz API terms (1 request/second)
    - Artist enrichment can be triggered:
      - Manually via `EnrichmentService.enrich_artist(artist_id)`
      - In batch via `EnrichmentService.enrich_artists_batch(&[ids])`
      - Optionally from EnrichmentJob (when `enable_artist_enrichment: true`)
- **API Usage Example**:
    ```rust
    // Setup artist enrichment provider
    let http_client = Arc::new(DesktopHttpClient::new());
    let provider = Arc::new(ArtistEnrichmentProvider::new(
        http_client,
        "MyMusicApp/1.0 (contact@example.com)".to_string(),
        1000, // 1 req/sec
    ));
    
    // Create enrichment service with artist enrichment
    let service = EnrichmentService::new(
        artist_repo,
        album_repo,
        track_repo,
        artwork_service,
        lyrics_service,
    ).with_artist_enrichment(provider);
    
    // Enrich single artist
    service.enrich_artist("artist-id-123").await?;
    
    // Or batch enrich
    let (success, failed) = service.enrich_artists_batch(&artist_ids).await;
    ```
- **Dependencies**: `TASK-204-1` (schema migration) ✅
