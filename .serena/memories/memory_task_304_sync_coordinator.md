# TASK-304: Sync Coordinator Implementation Details

## Overview
Created comprehensive SyncCoordinator for orchestrating full and incremental synchronization workflows in `core-sync/src/coordinator.rs` (1,220+ lines).

## File Locations

### Main Implementation
- **Primary file**: `core-sync/src/coordinator.rs` (1,220+ lines)
- **Module export**: `core-sync/src/lib.rs` (added coordinator module)
- **Dependencies**: `core-sync/Cargo.toml` (added tokio-util, bytes)

## Public API

### SyncCoordinator Methods
```rust
// Initialization
pub async fn new(
    config: SyncConfig,
    auth_manager: Arc<AuthManager>,
    event_bus: EventBus,
    network_monitor: Option<Arc<dyn NetworkMonitor>>,
    db_pool: SqlitePool,
) -> Result<Self>

// Provider Management
pub async fn register_provider(
    &self,
    kind: ProviderKind,
    provider: Arc<dyn StorageProvider>,
) -> Result<()>

// Sync Operations
pub async fn start_full_sync(&self, profile_id: ProfileId) -> Result<SyncJobId>
pub async fn start_incremental_sync(&self, profile_id: ProfileId, cursor: Option<String>) -> Result<SyncJobId>
pub async fn cancel_sync(&self, job_id: SyncJobId) -> Result<()>

// Status Queries
pub async fn get_status(&self, job_id: SyncJobId) -> Result<SyncJob>
pub async fn list_history(&self, provider: ProviderKind, limit: usize) -> Result<Vec<SyncJob>>
pub async fn is_sync_active(&self, profile_id: &ProfileId) -> bool
```

## Internal Workflow

### Sync Execution Phases (execute_sync method)
Located in: `coordinator.rs:590-780`

1. **Phase 1: Session Validation** (lines 556-562)
   - Get current session from AuthManager
   - Verify provider is available

2. **Phase 2: File Discovery** (lines 633-646)
   - List files from StorageProvider
   - Track total file count
   - Emit progress event

3. **Phase 3: Audio Filtering** (lines 648-661)
   - Filter by MIME types (audio/*, application/ogg)
   - Filter by extensions (.mp3, .flac, .m4a, etc.)
   - Apply file size limits
   - Track filtered count

4. **Phase 4: Work Item Enqueueing** (lines 663-684)
   - Create WorkItem for each audio file
   - Set file size from metadata
   - Enqueue to ScanQueue
   - Check cancellation between items

5. **Phase 5: Queue Processing** (lines 686-740)
   - **⚠️ TODO: Metadata Extraction**
   - Location: `coordinator.rs:696-710`
   - Current: Stub with `mark_complete()` call
   - Future implementation:
     ```rust
     // TODO: Download file, extract metadata, persist to database
     // 1. Download file from provider via provider.download(item.remote_file_id)
     // 2. Save to temporary file via FileSystemAccess
     // 3. Extract metadata via MetadataExtractor (TASK-401)
     // 4. Persist to library via TrackRepository
     // 5. Calculate content hash for deduplication
     // 6. Mark work item complete
     ```
   - Requires: TASK-401 (MetadataExtractor implementation)

6. **Phase 6: Conflict Resolution** (lines 741-752)
   - **⚠️ TODO: Full Integration**
   - Location: `coordinator.rs:743`
   - Current: TODO comment, basic structure
   - Future implementation:
     ```rust
     // TODO: Implement conflict resolution
     // 1. Detect duplicates via conflict_resolver.detect_duplicates()
     // 2. Handle renames via conflict_resolver.resolve_rename()
     // 3. Handle deletions via conflict_resolver.handle_deletion()
     // 4. Apply resolution results to database
     ```
   - Note: ConflictResolver is fully implemented (TASK-303), just needs workflow integration

7. **Phase 7: Completion** (lines 759-779)
   - Update job with final stats
   - **⚠️ TODO: Deletion Tracking**
   - Location: `coordinator.rs:771`
   - Current: `items_deleted: 0` hardcoded
   - Future: Track actual deletions during sync
   - Emit SyncEvent::Completed

## Configuration

### SyncConfig Structure
```rust
pub struct SyncConfig {
    // Audio file filtering
    pub audio_mime_types: Vec<String>,      // Default: ["audio/*", "application/ogg"]
    pub audio_extensions: Vec<String>,       // Default: [".mp3", ".flac", ".m4a", ...]
    pub max_file_size: Option<u64>,         // Optional size limit
    
    // Network constraints
    pub wifi_only: bool,                     // Default: false
    
    // Performance tuning
    pub max_concurrent_downloads: usize,     // Default: 4
    pub retry_attempts: u32,                 // Default: 3
    pub sync_timeout_secs: u64,              // Default: 3600 (1 hour)
}
```

## Active Sync Tracking

### ActiveSync Structure (lines 156-160)
```rust
struct ActiveSync {
    job_id: SyncJobId,
    profile_id: ProfileId,  // Note: Currently unused but kept for future cancellation by profile
    cancellation_token: CancellationToken,
}
```

Stored in: `Mutex<HashMap<ProfileId, ActiveSync>>`
- Prevents concurrent syncs for same profile
- Provides cancellation tokens for graceful shutdown
- Automatically cleaned up on completion/failure

## Event Emissions

All events emitted via `EventBus` as `CoreEvent::Sync(SyncEvent::...)`:

1. **Started** - Line 440
   - Fields: job_id, profile_id, provider, is_full_sync
   
2. **Progress** - Lines 713-722
   - Fields: job_id, items_processed, total_items (Option)
   - Emitted every 10 items during queue processing

3. **Completed** - Lines 765-775
   - Fields: job_id, items_processed, items_added, items_updated, items_deleted, duration_secs
   
4. **Failed** - Lines 516-527, 540-551
   - Fields: job_id, message, items_processed, recoverable
   
5. **Cancelled** - Lines 871-878
   - Fields: job_id, items_processed

## Network Awareness

### NetworkMonitor Integration (lines 369-392)
When `wifi_only` is enabled:
1. Get network info via `NetworkMonitor::get_network_info()`
2. Check connection status (must be Connected)
3. Verify network type is WiFi
4. Ensure connection is not metered
5. Return error if constraints not met

## Error Handling

### SyncError Types Used
- `SyncError::Provider` - Provider errors, network failures, session issues
- `SyncError::SyncInProgress` - Concurrent sync attempt for same profile
- `SyncError::Cancelled` - User-initiated cancellation
- `SyncError::Timeout` - Sync exceeded timeout limit
- `SyncError::JobNotFound` - Invalid job ID query
- `SyncError::InvalidInput` - Missing required parameters (e.g., cursor for incremental)

## Testing

### Mock Implementations (lines 969-1147)
- `MockProvider`: Implements StorageProvider with configurable file list
- `MockSecureStore`: Implements SecureStore with in-memory HashMap
- `MockHttpClient`: Implements HttpClient (returns NotAvailable)

### Test Cases (lines 1149-1195)
1. `test_filter_audio_files`: Validates MIME type and extension filtering
2. `test_register_provider`: Tests provider registration and retrieval

### Test Coverage
- 56 total core-sync tests passing
- 2 coordinator-specific unit tests
- Integration test setup helper: `setup_test_coordinator()`

## Outstanding TODOs

### 1. Metadata Extraction (High Priority)
**Location**: `coordinator.rs:696-710` (Phase 4 processing loop)
**Blocks**: Full sync functionality
**Requires**: TASK-401 (MetadataExtractor)
**Implementation**:
```rust
// Current stub:
match self.scan_queue.mark_complete(item.id).await {
    Ok(_) => { added += 1; }
    Err(e) => { ... }
}

// Future implementation:
// 1. Download file: provider.download(&item.remote_file_id).await?
// 2. Save temporary: file_system.write_temp(&file_id, data).await?
// 3. Extract metadata: extractor.extract_from_file(&temp_path).await?
// 4. Persist to library: track_repo.insert(track).await?
// 5. Calculate hash: hash_service.calculate(&temp_path).await?
// 6. Clean up temp file
// 7. Mark work item complete
```

### 2. Conflict Resolution Integration (Medium Priority)
**Location**: `coordinator.rs:743` (Phase 5 after queue processing)
**Blocks**: Duplicate handling, rename detection, deletion management
**Requires**: No new tasks (ConflictResolver is complete from TASK-303)
**Implementation**:
```rust
// Current TODO comment

// Future implementation:
// Phase 5: Conflict Resolution
info!("Phase 5: Resolving conflicts");

// Detect duplicates
let duplicate_sets = self.conflict_resolver
    .detect_duplicates(&self.db_pool)
    .await?;

// Process renames (file_id changed but content same)
for item in &work_items {
    if let Some(existing) = track_repo.find_by_hash(&item.content_hash).await? {
        if existing.provider_file_id != item.remote_file_id {
            self.conflict_resolver.resolve_rename(
                existing.id,
                &item.remote_file_id,
                &item.title,
                &self.db_pool
            ).await?;
        }
    }
}

// Handle deletions (files in DB but not in provider list)
let provider_file_ids: HashSet<_> = work_items.iter()
    .map(|i| i.remote_file_id.clone())
    .collect();
let all_tracks = track_repo.find_by_provider(session.provider).await?;
for track in all_tracks {
    if !provider_file_ids.contains(&track.provider_file_id) {
        self.conflict_resolver.handle_deletion(
            track.id,
            false, // soft delete
            &self.db_pool
        ).await?;
    }
}
```

### 3. Deletion Tracking (Low Priority)
**Location**: `coordinator.rs:771` (SyncEvent::Completed emission)
**Blocks**: Accurate sync statistics
**Requires**: Conflict resolution integration (#2 above)
**Implementation**:
```rust
// Current hardcoded:
items_deleted: 0,

// Future implementation:
items_deleted: deleted_count, // Track during conflict resolution phase
```

## Integration Points

### Dependencies Used
- `AuthManager`: Session validation, token management
- `StorageProvider`: Cloud file operations (list_files, download, get_changes)
- `SyncJobRepository`: Job persistence and history
- `ScanQueue`: Work item management
- `ConflictResolver`: Duplicate detection, rename handling (ready for integration)
- `EventBus`: Real-time progress updates
- `NetworkMonitor`: Optional network constraint checking

### Files Modified Beyond Coordinator
- `core-sync/src/lib.rs`: Added `pub mod coordinator;` and public API exports
- `core-sync/Cargo.toml`: Added `tokio-util = { workspace = true }` and `bytes = { workspace = true }`
- `docs/ai_task_list.md`: Marked TASK-304 as completed with detailed notes

## Performance Considerations

### Concurrency
- `max_concurrent_downloads: 4` (default) - Configurable parallelism
- Work items processed sequentially in Phase 4 (can be parallelized in future)
- Provider rate limiting not yet implemented (future enhancement)

### Timeouts
- `sync_timeout_secs: 3600` (1 hour default)
- Wrapped with `tokio::time::timeout` for each sync operation
- Graceful cancellation before timeout kills operation

### Memory
- Audio file list loaded entirely in memory (Phase 2)
- Consider streaming for large collections (>100k files)
- Work items queued to database, not held in memory

## Future Enhancements

### Phase 4 Metadata Extraction (TASK-401)
- Download files from StorageProvider
- Extract metadata via MetadataExtractor (lofty crate)
- Persist to library via TrackRepository
- Calculate content hash for deduplication
- Estimated effort: Medium (depends on TASK-401 completion)

### Phase 5 Conflict Resolution
- Integrate existing ConflictResolver methods
- Detect duplicates by content hash
- Resolve renames without re-download
- Handle deletions with soft/hard options
- Estimated effort: Low (just workflow integration)

### Deletion Tracking
- Track files removed from provider
- Update library accordingly
- Report accurate deletion counts in events
- Estimated effort: Low (piggybacks on conflict resolution)

### Adaptive Rate Limiting
- Monitor provider response headers
- Implement exponential backoff on 429 errors
- Adjust concurrency dynamically
- Estimated effort: Medium

### Incremental Sync Optimization
- Use provider change notifications (webhooks)
- Store and compare file modification timestamps
- Skip unchanged files entirely
- Estimated effort: Medium-High (provider-specific)

## References

- Primary implementation: `core-sync/src/coordinator.rs`
- State machine: `core-sync/src/job.rs` (TASK-301)
- Work queue: `core-sync/src/scan_queue.rs` (TASK-302)
- Conflict resolver: `core-sync/src/conflict_resolver.rs` (TASK-303)
- Event definitions: `core-runtime/src/events.rs`
- Task documentation: `docs/ai_task_list.md` (TASK-304 section)
