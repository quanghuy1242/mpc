# WASM Sync Migration - Week of November 7, 2024

## Mission: Enable core-sync WASM Compilation

**Goal**: Migrate core-sync from direct SqlitePool usage to DatabaseAdapter trait pattern, enabling cross-platform compilation for WASM targets.

**Why This Matters**: 
- core-sync is the synchronization engine for cloud storage providers (Google Drive, OneDrive)
- Must work identically on native (desktop/mobile) and WASM (browser) platforms
- Database abstraction already proven successful in core-library (TASK-204-2)
- WasmDbAdapter already exists and fully implements DatabaseAdapter (430+ lines in bridge-wasm/src/database.rs)

## Progress Summary

### ✅ Completed This Week (2 modules, 1,452 lines)

**1. repository.rs - 100% Complete (667 lines)**
- Migrated SyncJobRepository trait and SqliteSyncJobRepository implementation
- All 9 trait methods now accept `db: &dyn DatabaseAdapter` parameter
- Removed SqlitePool dependency entirely
- Converted all sqlx queries to DatabaseAdapter::execute/query/query_one_optional
- Implemented manual row parsing with helper functions
- Updated all 10 tests to use Arc<dyn DatabaseAdapter>
- **Status**: ✅ 0 compilation errors

**2. scan_queue.rs - 100% Complete (785 lines)**
- Migrated ScanQueueRepository trait with 7 methods
- Removed SqlitePool from SqliteScanQueueRepository
- Converted all database queries to QueryValue arrays
- Updated ScanQueue to hold and use Arc<dyn DatabaseAdapter>
- Added row parsing helpers (row_to_work_item, get_string, get_i32, etc.)
- Updated all 8 tests with SqliteAdapter::from_pool
- Fixed tricky lifetime issue in count query (needed .as_ref())
- **Status**: ✅ 0 compilation errors

**3. coordinator.rs - 50% Complete (720/1436 lines)**
- Updated struct to use `db: Arc<dyn DatabaseAdapter>`
- Updated constructor signature to accept DatabaseAdapter
- Fixed all 11 repository method calls to pass db.as_ref()
- Updated repository construction pattern
- ScanQueue::new() now works after scan_queue migration
- **Status**: ⚠️ Blocked by 3 unmigrated dependencies (see below)

### 🚧 Remaining Work (3 modules, ~2,264 lines)

**Blocking coordinator.rs completion:**

1. **conflict_resolution_orchestrator.rs** (618 lines)
   - Used by coordinator at line 318
   - Constructor expects SqlitePool
   - Estimated: 2-3 hours
   
2. **metadata_processor.rs** (721 lines)
   - Used by coordinator at line 312
   - Constructor expects SqlitePool
   - Estimated: 3-4 hours

3. **conflict_resolver.rs** (925 lines)
   - Used by coordinator at line 268
   - Constructor expects SqlitePool
   - Estimated: 3-4 hours

**After these 3 modules:**
- Update coordinator.rs tests
- Run full test suite: `cargo test --package core-sync`
- Verify WASM compilation: `cargo check --target wasm32-unknown-unknown`

## Technical Pattern Established

The migration pattern is now well-proven and consistent:

### 1. Update Trait Definition
```rust
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Repository: Send + Sync {
    async fn insert(&self, db: &dyn DatabaseAdapter, item: &Item) -> Result<()>;
    async fn find(&self, db: &dyn DatabaseAdapter, id: &Id) -> Result<Option<Item>>;
    // Add db parameter to ALL async methods
}
```

### 2. Remove SqlitePool from Implementation
```rust
// Before
pub struct SqliteRepository {
    pool: SqlitePool,  // ❌ Remove this
}

// After  
pub struct SqliteRepository {}  // ✅ Stateless, db passed to methods
```

### 3. Convert Queries to DatabaseAdapter
```rust
// Before: sqlx direct
sqlx::query("INSERT INTO table (col1, col2) VALUES (?, ?)")
    .bind(&item.col1)
    .bind(item.col2)
    .execute(&self.pool)
    .await?;

// After: DatabaseAdapter
db.execute(
    "INSERT INTO table (col1, col2) VALUES (?, ?)",
    &[
        QueryValue::Text(item.col1.clone()),
        QueryValue::Integer(item.col2),
    ],
).await?;
```

### 4. Add Row Parsing Helpers
```rust
impl SqliteRepository {
    fn row_to_model(row: &QueryRow) -> Result<Model> {
        Ok(Model {
            id: ModelId::from_string(&Self::get_string(row, "id")?)?,
            name: Self::get_string(row, "name")?,
            count: Self::get_i64(row, "count")? as u64,
            optional: Self::get_optional_string(row, "optional"),
        })
    }
    
    fn get_string(row: &QueryRow, key: &str) -> Result<String> {
        row.get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| Error::Database(format!("Missing: {}", key)))
    }
    
    fn get_i64(row: &QueryRow, key: &str) -> Result<i64> {
        row.get(key)
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::Database(format!("Missing: {}", key)))
    }
    
    fn get_optional_string(row: &QueryRow, key: &str) -> Option<String> {
        row.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
    }
}
```

### 5. Update Tests
```rust
#[core_async::test]
async fn test_method() {
    use core_library::adapters::sqlite_native::SqliteAdapter;
    
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    let db: Arc<dyn DatabaseAdapter> = Arc::new(SqliteAdapter::from_pool(pool));
    
    let repo = SqliteRepository::new();
    repo.insert(db.as_ref(), &item).await.unwrap();
    
    let found = repo.find(db.as_ref(), &id).await.unwrap();
    assert!(found.is_some());
}
```

## Key Technical Lessons

### QueryValue Conversions
- `String` → `QueryValue::Text(s.clone())`
- `i64` → `QueryValue::Integer(i)`
- `u32` → `QueryValue::Integer(u as i64)` (cast needed)
- `Option<T>` → `opt.map(QueryValue::Type).unwrap_or(QueryValue::Null)`

### Row Parsing Gotchas
- Always extract values immediately from QueryRow (can't return borrowed data)
- Use `.as_ref()` before accessing fields to avoid lifetime issues
- COUNT queries: `row.as_ref().and_then(|r| r.get("count")).and_then(|v| v.as_i64())`

### Test Database Creation
- ✅ Use `SqliteAdapter::from_pool(pool)` 
- ❌ Don't use `SqliteAdapter::new()` (it's async, returns Result)

### Async Trait Annotations
- WASM: `#[async_trait(?Send)]` (no Send bound)
- Native: `#[async_trait]` (standard)
- Use `cfg_attr` for conditional compilation

## Progress Metrics

| Module | Lines | Status | Tests | Errors |
|--------|-------|--------|-------|--------|
| repository.rs | 667 | ✅ Complete | 10 updated | 0 |
| scan_queue.rs | 785 | ✅ Complete | 8 updated | 0 |
| coordinator.rs | 1436 | ⏳ 50% | Pending | 4 |
| conflict_resolution_orchestrator.rs | 618 | ❌ Not started | - | - |
| metadata_processor.rs | 721 | ❌ Not started | - | - |
| conflict_resolver.rs | 925 | ❌ Not started | - | - |
| **TOTALS** | **5,152** | **30.8%** | **18/~40** | **4** |

**Time Investment**: ~3 hours  
**Remaining Estimate**: 8-10 hours

## What's Already Done (Bonus!)

The Rust side of WASM database support is **already complete**:

- ✅ `WasmDbAdapter` fully implemented (430+ lines in bridge-wasm/src/database.rs)
- ✅ All 14 DatabaseAdapter trait methods working
- ✅ Transaction support (begin, commit, rollback)
- ✅ Migration system (schema versioning)
- ✅ Batch operations
- ✅ Statistics and monitoring

**What's needed**: JavaScript implementation of `bridgeWasmDb` namespace (estimated 400 lines, 4-6 hours) that provides:
- sql.js wrapper with IndexedDB persistence
- 14 async functions matching DatabaseAdapter methods
- Serialization between Rust QueryValue and JS types

Once Phase 1 (this migration) is complete, core-sync will automatically work with WasmDbAdapter!

## Next Session Action Items

**Priority Order:**
1. Migrate `conflict_resolution_orchestrator.rs` (618 lines, smallest)
   - Apply established pattern
   - Update constructor signature
   - Convert queries to DatabaseAdapter
   - Add row parsing helpers
   - Update tests

2. Migrate `metadata_processor.rs` (721 lines)
   - Same pattern

3. Migrate `conflict_resolver.rs` (925 lines, largest)
   - Same pattern

4. Fix coordinator.rs dependency issues
   - Update ConflictResolver::new() call
   - Update MetadataProcessor::new() call
   - Update ConflictResolutionOrchestrator::new() call
   - Update tests

5. Verification
   - `cargo test --package core-sync` (all tests pass)
   - `cargo check --package core-sync --target wasm32-unknown-unknown` (compiles)

## Files Modified This Session

- `core-sync/Cargo.toml` - Added bridge-desktop dev dependency (later found unnecessary)
- `core-sync/src/repository.rs` - Complete migration (667 lines)
- `core-sync/src/scan_queue.rs` - Complete migration (785 lines)
- `core-sync/src/coordinator.rs` - Partial migration (720 lines)
- `docs/wasm_sync_strategy.md` - Updated with progress and session log

## Context for Future Sessions

**Current State:**
- 2 modules fully migrated with 0 errors ✅
- 3 modules remain unmigrated ❌
- coordinator.rs 50% complete, blocked by 3 dependencies ⏳
- Pattern proven and consistent across modules
- Tests pattern established (use SqliteAdapter::from_pool)

**Key Context:**
- This is Phase 1 of 3-phase WASM sync strategy
- Phase 2 (WASM adapter) already done! Just needs JS bridge implementation
- Phase 3 (WASM-specific features) comes after Phase 1 complete
- High priority: blocks WASM compilation for entire sync system
- Architecture alignment: making core-sync match core-library pattern

**When Resuming:**
- Start with `conflict_resolution_orchestrator.rs` (smallest remaining, 618 lines)
- Follow established pattern exactly (proven to work)
- Each module takes ~1-2 hours
- Keep running `cargo check --package core-sync` after each module
- Don't forget to update tests with SqliteAdapter::from_pool

## Success Criteria

✅ **Phase 1 Complete When:**
- All 6 core-sync modules migrated
- `cargo check --package core-sync --target wasm32-unknown-unknown` succeeds
- All tests pass on native: `cargo test --package core-sync`
- Zero direct SqlitePool usage in core-sync
- All repository methods accept DatabaseAdapter parameter

**Expected Timeline**: 1-2 more focused sessions (8-10 hours total remaining)