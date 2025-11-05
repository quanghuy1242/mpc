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

### TASK-402.1: Implement Remote Artwork Fetching

- **Status**: Not Started
- **Description**: The `ArtworkService` has stubs for fetching artwork from MusicBrainz and Last.fm. This task is to implement the API calls to these services.
- **File to Modify**: `core-metadata/src/artwork.rs`
- **Implementation Steps**:
    1. Implement the `fetch_from_musicbrainz` function to query the MusicBrainz and Cover Art Archive APIs.
    2. Implement the `fetch_from_lastfm` function to query the Last.fm API.
    3. Handle API keys and rate limiting.
    4. Ensure the `artwork-remote` feature flag correctly gates this functionality.
- **Dependencies**: None. Ready to implement.

### TASK-403.1: Implement Genius Lyrics Provider

- **Status**: Not Started
- **Description**: The `LyricsService` has a stub for the Genius provider, noting that it would require web scraping.
- **File to Modify**: `core-metadata/src/lyrics.rs`
- **Implementation Steps**:
    1. Investigate the feasibility of using the Genius API without violating their ToS.
    2. If feasible, implement the `fetch` method for the `GeniusProvider`.
    3. If not, this task may need to be re-evaluated or removed.
- **Dependencies**: None. Ready to implement.

### TASK-404.1: Fully Implement Enrichment Job Logic

- **Status**: Not Started
- **Description**: The `EnrichmentJob` is currently stubbed because it cannot resolve `artist_id` and `album_id` to names. This task is to add the necessary repository queries to make the job functional.
- **File to Modify**: `core-metadata/src/enrichment_job.rs`
- **Implementation Steps**:
    1. In the `enrich_track` function, before calling the `ArtworkService` or `LyricsService`, query the `ArtistRepository` and `AlbumRepository` to get the artist and album names.
    2. Pass the retrieved names to the respective services.
    3. This will likely require adding the `ArtistRepository` and `AlbumRepository` as dependencies to the `EnrichmentJob`.
- **Dependencies**: `TASK-203` (which is complete). Ready to implement.

## Cross-Cutting Concerns

### TASK-CC.1: Utilize Enhanced Schema Fields

- **Status**: Not Started
- **Description**: The `002_add_model_fields.sql` migration added several new columns to the database, but the application code does not yet use them. This task is to integrate these fields.
- **Files to Modify**:
    - `core-library/src/models.rs`: Ensure all new fields are present in the domain models.
    - `core-library/src/repositories/*.rs`: Update repository queries to insert, update, and select the new fields.
    - `core-metadata/src/extractor.rs`: Populate new fields like `genre` during metadata extraction.
    - `core-metadata/src/enrichment_job.rs`: The enrichment job could populate the `bio` and `country` fields for artists.
- **Implementation Steps**:
    1. Review each new field (`bio`, `country`, `is_public`, `genre`, `updated_at`) and identify where it should be read from and written to.
    2. Update the relevant structs and repository methods.
    3. Add logic to the `MetadataExtractor` and `EnrichmentJob` to populate these fields.
- **Dependencies**: `TASK-204-1` (schema part is complete). Ready to implement.
