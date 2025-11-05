# Task 3.01: Sync Job State Machine Memory

## Overview
The sync job state machine is implemented in `core-sync/src/job.rs` and provides validated state transitions for managing the lifecycle of cloud storage sync operations. This is a critical component ensuring data integrity and preventing invalid state transitions during sync processes.

## State Machine Architecture

### States (SyncStatus enum)
```
Pending → Running → Completed
    ↓         ↓         
    └──────→ Failed    
    └──────→ Cancelled 
```

1. **Pending**: Job created but not started
2. **Running**: Job actively executing
3. **Completed**: Job finished successfully (terminal)
4. **Failed**: Job encountered error (terminal)
5. **Cancelled**: Job cancelled by user (terminal)

### Valid State Transitions

| From State | Valid To States | Notes |
|-----------|----------------|-------|
| Pending | Running, Cancelled, Failed | Initial state |
| Running | Completed, Failed, Cancelled | Active execution |
| Completed | None | Terminal state |
| Failed | None | Terminal state |
| Cancelled | None | Terminal state |

### State Classification
- **Terminal States**: `Completed`, `Failed`, `Cancelled` - cannot transition further
- **Active States**: `Pending`, `Running` - can still transition
- **Methods**: `is_terminal()`, `is_active()` provide state classification

## Core Types

### SyncJobId
- Wrapper around `Uuid` for type-safe job identification
- Implements: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`
- Methods: `new()`, `from_string()`, `as_str()`, `Display`

### SyncType
- **Full**: Complete library rescan from scratch
- **Incremental**: Resume from cursor position (uses `cursor` field)

### SyncProgress
Tracks real-time progress during sync execution:
```rust
pub struct SyncProgress {
    pub items_discovered: u64,  // Total items found
    pub items_processed: u64,   // Items completed
    pub items_failed: u64,      // Failed items
    pub percent: u8,            // 0-100 completion
    pub phase: String,          // Human-readable phase description
}
```

### SyncJobStats
Final statistics available only when `Completed`:
```rust
pub struct SyncJobStats {
    pub items_added: u64,
    pub items_updated: u64,
    pub items_deleted: u64,
    pub items_failed: u64,
}
```

## SyncJob Structure

### Fields
```rust
pub struct SyncJob {
    pub id: SyncJobId,
    pub provider_id: ProviderKind,
    pub status: SyncStatus,
    pub sync_type: SyncType,
    pub progress: SyncProgress,
    pub stats: Option<SyncJobStats>,       // Only set when Completed
    pub cursor: Option<String>,            // For incremental sync
    pub error_message: Option<String>,     // Set when Failed
    pub error_details: Option<String>,     // Additional error context
    pub created_at: i64,                   // Unix timestamp
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}
```

## State Machine Methods

### Transition Methods (Consume & Return Self)

#### `start() -> Result<Self>`
- **Transition**: `Pending → Running`
- Sets `started_at` timestamp
- Sets initial phase: "Starting sync"
- **Error**: `InvalidStateTransition` if not in `Pending`

#### `complete(stats: SyncJobStats) -> Result<Self>`
- **Transition**: `Running → Completed`
- Sets `completed_at` timestamp
- Stores final statistics
- Sets progress to 100% and phase to "Completed"
- **Error**: `InvalidStateTransition` if not in `Running`

#### `fail(error_message: String, error_details: Option<String>) -> Result<Self>`
- **Transition**: `Pending/Running → Failed`
- Sets `completed_at` timestamp
- Stores error information
- Sets phase to "Failed"
- **Error**: `InvalidStateTransition` if already terminal

#### `cancel() -> Result<Self>`
- **Transition**: `Pending/Running → Cancelled`
- Sets `completed_at` timestamp
- Sets phase to "Cancelled"
- **Error**: `InvalidStateTransition` if already terminal

### Mutation Methods (Update In-Place)

#### `update_progress(&mut self, items_processed, items_discovered, phase) -> Result<()>`
- Only valid in `Running` state
- Updates `progress` field with current counts and phase
- **Error**: `InvalidStateTransition` if not `Running`

#### `update_cursor(&mut self, cursor: String) -> Result<()>`
- Only valid in `Running` state
- Updates sync cursor for resumable incremental sync
- **Error**: `InvalidStateTransition` if not `Running`

### Query Methods

#### `duration_secs() -> Option<u64>`
- Returns job duration in seconds if both `started_at` and `completed_at` are set
- Returns `None` if job hasn't started or completed

### Internal Validation

#### `validate_transition(&self, to: SyncStatus) -> Result<()>`
- Private method enforcing state machine rules
- Returns `SyncError::InvalidStateTransition` with details if invalid
- Called by all state transition methods

## Usage Patterns

### Full Sync Workflow
```rust
// Create new job
let job = SyncJob::new(ProviderKind::GoogleDrive, SyncType::Full);

// Start execution
let mut job = job.start()?;

// Update progress during sync
job.update_progress(50, 100, "Processing files")?;
job.update_cursor("cursor_token_123")?;

// Complete successfully
let job = job.complete(SyncJobStats {
    items_added: 45,
    items_updated: 5,
    items_deleted: 0,
    items_failed: 0,
})?;
```

### Incremental Sync Workflow
```rust
// Resume from previous cursor
let job = SyncJob::new_incremental(
    ProviderKind::OneDrive, 
    "previous_cursor".to_string()
);

let mut job = job.start()?;
// ... process ...
job.update_cursor("new_cursor")?;
```

### Error Handling
```rust
let job = match execute_sync(&mut job).await {
    Ok(_) => job.complete(stats)?,
    Err(e) => job.fail(
        e.to_string(), 
        Some(format!("Debug: {:?}", e))
    )?,
};
```

## Error Handling

### SyncError::InvalidStateTransition
```rust
#[error("Invalid state transition from {from} to {to}: {reason}")]
InvalidStateTransition {
    from: String,
    to: String,
    reason: String,
}
```

This error is returned when:
- Calling `start()` on non-Pending job
- Calling `update_progress()` on non-Running job
- Calling `complete()` on non-Running job
- Attempting any transition from terminal states
- Invalid state combinations per state machine rules

## Database Persistence

The state machine is designed to persist across restarts:
- `SyncStatus::as_str()` provides database-friendly string representation
- Timestamps stored as Unix epoch integers
- All fields are serializable (`Serialize`, `Deserialize`)
- Jobs can be reconstructed from database and continue from last known state

## Testing Coverage

Comprehensive tests in `core-sync/src/job.rs::tests` module cover:
- ID generation and parsing
- Status classification (terminal, active)
- State transition validation
- Progress tracking and percent calculation
- Statistics accumulation
- Full workflow scenarios
- Invalid transition error cases
- Duration calculation
- Cursor management

## Key Design Principles

1. **Type Safety**: Strong types (`SyncJobId`, `SyncStatus`) prevent invalid operations
2. **Fail-Fast**: Invalid transitions return errors immediately
3. **Immutability**: State transitions consume and return `Self`, preventing partial updates
4. **Progress Tracking**: Real-time progress updates for UI feedback
5. **Error Context**: Rich error messages with `from`, `to`, and `reason` fields
6. **Resumability**: Cursor support enables incremental sync across sessions
7. **Auditability**: Timestamps for all lifecycle events

## Integration Points

- **core-service**: Exposes sync operations via `CoreService` facade
- **core-sync coordinator**: Uses state machine for job lifecycle management
- **Database**: Persists job state for restart recovery
- **Event Bus**: Emits `SyncEvent` messages for UI updates
- **Provider traits**: Job metadata tracked alongside provider operations

## Future Considerations

- Potential for retry/restart from Failed state (would need state machine extension)
- Partial completion tracking for large syncs
- Pause/resume capability (would need new `Paused` state)
- Progress checkpointing for better crash recovery
- Rate limiting integration (could use job state)

## Related Files
- `core-sync/src/job.rs` - State machine implementation
- `core-sync/src/error.rs` - Error types including `InvalidStateTransition`
- `core-sync/src/coordinator.rs` - Orchestrates job execution
- `core-sync/src/lib.rs` - Public API surface
