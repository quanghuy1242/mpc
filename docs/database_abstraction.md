# Database Abstraction Layer

## Overview

The Music Platform Core uses a database abstraction layer to enable cross-platform compatibility while maintaining a consistent API for data access. This abstraction allows the core library to work seamlessly across native platforms (desktop, iOS, Android) and WebAssembly (web browsers) without requiring platform-specific code in the business logic layer.

## Architecture

### Component Structure

```
┌─────────────────────────────────────────────────────────────┐
│                     Core Library                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │         Repositories (TrackRepository, etc.)           │ │
│  └────────────────────────────────────────────────────────┘ │
│                           ↓                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │     DatabaseAdapter Trait (bridge-traits)              │ │
│  │  • query(), execute()                                  │ │
│  │  • Transaction support                                 │ │
│  │  • Migration management                                │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                            ↓
        ┌──────────────────────────────────────┐
        │                                       │
┌───────▼─────────┐               ┌────────────▼────────────┐
│ SqliteAdapter   │               │  WasmDbAdapter          │
│ (Native)        │               │  (WebAssembly)          │
│                 │               │                         │
│ • Uses sqlx     │               │ • Uses sqlx with        │
│ • Native SQLite │               │   sql.js driver         │
│ • File-based    │               │ • IndexedDB backend     │
└─────────────────┘               └─────────────────────────┘
```

## Components

### 1. DatabaseAdapter Trait (`bridge-traits/src/database.rs`)

The `DatabaseAdapter` trait defines the contract for all database operations:

```rust
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    // Connection management
    async fn initialize(&mut self) -> Result<()>;
    async fn health_check(&self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    
    // Query execution
    async fn query(&self, query: &str, params: &[QueryValue]) -> Result<Vec<QueryRow>>;
    async fn execute(&self, statement: &str, params: &[QueryValue]) -> Result<u64>;
    
    // Transactions
    async fn begin_transaction(&self) -> Result<TransactionId>;
    async fn commit_transaction(&self, transaction_id: TransactionId) -> Result<()>;
    async fn rollback_transaction(&self, transaction_id: TransactionId) -> Result<()>;
    
    // Migrations
    async fn apply_migration(&self, version: i64, up_sql: &str) -> Result<()>;
    async fn get_schema_version(&self) -> Result<i64>;
    
    // And more...
}
```

### 2. SqliteAdapter (`core-library/src/adapters/sqlite_native.rs`)

Native SQLite implementation for desktop, iOS, and Android:

**Features:**
- Uses `sqlx` with the native SQLite driver
- Connection pooling for concurrent access
- WAL mode for better concurrency
- Foreign key enforcement
- Automatic migrations
- Prepared statement caching

**Configuration:**

```rust
use core_library::adapters::SqliteAdapter;
use bridge_traits::database::DatabaseConfig;

// Create configuration
let config = DatabaseConfig::new("music.db")
    .max_connections(10)
    .acquire_timeout_secs(30);

// Create and initialize adapter
let mut adapter = SqliteAdapter::new(config).await?;
adapter.initialize().await?;
```

### 3. Query Value Types

The abstraction uses platform-agnostic value types:

```rust
pub enum QueryValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}
```

Results are returned as `QueryRow` (a HashMap of column names to values):

```rust
pub type QueryRow = HashMap<String, QueryValue>;
```

## Usage Examples

### Basic Query

```rust
use bridge_traits::database::{DatabaseAdapter, QueryValue};

// Execute a query
let params = vec![QueryValue::Integer(42)];
let rows = adapter.query("SELECT * FROM tracks WHERE id = ?", &params).await?;

// Access results
for row in rows {
    let id = row.get("id").and_then(|v| v.as_i64());
    let title = row.get("title").and_then(|v| v.as_str());
    println!("Track {}: {}", id.unwrap(), title.unwrap());
}
```

### Insert/Update/Delete

```rust
let params = vec![
    QueryValue::Text("My Song".to_string()),
    QueryValue::Integer(180000), // duration in ms
];

let rows_affected = adapter.execute(
    "INSERT INTO tracks (title, duration_ms) VALUES (?, ?)",
    &params
).await?;
```

### Transactions

```rust
// Begin transaction
let tx_id = adapter.begin_transaction().await?;

// Execute statements in transaction
adapter.execute_in_transaction(
    tx_id,
    "INSERT INTO tracks (title) VALUES (?)",
    &[QueryValue::Text("Song 1".to_string())]
).await?;

adapter.execute_in_transaction(
    tx_id,
    "INSERT INTO tracks (title) VALUES (?)",
    &[QueryValue::Text("Song 2".to_string())]
).await?;

// Commit or rollback
adapter.commit_transaction(tx_id).await?;
// OR
adapter.rollback_transaction(tx_id).await?;
```

### Using with Repositories

Repositories should accept a reference to the adapter:

```rust
pub struct SqliteTrackRepository {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteTrackRepository {
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }
}

#[async_trait]
impl TrackRepository for SqliteTrackRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>> {
        let params = vec![QueryValue::Text(id.to_string())];
        let row = self.adapter.query_one_optional(
            "SELECT * FROM tracks WHERE id = ?",
            &params
        ).await?;
        
        // Map row to Track model
        row.map(|r| self.row_to_track(&r)).transpose()
    }
}
```

## Current Limitations

### Transaction Support

The current `SqliteAdapter` transaction implementation has a known limitation with connection pooling:

**Issue:** SQLite transactions are connection-specific, but our implementation uses a connection pool where different operations might acquire different connections.

**Current Behavior:** 
- `begin_transaction()`, `commit_transaction()`, and `rollback_transaction()` work correctly if they happen to use the same connection from the pool
- This is often the case for simple scenarios but not guaranteed

**Workarounds:**
1. For simple batch operations, use `execute_batch()` which executes statements sequentially
2. For complex transactions, consider acquiring a dedicated connection for the transaction lifetime
3. Use repository-level transactional methods that handle this internally

**Future Improvements:**
- Implement connection reservation system for transaction duration
- Use `sqlx::Transaction` directly for guaranteed connection isolation
- Add `begin_transaction_with_connection()` that returns a dedicated connection guard

### Migration System

Currently, the adapter relies on `sqlx::migrate!()` macro which:
- Embeds migrations at compile time
- Requires migrations to be in the `migrations/` directory
- Works great for native platforms

For WASM, we'll need to:
- Bundle migrations differently (possibly as embedded strings)
- Implement custom migration runner
- Handle schema version tracking in the WASM database

## Future: WASM Implementation

The WASM implementation (`WasmDbAdapter`) will be created in the `bridge-wasm` crate:

**Planned Features:**
- Use `sqlx` with `sql.js` driver (SQLite compiled to WebAssembly)
- Store data in IndexedDB for persistence
- Implement same `DatabaseAdapter` interface
- Handle migrations via bundled SQL strings

**Challenges:**
- Size constraints (sql.js is ~700KB)
- Performance (slower than native)
- Browser storage limits
- No file system access

## Testing

### Unit Tests

The `SqliteAdapter` includes comprehensive unit tests:

```bash
cargo test --package core-library --lib adapters::sqlite_native
```

Tests cover:
- ✅ Adapter creation and initialization
- ✅ Basic query execution
- ✅ Statement execution (INSERT, UPDATE, DELETE)
- ✅ Transaction support
- ✅ Batch operations
- ✅ Statistics retrieval

### Integration Tests

Integration tests should verify the adapter works correctly with repositories:

```rust
#[core_async::test]
async fn test_repository_with_adapter() {
    let config = DatabaseConfig::in_memory();
    let mut adapter = SqliteAdapter::new(config).await.unwrap();
    adapter.initialize().await.unwrap();
    
    let repo = SqliteTrackRepository::new(Arc::new(adapter));
    // Test repository operations...
}
```

## Migration Guide

### For Existing Code

To migrate existing code that uses `Pool<Sqlite>` directly:

**Before:**
```rust
pub struct SqliteTrackRepository {
    pool: SqlitePool,
}

impl SqliteTrackRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>> {
        sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }
}
```

**After:**
```rust
pub struct SqliteTrackRepository {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteTrackRepository {
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }
    
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>> {
        let params = vec![QueryValue::Text(id.to_string())];
        let row = self.adapter.query_one_optional(
            "SELECT * FROM tracks WHERE id = ?",
            &params
        ).await?;
        
        row.map(|r| Track::from_query_row(&r)).transpose()
    }
}
```

## Best Practices

### 1. Use Parameterized Queries

**Always** use parameterized queries to prevent SQL injection:

```rust
// ✅ GOOD
let params = vec![QueryValue::Text(user_input)];
adapter.query("SELECT * FROM tracks WHERE title = ?", &params).await?;

// ❌ BAD
let query = format!("SELECT * FROM tracks WHERE title = '{}'", user_input);
adapter.query(&query, &[]).await?;
```

### 2. Handle Null Values

Always check for null before unwrapping:

```rust
let title = row.get("title")
    .and_then(|v| v.as_str())
    .ok_or_else(|| LibraryError::InvalidData("Missing title".into()))?;
```

### 3. Use Appropriate Value Types

Match the database column type:

```rust
// For INTEGER columns
QueryValue::Integer(duration_ms)

// For TEXT columns
QueryValue::Text(title.to_string())

// For REAL columns
QueryValue::Real(rating)

// For BLOB columns
QueryValue::Blob(artwork_data)

// For NULL values
QueryValue::Null
```

### 4. Batch Operations for Performance

When inserting/updating multiple rows, use batch operations:

```rust
let statements: Vec<(&str, &[QueryValue])> = tracks
    .iter()
    .map(|t| (
        "INSERT INTO tracks (id, title) VALUES (?, ?)",
        vec![QueryValue::Text(t.id.clone()), QueryValue::Text(t.title.clone())]
    ))
    .collect();

adapter.execute_batch(&statements).await?;
```

### 5. Implement Model Converters

Create helper methods to convert between database rows and domain models:

```rust
impl Track {
    fn from_query_row(row: &QueryRow) -> Result<Self> {
        Ok(Track {
            id: get_column!(row, "id", String),
            title: get_column!(row, "title", String),
            duration_ms: get_column!(row, "duration_ms", i64),
            album_id: get_column!(row, "album_id", Option<String>),
            // ... more fields
        })
    }
    
    fn to_query_params(&self) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(self.id.clone()),
            QueryValue::Text(self.title.clone()),
            QueryValue::Integer(self.duration_ms),
            self.album_id.as_ref()
                .map(|id| QueryValue::Text(id.clone()))
                .unwrap_or(QueryValue::Null),
        ]
    }
}
```

## Performance Considerations

### Connection Pooling

Configure pool sizes based on your workload:

```rust
// For desktop apps with moderate concurrency
let config = DatabaseConfig::new("music.db")
    .min_connections(2)
    .max_connections(10);

// For servers with high concurrency
let config = DatabaseConfig::new("music.db")
    .min_connections(5)
    .max_connections(50);
```

### Statement Caching

Enable caching for frequently used queries:

```rust
let config = DatabaseConfig::new("music.db")
    .enable_cache(true)
    .cache_capacity(200);
```

### Batch Operations

Use batch operations instead of individual inserts:

```rust
// ✅ FAST: One batch operation
adapter.execute_batch(&statements).await?;

// ❌ SLOW: Multiple individual operations
for statement in statements {
    adapter.execute(statement.0, statement.1).await?;
}
```

## Troubleshooting

### "No such table" errors

Ensure migrations have been run:

```rust
adapter.initialize().await?; // This runs migrations
```

### Connection pool timeout

Increase timeout or pool size:

```rust
let config = DatabaseConfig::new("music.db")
    .max_connections(20)
    .acquire_timeout_secs(60);
```

### Transaction commit failures

See [Transaction Support Limitations](#transaction-support) above. Use `execute_batch()` for simple atomic operations.

## Related Documentation

- [Core Architecture](./core_architecture.md) - Overall system architecture
- [Task List](./ai_task_list.md) - Implementation task tracking
- [Immediate TODO](./immediate_todo.md) - Current priority tasks

## Changelog

### Phase 0.1 - Initial Implementation
- ✅ Created `DatabaseAdapter` trait in `bridge-traits`
- ✅ Implemented `SqliteAdapter` for native platforms
- ✅ Added comprehensive tests
- ✅ Updated `core-library` to use abstraction
- 📋 TODO: Implement `WasmDbAdapter` for WebAssembly
- 📋 TODO: Migrate all repositories to use adapter
- 📋 TODO: Implement dedicated transaction connections
