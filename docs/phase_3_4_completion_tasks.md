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

### TASK-304.2: Integrate Conflict Resolution into Sync Coordinator

- **Status**: Not Started
- **Description**: The `SyncCoordinator` has a `TODO` for conflict resolution. This task is to integrate the `ConflictResolver`.
- **File to Modify**: `core-sync/src/coordinator.rs` (inside the `execute_sync` function, after the processing loop)
- **Implementation Steps**:
    1. After the main processing loop, use the `ConflictResolver` to detect duplicates based on content hash.
    2. Implement logic to handle renames and deletions, which will require changes to how the provider change set is processed.
    3. Update the `items_deleted` statistic.
- **Dependencies**: `TASK-303` (which is code-complete).

### TASK-304.3: Implement Deletion Tracking

- **Status**: Not Started
- **Description**: The `SyncCoordinator` does not track deleted files. This needs to be implemented to keep the local library in sync with the provider.
- **File to Modify**: `core-sync/src/coordinator.rs`
- **Implementation Steps**:
    1. When performing an incremental sync, identify files that have been deleted from the provider.
    2. Use the `ConflictResolver` or `TrackRepository` to mark the corresponding tracks as deleted in the local database.
    3. Update the `items_deleted` count in the `SyncJobStats`.
- **Dependencies**: `TASK-304.2`.

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
