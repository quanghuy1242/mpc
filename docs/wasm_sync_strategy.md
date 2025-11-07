# WASM Sync Support Strategy

## ✅ Phase 1 Complete: Database Abstraction Migration (100% Complete)

### Migration Summary

All core-sync modules have been successfully migrated from `SqlitePool` to `DatabaseAdapter`:

1. **✅ repository.rs (SqliteSyncJobRepository)** - 667 lines
   - Migrated constructor to accept `Arc<dyn DatabaseAdapter>`
   - Converted all 15+ sqlx queries to DatabaseAdapter pattern
   - Updated all tests to use SqliteAdapter::from_pool()

2. **✅ scan_queue.rs (ScanQueue)** - 785 lines
   - Migrated constructor to accept `Arc<dyn DatabaseAdapter>`
   - Converted all 20+ sqlx queries to DatabaseAdapter pattern
   - Updated transaction handling to use DatabaseAdapter API
   - Updated all 8 test functions

3. **✅ conflict_resolution_orchestrator.rs** - 618 lines
   - Migrated from SqlitePool to Arc<dyn DatabaseAdapter>
   - Converted query_tracks_with_hashes() and query_all_tracks_for_provider()
   - Updated all 4 test functions

4. **✅ metadata_processor.rs** - 816 lines (after migration)
   - Migrated constructor to accept `Arc<dyn DatabaseAdapter>`
   - Converted transaction handling from pool.begin() to db.begin_transaction()
   - Updated resolve_or_create_artist(), resolve_or_create_album()
   - Updated create_track() and update_track() with proper QueryValue conversions

5. **✅ conflict_resolver.rs** - 925 lines
   - Migrated from SqlitePool to Arc<dyn DatabaseAdapter>
   - Converted 20+ queries: detect_duplicates(), resolve_rename(), handle_deletion(), merge_metadata(), deduplicate()
   - Updated all 7 test functions

6. **✅ coordinator.rs** - 1439 lines
   - Constructor calls already using db.clone() for all three dependencies
   - Updated setup_test_coordinator() test helper to use SqliteAdapter
   - All 62 tests passing ✅

### Platform-Agnostic Design

core-sync is a **pure library** that accepts trait objects. It does NOT know about platform-specific implementations.

**Dependency Injection Pattern:**
```rust
// SyncCoordinator::new() signature:
pub async fn new(
    config: SyncConfig,
    auth_manager: Arc<AuthManager>,
    event_bus: Arc<EventBus>,
    network_monitor: Option<Arc<dyn NetworkMonitor>>,
    file_system: Arc<dyn FileSystemAccess>,  // ← Trait object
    db: Arc<dyn DatabaseAdapter>,             // ← Trait object
) -> Result<Self>
```

**Desktop Usage:**
```rust
// Desktop application creates SqliteAdapter
use bridge_desktop::SqliteAdapter;
use bridge_desktop::NativeFileSystem;

let db = Arc::new(SqliteAdapter::from_pool(pool));
let fs = Arc::new(NativeFileSystem::new());

let coordinator = SyncCoordinator::new(
    config, auth_manager, event_bus, None, fs, db
).await?;
```

**WASM Usage:**
```rust
// WASM application creates WasmDbAdapter
use bridge_wasm::{WasmDbAdapter, WasmFileSystem};

let mut db_adapter = WasmDbAdapter::new(db_config).await?;
db_adapter.initialize().await?;
let db = Arc::new(db_adapter) as Arc<dyn DatabaseAdapter>;

let fs = Arc::new(WasmFileSystem::new("app-name").await?) as Arc<dyn FileSystemAccess>;

let coordinator = SyncCoordinator::new(
    config, auth_manager, event_bus, None, fs, db
).await?;
```

**Key Point:** The caller (desktop app or wasm app) is responsible for:
1. Creating the platform-specific implementations (SqliteAdapter vs WasmDbAdapter)
2. Wrapping them as trait objects (Arc<dyn DatabaseAdapter>)
3. Passing them to SyncCoordinator

core-sync remains platform-agnostic and doesn't need platform-specific modules.

### Current Situation Analysis

#### Database Layer (SQLite) - ✅ RESOLVED
   - ✅ All core-sync modules use `DatabaseAdapter` trait
   - ✅ SqliteAdapter available for native (core_library::adapters::sqlite_native)
   - ✅ WasmDbAdapter available for WASM (bridge-wasm/src/database.rs, 430+ lines)
   - ✅ All 62 tests pass with new pattern

#### Networking - ✅ NO BLOCKER
   - ✅ Already using `StorageProvider` trait (abstraction)
   - ✅ Already using `HttpClient` trait (abstraction)

#### Async Runtime - ⚠️ DEPENDENCY ISSUE
   - ✅ Most async code uses `core-async` already
   - ✅ No direct tokio usage in core-sync source code
   - ❌ Some dependencies (mio, sqlx native features) still pulling incompatible WASM features
   
### Test Results

Native compilation:
```
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
```

WASM target compilation:
- core-sync module itself: ✅ Successfully migrated
- Dependencies blocking: mio v1.1.0, sqlx-sqlite native driver
- **Action needed**: Configure Cargo.toml to use WASM-compatible database driver for wasm32 target

## Blockers for core-sync WASM Compilation (OUTDATED - See Phase 1 Complete)

Apply the same pattern we used in `core-library`:

#### Step 1: Update ScanQueueRepository Trait
```rust
// Before (current)
#[async_trait(?Send)]  // Already has this from TASK-204-2
pub trait ScanQueueRepository: PlatformSendSync {
    async fn create_table(&self) -> Result<()>;
    async fn insert(&self, item: &WorkItem) -> Result<WorkItemId>;
    // ... other methods
}

// After
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ScanQueueRepository: PlatformSendSync {
    async fn create_table(&self, db: &dyn DatabaseAdapter) -> Result<()>;
    async fn insert(&self, db: &dyn DatabaseAdapter, item: &WorkItem) -> Result<WorkItemId>;
    // ... update all method signatures
}
```

#### Step 2: Create Adapter-Based Implementation
```rust
pub struct ScanQueueRepositoryImpl {
    // Remove: pool: SqlitePool
    // No fields needed - adapter passed to methods
}

impl ScanQueueRepository for ScanQueueRepositoryImpl {
    async fn insert(&self, db: &dyn DatabaseAdapter, item: &WorkItem) -> Result<WorkItemId> {
        let query = "INSERT INTO scan_queue (id, sync_job_id, ...) VALUES (?, ?, ...)";
        let params = vec![
            QueryValue::Text(item.id.to_string()),
            QueryValue::Text(item.sync_job_id.to_string()),
            // ... other params
        ];
        
        db.execute(query, &params).await?;
        Ok(item.id)
    }
    
    async fn find_next(&self, db: &dyn DatabaseAdapter, status: &str) -> Result<Option<WorkItem>> {
        let query = "SELECT * FROM scan_queue WHERE status = ? ORDER BY priority DESC, created_at ASC LIMIT 1";
        let params = vec![QueryValue::Text(status.to_string())];
        
        let rows = db.query(query, &params).await?;
        if let Some(row) = rows.first() {
            Ok(Some(WorkItem::try_from(row)?))
        } else {
            Ok(None)
        }
    }
}
```

#### Step 3: Update SyncJobRepository Similarly
Same pattern for `SqliteSyncJobRepository` in `repository.rs`.

#### Step 4: Update ScanQueue to Accept DatabaseAdapter
```rust
pub struct ScanQueue {
    repository: Arc<dyn ScanQueueRepository>,
    db: Arc<dyn DatabaseAdapter>,  // Add this
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl ScanQueue {
    pub async fn new(
        db: Arc<dyn DatabaseAdapter>,
        max_concurrent: usize,
    ) -> Result<Self> {
        let repository = Arc::new(ScanQueueRepositoryImpl::new());
        repository.create_table(&*db).await?;
        
        Ok(Self {
            repository,
            db,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        })
    }
    
    pub async fn enqueue(&self, item: WorkItem) -> Result<WorkItemId> {
        self.repository.insert(&*self.db, &item).await
    }
}
```

#### Step 5: Update SyncCoordinator
```rust
pub struct SyncCoordinator {
    db: Arc<dyn DatabaseAdapter>,  // Change from SqlitePool
    auth_manager: Arc<AuthManager>,
    scan_queue: Arc<ScanQueue>,
    // ... other fields
}

impl SyncCoordinator {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,  // Change signature
        auth_manager: Arc<AuthManager>,
        event_bus: EventBus,
        config: SyncConfig,
    ) -> Self {
        // Create scan_queue with db adapter
        let scan_queue = Arc::new(ScanQueue::new(db.clone(), config.max_concurrent).await?);
        
        Self {
            db,
            auth_manager,
            scan_queue,
            // ...
        }
    }
}
```

### Phase 2: WASM Database Implementation

✅ **ALREADY IMPLEMENTED!** The `WasmDbAdapter` exists in `bridge-wasm/src/database.rs` (430+ lines)

#### What's Already Done:
```rust
pub struct WasmDbAdapter {
    handle: JsValue,
    config: DatabaseConfig,
}

#[async_trait(?Send)]
impl DatabaseAdapter for WasmDbAdapter {
    async fn initialize(&mut self) -> BridgeResult<()> { /* ... */ }
    async fn query(&self, query: &str, params: &[QueryValue]) -> BridgeResult<Vec<QueryRow>> { /* ... */ }
    async fn execute(&self, statement: &str, params: &[QueryValue]) -> BridgeResult<u64> { /* ... */ }
    async fn begin_transaction(&self) -> BridgeResult<TransactionId> { /* ... */ }
    async fn commit_transaction(&self, transaction_id: TransactionId) -> BridgeResult<()> { /* ... */ }
    async fn rollback_transaction(&self, transaction_id: TransactionId) -> BridgeResult<()> { /* ... */ }
    async fn execute_batch(&self, statements: &[(&str, &[QueryValue])]) -> BridgeResult<Vec<u64>> { /* ... */ }
    async fn get_schema_version(&self) -> BridgeResult<i64> { /* ... */ }
    async fn apply_migration(&self, version: i64, up_sql: &str) -> BridgeResult<()> { /* ... */ }
    // ... all DatabaseAdapter methods fully implemented
}
```

#### Implementation Details:
- **Fully implements** the `DatabaseAdapter` trait with all 14 methods
- **Delegates to JavaScript**: Uses `bridgeWasmDb` namespace for actual database operations
- **Supports SQL.js**: JavaScript side can use sql.js (SQLite compiled to WASM) backed by IndexedDB
- **Transaction support**: Full transaction API (begin, commit, rollback)
- **Migration support**: Schema versioning and migration application
- **Batch operations**: Execute multiple statements efficiently
- **Statistics**: Connection pool stats, database size, etc.

#### JavaScript Bridge Requirements:
The host application must provide a `bridgeWasmDb` global object with these async functions:
- `init(config) -> handle`
- `query(handle, sql, params) -> rows`
- `execute(handle, sql, params) -> rowsAffected`
- `beginTransaction(handle) -> transactionId`
- `commitTransaction(handle, transactionId)`
- `rollbackTransaction(handle, transactionId)`
- `executeBatch(handle, statements)`
- `applyMigration(handle, version, sql)`
- etc. (14 total methods)

#### What This Means:
**Phase 2 is essentially complete!** Once Phase 1 (database abstraction migration) is done, core-sync will automatically work with `WasmDbAdapter`. The JavaScript host just needs to provide the `bridgeWasmDb` implementation (typically using sql.js + IndexedDB).

### Phase 3: Make Sync Operations Network-Aware

Even with database fixed, sync on WASM has special considerations:

#### 1. Background Sync API (Service Workers)
```rust
// In coordinator.rs
#[cfg(target_arch = "wasm32")]
async fn execute_sync_wasm(&self, job: SyncJob) -> Result<()> {
    // Register with Background Sync API
    // Allows sync to continue even if tab closes
    
    if let Some(background_executor) = &self.background_executor {
        background_executor.register_periodic_task(
            "music-sync",
            Duration::from_secs(3600), // Every hour
            Box::new(move || {
                // Sync logic
            })
        ).await?;
    }
}
```

#### 2. Storage Quota Management
```rust
// Check IndexedDB quota before downloading
#[cfg(target_arch = "wasm32")]
async fn check_storage_quota(&self) -> Result<bool> {
    // Use navigator.storage.estimate()
    // Returns available space
    // Warn if running low
}
```

#### 3. Network-Aware Downloads
```rust
// Already have NetworkMonitor trait
// Use it to pause sync on cellular
if let Some(network_monitor) = &self.network_monitor {
    let info = network_monitor.get_info().await?;
    if info.network_type != NetworkType::WiFi && self.config.wifi_only {
        return Err(SyncError::NetworkConstraint("WiFi required".into()));
    }
}
```

### Phase 4: Update Tests

All tests need to use `DatabaseAdapter` instead of `SqlitePool`:

```rust
#[cfg(test)]
mod tests {
    use core_library::db::SqliteAdapter;  // For native tests
    
    async fn create_test_db() -> Arc<dyn DatabaseAdapter> {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        Arc::new(SqliteAdapter::new(pool))
    }
    
    #[tokio::test]
    async fn test_scan_queue() {
        let db = create_test_db().await;
        let queue = ScanQueue::new(db, 5).await.unwrap();
        // ... test with db adapter
    }
}
```

## Implementation Roadmap

### ✅ Phase 1 Complete: Database Abstraction Migration (100%)

**Status as of November 7, 2025:**

**All Modules Completed:**
1. ✅ `repository.rs` (667 lines) - **COMPLETE**
2. ✅ `scan_queue.rs` (785 lines) - **COMPLETE**
3. ✅ `coordinator.rs` (1439 lines) - **COMPLETE**
4. ✅ `conflict_resolution_orchestrator.rs` (618 lines) - **COMPLETE**
5. ✅ `metadata_processor.rs` (816 lines) - **COMPLETE**
6. ✅ `conflict_resolver.rs` (925 lines) - **COMPLETE**

**Final Metrics:**
- **Lines Migrated**: 4,600+ lines across 6 modules
- **Files Complete**: 6 / 6 (100%)
- **Queries Converted**: 60+ SQL queries to DatabaseAdapter pattern
- **Tests Updated**: All 62 tests passing ✅
- **Compilation Status**: 0 errors
- **Pattern Established**: SqlitePool → Arc<dyn DatabaseAdapter> conversion complete

---

## 📋 Phase 2 Recommendation: JavaScript Bridge Implementation

### ✅ ACTUAL STATUS: core-sync is NOW WASM-buildable!

**Compilation Status (November 7, 2025):**
- ✅ **Native build**: PASSING (0 errors, 62 tests passing)
- ✅ **WASM build**: PASSING (0 errors)
- ✅ **Bridge implementations exist**:
  - `WasmDbAdapter` in bridge-wasm/src/database.rs (430+ lines)
  - `WasmFileSystem` in bridge-wasm/src/filesystem.rs (875+ lines)

**Changes Required for WASM Compatibility:**
1. ✅ Made `sqlx` dependency native-only in core-sync/Cargo.toml
2. ✅ Made `bridge-desktop` dev-dependency native-only
3. ✅ Removed unused `tokio-util` dependency
4. ✅ Fixed broadcast error import paths in core-runtime/src/events.rs
5. ✅ Added `available_permits()` method to WASM Semaphore in core-async/src/sync.rs
6. ✅ Added conditional `extract_from_file()` call in metadata_processor.rs
7. ✅ Added `Arc<dyn FileSystemAccess>` blanket impl in bridge-traits/src/storage.rs
8. ✅ Fixed doctest in conflict_resolver.rs to use DatabaseAdapter

**No platform-specific modules needed** - core-sync accepts trait objects via dependency injection.

### Current Status
- ✅ **Rust side complete**: All abstractions work, WASM compiles successfully
- ⏳ **JavaScript side needed**: Must implement `bridgeWasmDb` namespace with sql.js

### Recommendation: **Document Now, Implement Later**

**Rationale:**
1. Phase 1 (Rust migration) is the **critical path** - it enables all WASM work
2. JavaScript bridge is **separate task** - doesn't block other WASM modules
3. sql.js integration requires **frontend expertise** (IndexedDB, persistence)
4. Better to **complete full architecture document** before implementation

### What to Document Now:

Create a new document: `docs/wasm_javascript_bridge.md` covering:

1. **Required Functions** (14 total):
   ```javascript
   // bridgeWasmDb namespace requirements
   async init(config: string): Promise<number>
   async query(handle: number, sql: string, params: string): Promise<string>
   async execute(handle: number, sql: string, params: string): Promise<number>
   async beginTransaction(handle: number): Promise<string>
   async commitTransaction(handle: number, txId: string): Promise<void>
   async rollbackTransaction(handle: number, txId: string): Promise<void>
   async queryInTransaction(handle: number, txId: string, sql: string, params: string): Promise<string>
   async executeInTransaction(handle: number, txId: string, sql: string, params: string): Promise<number>
   async executeBatch(handle: number, statements: string): Promise<void>
   async getSchemaVersion(handle: number): Promise<number>
   async applyMigration(handle: number, version: number, sql: string): Promise<void>
   async isMigrationApplied(handle: number, version: number): Promise<boolean>
   async lastInsertRowid(handle: number): Promise<number>
   async getStatistics(handle: number): Promise<string>
   ```

2. **sql.js Integration Pattern**:
   - Use sql.js (SQLite compiled to WASM)
   - Load database from IndexedDB on init
   - Persist to IndexedDB after each transaction commit
   - Handle concurrent access via transaction queue

3. **Serialization Format**:
   - QueryValue JSON schema
   - QueryRow JSON schema
   - Error handling format

4. **Example Implementation** (pseudo-code skeleton)

5. **Testing Strategy**:
   - Unit tests for each function
   - Integration test with real WASM module
   - Browser compatibility matrix

### Implementation Timeline (Future):

**When to implement:**
- After core-library, core-playback also support WASM
- After we have a WASM demo application ready
- Estimated effort: 3-4 days for full implementation + testing

**Priority:** LOW (nice-to-have, not blocking)

**Pattern Established:**
```rust
// 1. Update trait with conditional async_trait
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Repository: Send + Sync {
    async fn method(&self, db: &dyn DatabaseAdapter, ...) -> Result<T>;
}

// 2. Remove SqlitePool field from implementation
pub struct SqliteRepository {}  // No fields needed

// 3. Convert queries to DatabaseAdapter
async fn insert(&self, db: &dyn DatabaseAdapter, item: &Item) -> Result<()> {
    db.execute(
        "INSERT INTO table (col1, col2) VALUES (?, ?)",
        &[QueryValue::Text(item.col1.clone()), QueryValue::Integer(item.col2)],
    ).await?;
    Ok(())
}

// 4. Manual row parsing with helper functions
fn row_to_model(row: &QueryRow) -> Result<Model> {
    Ok(Model {
        field: Self::get_string(row, "field")?,
        // ...
    })
}

// 5. Update tests with SqliteAdapter
#[core_async::test]
async fn test_method() {
    use core_library::adapters::sqlite_native::SqliteAdapter;
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let db: Arc<dyn DatabaseAdapter> = Arc::new(SqliteAdapter::from_pool(pool));
    // ... test with db
}
```

**Next Steps:**
1. Migrate `conflict_resolution_orchestrator.rs` (618 lines, 2-3 hours)
2. Migrate `metadata_processor.rs` (721 lines, 3-4 hours)
3. Migrate `conflict_resolver.rs` (925 lines, 3-4 hours)
4. Update coordinator.rs tests
5. Run full test suite: `cargo test --package core-sync`
6. Verify WASM compilation: `cargo check --package core-sync --target wasm32-unknown-unknown`

### ✅ Phase 1 Complete (November 7, 2025)
1. ✅ Document current state and strategy (this file)
2. ✅ Apply DatabaseAdapter pattern to `repository.rs` - **COMPLETE** (667 lines)
3. ✅ Apply DatabaseAdapter pattern to `scan_queue.rs` - **COMPLETE** (785 lines)
4. ✅ Apply DatabaseAdapter pattern to `conflict_resolution_orchestrator.rs` - **COMPLETE** (618 lines)
5. ✅ Apply DatabaseAdapter pattern to `metadata_processor.rs` - **COMPLETE** (816 lines)
6. ✅ Apply DatabaseAdapter pattern to `conflict_resolver.rs` - **COMPLETE** (925 lines)
7. ✅ Update SyncCoordinator dependencies fully - **COMPLETE**
8. ✅ Update all tests - **ALL 62 TESTS PASSING**
9. ✅ Verify native compilation - **0 ERRORS**

**Phase 1 Summary**: 
- Total lines migrated: ~4,600 lines across 6 modules
- Queries converted: 60+ SQL queries to DatabaseAdapter pattern
- Transaction handling: Updated to use TransactionId API
- Test coverage: All 62 core-sync tests passing
- Pattern established: SqlitePool → Arc<dyn DatabaseAdapter>

### Phase 2: JavaScript Bridge Implementation (Week 2)

**Status**: Core Rust infrastructure complete, needs JavaScript host implementation

1. ✅ **WasmDbAdapter already exists** - bridge-wasm/src/database.rs (430+ lines, fully implemented)
2. ⏳ **Create sql.js + IndexedDB implementation** (Recommended approach)
   - Use [sql.js](https://github.com/sql-js/sql.js) (SQLite compiled to WASM)
   - Persist database to IndexedDB for durability
   - Implement the `bridgeWasmDb` JavaScript namespace
3. ⏳ **Implement 14 required async functions**:
   - `init(config)` → Initialize sql.js database, load from IndexedDB
   - `query(handle, sql, params)` → Execute SELECT queries
   - `execute(handle, sql, params)` → Execute INSERT/UPDATE/DELETE
   - `beginTransaction(handle)` → Start transaction, return ID
   - `commitTransaction(handle, txId)` → Commit transaction
   - `rollbackTransaction(handle, txId)` → Rollback transaction
   - `queryInTransaction(handle, txId, sql, params)` → Query in transaction context
   - `executeInTransaction(handle, txId, sql, params)` → Execute in transaction context
   - `executeBatch(handle, statements)` → Batch operations
   - `getSchemaVersion(handle)` → Read schema_version table
   - `applyMigration(handle, version, sql)` → Run migration SQL
   - `isMigrationApplied(handle, version)` → Check migration status
   - `lastInsertRowid(handle)` → Get last inserted row ID
   - `getStatistics(handle)` → Return database stats
4. ⏳ **Handle serialization between Rust and JavaScript**:
   - Convert `QueryValue` enum to/from JavaScript values
   - Convert `QueryRow` HashMap to JavaScript objects
   - Handle NULL, Integer, Real, Text, Blob types
5. ⏳ **Create example/demo**:
   - Simple HTML page with WASM module loaded
   - Demonstrate sync operations in browser
   - Show IndexedDB persistence across page reloads

**Implementation Notes**:
- sql.js is ~500KB gzipped, loads entire SQLite into WASM
- IndexedDB provides durable storage (browser-managed quota)
- Transactions are in-memory until commit (then persisted to IndexedDB)
- Migration system allows schema evolution over time

### Phase 3: Production Readiness (Week 3-4)

1. ⏳ **Implement Background Sync API integration**
   - Use Service Workers for background sync
   - Allow sync to continue even if tab closes
   - Handle periodic sync for incremental updates
2. ⏳ **Add storage quota management**
   - Use `navigator.storage.estimate()` to check available space
   - Warn user when approaching quota limits
   - Implement cleanup strategies for old cached data
3. ⏳ **Network-aware sync optimization**
   - Pause sync on cellular if `wifi_only` configured
   - Implement adaptive throttling based on connection quality
   - Resume interrupted syncs gracefully
4. ⏳ **WASM bundle optimization**
   - Enable LTO (Link Time Optimization)
   - Use `wasm-opt` for size reduction
   - Code splitting if bundle is too large
5. ⏳ **Create comprehensive WASM sync demo**
   - Full UI showing sync progress
   - Library browsing after sync
   - Network status indicators
   - Storage quota display

---

## Migration Session Log

### Session: November 7, 2025 - Phase 1 Complete ✅

**Duration**: Full day session  
**Modules Completed**: 6/6 (ALL MODULES)  
**Lines Migrated**: ~4,600 lines  
**Compilation Status**: ✅ 0 errors, all 62 tests passing  
**Result**: **PHASE 1 COMPLETE - core-sync ready for WASM**

#### Work Completed:

**1. repository.rs Migration (667 lines)** ✅
- Updated SyncJobRepository trait with conditional async_trait
- Converted all 9 methods to accept `db: &dyn DatabaseAdapter`
- Removed SqlitePool field from SqliteSyncJobRepository
- Converted all sqlx queries to DatabaseAdapter::execute/query/query_one_optional
- Implemented manual row parsing with helper functions
- Updated all 10 tests to use `Arc<dyn DatabaseAdapter>`
- Tests use `SqliteAdapter::from_pool()` for test database creation

**2. scan_queue.rs Migration (785 lines)** ✅
- Updated ScanQueueRepository trait with conditional async_trait
- Converted all 7 methods to accept `db: &dyn DatabaseAdapter`
- Removed SqlitePool field from SqliteScanQueueRepository
- Converted all queries to use QueryValue arrays
- Implemented row parsing helper functions
- Updated ScanQueue struct to hold `Arc<dyn DatabaseAdapter>`
- Updated ScanQueue constructor to accept DatabaseAdapter
- Updated all 8 tests to use SqliteAdapter

**3. conflict_resolution_orchestrator.rs Migration (618 lines)** ✅
- Updated struct from `SqlitePool` to `Arc<dyn DatabaseAdapter>`
- Converted `query_tracks_with_hashes()` method
- Converted `query_all_tracks_for_provider()` method
- Updated constructor signature
- Updated all 4 test functions with SqliteAdapter pattern
- Zero compilation errors after migration

**4. metadata_processor.rs Migration (816 lines)** ✅
- Updated imports: removed sqlx, added DatabaseAdapter, TransactionId
- Changed struct field from `db_pool: SqlitePool` to `db: Arc<dyn DatabaseAdapter>`
- Updated constructor to accept `Arc<dyn DatabaseAdapter>`
- Converted transaction handling:
  - `pool.begin()` → `db.begin_transaction()` returning TransactionId
  - `tx.commit()` → `db.commit_transaction(tx_id)`
- Updated `resolve_or_create_artist()` with DatabaseAdapter queries
- Updated `resolve_or_create_album()` with conditional queries
- Updated `create_track()` with 26 QueryValue parameters
- Updated `update_track()` with 21 QueryValue parameters
- Fixed type conversions: `i32` → `i64`, handled Option types with `map_or`
- All content_hash handling corrected (String not Option<String>)

**5. conflict_resolver.rs Migration (925 lines)** ✅
- Updated imports: DatabaseAdapter, QueryValue
- Changed struct from `pool: SqlitePool` to `db: Arc<dyn DatabaseAdapter>`
- Updated constructor signature
- Converted 20+ query methods:
  - `detect_duplicates()` - GROUP BY query with aggregation
  - `resolve_rename()` - UPDATE provider_file_id and title
  - `handle_deletion()` - DELETE or soft-delete with marker
  - `merge_metadata()` - Dynamic UPDATE with multiple fields
  - `deduplicate()` - Complex quality-based selection with DELETE
- Updated all 7 test functions
- Created `create_test_db()` helper returning `Arc<dyn DatabaseAdapter>`
- Updated `create_test_track()` to accept DatabaseAdapter reference
- All tests passing with new pattern

**6. coordinator.rs Dependencies (test updates)** ✅
- Updated `setup_test_coordinator()` return type
- Changed from `SqlitePool` to `Arc<dyn DatabaseAdapter>`
- Added SqliteAdapter import
- Created db adapter from pool for test consistency
- All constructor calls already correct (using db.clone())
- All 62 tests passing

#### Migration Pattern Established:

```rust
// BEFORE (Native-only)
use sqlx::SqlitePool;
struct Repository { pool: SqlitePool }
let result = sqlx::query("SELECT * FROM table WHERE id = ?")
    .bind(id)
    .fetch_one(&pool)
    .await?;

// AFTER (Cross-platform)
use bridge_traits::database::{DatabaseAdapter, QueryValue};
struct Repository { db: Arc<dyn DatabaseAdapter> }
let rows = db.query(
    "SELECT * FROM table WHERE id = ?",
    &[QueryValue::Text(id.to_string())],
).await?;
let value = rows[0].get("field")
    .and_then(|v| v.as_string())
    .ok_or_else(|| Error::MissingField)?;
```

#### Test Results:

```
cargo test --package core-sync
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
```

#### Next Steps:

1. **Configure Cargo.toml for WASM target** - Use conditional compilation for sqlx features
2. **Implement JavaScript bridge** - Create `bridgeWasmDb` namespace with sql.js
3. **Test WASM build** - `cargo build --target wasm32-unknown-unknown --package core-sync`
- ✅ Updated constructor to accept DatabaseAdapter parameter
- ✅ Updated all 11 repository method calls to pass `db.as_ref()`
- ✅ Updated clone_for_task() to clone db field
- ✅ Fixed repository construction (removed pool parameter)
- ✅ ScanQueue::new() call now works (after scan_queue migration)
- ❌ **3 dependencies still blocked**:
  - ConflictResolver::new() - expects SqlitePool
  - MetadataProcessor::new() - expects SqlitePool  
  - ConflictResolutionOrchestrator::new() - expects SqlitePool

#### Technical Challenges Resolved:

1. **Import Path Discovery**
   - Initially tried `bridge_desktop::database::SqliteAdapter` (doesn't exist)
   - Discovered correct path: `core_library::adapters::sqlite_native::SqliteAdapter`
   - Pattern from repository.rs tests

2. **QueryValue Conversion**
   - Option types require `.map(QueryValue::Type).unwrap_or(QueryValue::Null)`
   - Integer casting: u32 → i64 for QueryValue::Integer
   - String cloning needed for QueryValue::Text

3. **Row Parsing Lifetime Issues**
   - COUNT queries needed `.as_ref()` before accessing fields
   - Cannot return borrowed data from QueryRow - must extract immediately

4. **Async Trait Conditional Compilation**
   - WASM needs `#[async_trait(?Send)]` (no Send bound)
   - Native needs standard `#[async_trait]`
   - Use `cfg_attr` for conditional application

5. **Test Adapter Creation**
   - Must use `SqliteAdapter::from_pool(pool)` not `::new()`
   - `::new()` is async and returns Result<T>, not T

#### Files Modified:
- `core-sync/Cargo.toml` - Added bridge-desktop as dev dependency (later found unnecessary)
- `core-sync/src/repository.rs` - Full migration (667 lines)
- `core-sync/src/scan_queue.rs` - Full migration (785 lines)
- `core-sync/src/coordinator.rs` - Partial migration (720 lines completed)

#### Compilation Status:
```bash
cargo check --package core-sync
# repository.rs: ✅ 0 errors
# scan_queue.rs: ✅ 0 errors  
# coordinator.rs: ❌ 4 errors (3 unmigrated dependencies + 2 test errors)
```

#### Next Session Plan:
1. Migrate `conflict_resolution_orchestrator.rs` (618 lines, smallest remaining)
2. Migrate `metadata_processor.rs` (721 lines)
3. Migrate `conflict_resolver.rs` (925 lines, largest remaining)
4. Update coordinator.rs tests
5. Run full test suite
6. Verify WASM compilation

#### Lessons for Next Modules:
- Pattern is well-established and works consistently
- Each module takes ~1-2 hours depending on complexity
- Test adapter creation: always use `SqliteAdapter::from_pool(pool)`
- Helper functions for row parsing are essential (reduce duplication)
- Add helper functions in implementation block, not trait
- Query one optional needs careful Option handling with `.as_ref()`

---

## Benefits of This Approach

1. **Same Codebase**: Business logic identical on native and WASM
2. **Already Proven**: We did this successfully for core-library
3. **Minimal Changes**: Only need to thread DatabaseAdapter through
4. **Testable**: Can test with mock adapters
5. **Progressive**: Can land phase 1 without breaking anything

## Architecture Alignment

This strategy follows our core principle:

> **All core business logic modules must be compilable for both native and WASM targets.**

The sync coordinator's logic (state machines, conflict resolution, prioritization) is platform-agnostic. Only the I/O layer (database, network) needs platform-specific implementations via traits.

## Files to Modify (Estimated)

### Phase 1: Database Abstraction Migration (PRIORITY)
1. `core-sync/src/scan_queue.rs` (~300 lines to update)
2. `core-sync/src/repository.rs` (~200 lines to update)
3. `core-sync/src/coordinator.rs` (~150 lines to update)
4. `core-sync/src/metadata_processor.rs` (~50 lines to update)
5. `core-sync/src/conflict_resolution_orchestrator.rs` (~50 lines to update)
6. All test files (~500 lines across multiple files)

**Total Estimate**: ~1250 lines to modify, 6-8 hours of work

### Phase 2: JavaScript Bridge Implementation (NEW WORK)
1. Create `bridge-js/wasm-db-bridge.js` or similar (NEW ~400 lines)
   - Implement sql.js wrapper with IndexedDB persistence
   - Expose `bridgeWasmDb` global namespace
   - Handle all 14 DatabaseAdapter methods

**Total Estimate**: ~400 lines new JavaScript code, 4-6 hours of work

**Note**: The Rust side (`WasmDbAdapter`) is already complete in bridge-wasm!

### Phase 3: WASM-Specific Features
1. `core-sync/src/coordinator.rs` (+100 lines for WASM-specific logic)
2. Background Sync API integration (~200 lines)

**Total Estimate**: ~300 lines new code, 3-4 hours of work

## Success Criteria

✅ Phase 1 Complete When:
- `cargo check --package core-sync --target wasm32-unknown-unknown` succeeds
- All 62 core-sync tests still pass on native
- No direct SqlitePool usage in core-sync
- All repository methods accept DatabaseAdapter parameter

✅ Phase 2 Complete When:
- JavaScript `bridgeWasmDb` implementation created
- sql.js properly integrated with IndexedDB for persistence
- Basic database operations work in browser console
- Migration system works (can initialize database schema)

✅ Phase 3 Complete When:
- Background sync works with Service Workers
- Storage quota management prevents overruns
- Network-aware sync respects WiFi-only mode
- Demo app shows real-time sync in browser

## Next Steps

**IMMEDIATE ACTION**: Start Phase 1 by applying the database abstraction pattern to core-sync, following the same successful approach we used for core-library in TASK-204-2.

This is a **high-priority** task because:
1. Blocks WASM support for sync (critical feature)
2. Architectural inconsistency (core-library uses abstraction, core-sync doesn't)
3. Pattern already proven and documented
4. Estimated 1-2 days of focused work

**Recommendation**: Complete Phase 1 before moving to other WASM compatibility work.
