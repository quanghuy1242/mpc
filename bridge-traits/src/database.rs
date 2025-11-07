//! Database Abstraction Layer
//!
//! Provides a platform-agnostic trait for database operations to support
//! different storage backends across platforms:
//! - Native: SQLite via sqlx with native driver
//! - WebAssembly: SQLite via sql.js (sqlx with sqljs driver)
//!
//! ## Design Philosophy
//!
//! This trait abstracts all database operations behind a unified interface,
//! allowing the core library to work across platforms without hard dependencies
//! on platform-specific database drivers.
//!
//! ## Usage
//!
//! ```ignore
//! use bridge_traits::database::{DatabaseAdapter, DatabaseConfig};
//!
//! // Native implementation
//! #[cfg(not(target_arch = "wasm32"))]
//! let adapter = SqliteAdapter::new(pool).await?;
//!
//! // WASM implementation
//! #[cfg(target_arch = "wasm32")]
//! let adapter = WasmDbAdapter::new(config).await?;
//!
//! // Use the adapter
//! let track = adapter.find_track_by_id("track-id").await?;
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{error::Result, platform::PlatformSendSync};

// =============================================================================
// Configuration
// =============================================================================

/// Database configuration for cross-platform initialization
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database file path or connection string
    pub database_url: String,

    /// Minimum number of connections in the pool
    pub min_connections: u32,

    /// Maximum number of connections in the pool
    pub max_connections: u32,

    /// Maximum time to wait for a connection (seconds)
    pub acquire_timeout_secs: u64,

    /// Enable statement caching
    pub enable_cache: bool,

    /// Statement cache capacity
    pub cache_capacity: usize,
}

impl DatabaseConfig {
    /// Create a new database configuration with the given file path
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        let path = database_path.into();
        let database_url = format!("sqlite:{}", path.display());

        Self {
            database_url,
            min_connections: 1,
            max_connections: 5,
            acquire_timeout_secs: 30,
            enable_cache: true,
            cache_capacity: 100,
        }
    }

    /// Create a configuration for an in-memory database
    pub fn in_memory() -> Self {
        Self {
            database_url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 5,
            acquire_timeout_secs: 30,
            enable_cache: true,
            cache_capacity: 100,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self::in_memory()
    }
}

// =============================================================================
// Query Result Types
// =============================================================================

/// Represents a single row from a database query as a map of column names to values
pub type QueryRow = std::collections::HashMap<String, QueryValue>;

/// Represents a database value that can be null, integer, real, text, or blob
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl QueryValue {
    /// Convert to i64 if possible
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            QueryValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Convert to f64 if possible
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            QueryValue::Real(r) => Some(*r),
            QueryValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Convert to String if possible
    pub fn as_str(&self) -> Option<&str> {
        match self {
            QueryValue::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Convert to String (owned) if possible
    pub fn as_string(&self) -> Option<String> {
        match self {
            QueryValue::Text(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Convert to bytes if possible
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            QueryValue::Blob(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    /// Check if value is null
    pub fn is_null(&self) -> bool {
        matches!(self, QueryValue::Null)
    }
}

// =============================================================================
// Database Adapter Trait
// =============================================================================

/// Database adapter trait for cross-platform database operations
///
/// This trait abstracts all database operations needed by the core library,
/// allowing different implementations for native and WASM targets.
///
/// ## Thread Safety
///
/// Implementations must be thread-safe on native targets (`Send + Sync`). On WASM,
/// the trait relaxes those bounds automatically but implementations still need to
/// behave correctly in a single-threaded environment.
///
/// ## Error Handling
///
/// All methods return `Result<T>` using the `BridgeError` type for consistent
/// error handling across platforms.
///
/// ## Transaction Support
///
/// The trait provides transaction support through the `begin_transaction`,
/// `commit_transaction`, and `rollback_transaction` methods. Each transaction
/// is identified by a unique `TransactionId`.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DatabaseAdapter: PlatformSendSync {
    // =========================================================================
    // Connection Management
    // =========================================================================

    /// Initialize the database connection and run migrations
    ///
    /// This method should:
    /// 1. Establish database connection(s)
    /// 2. Configure the database (WAL mode, foreign keys, etc.)
    /// 3. Run pending migrations
    /// 4. Perform a health check
    async fn initialize(&mut self) -> Result<()>;

    /// Check if the database connection is healthy
    async fn health_check(&self) -> Result<()>;

    /// Close all database connections
    async fn close(&mut self) -> Result<()>;

    // =========================================================================
    // Raw Query Execution
    // =========================================================================

    /// Execute a raw SQL query and return rows
    ///
    /// # Arguments
    ///
    /// * `query` - SQL query string
    /// * `params` - Query parameters (positional)
    ///
    /// # Returns
    ///
    /// Vector of rows, where each row is a HashMap of column names to values
    ///
    /// # Safety
    ///
    /// This method should use parameterized queries to prevent SQL injection.
    /// Never concatenate user input directly into the query string.
    async fn query(&self, query: &str, params: &[QueryValue]) -> Result<Vec<QueryRow>>;

    /// Execute a SQL statement that doesn't return rows (INSERT, UPDATE, DELETE)
    ///
    /// # Arguments
    ///
    /// * `statement` - SQL statement string
    /// * `params` - Statement parameters (positional)
    ///
    /// # Returns
    ///
    /// Number of rows affected
    async fn execute(&self, statement: &str, params: &[QueryValue]) -> Result<u64>;

    /// Execute a query and return a single optional row
    ///
    /// This is a convenience method for queries that return 0 or 1 rows.
    async fn query_one_optional(
        &self,
        query: &str,
        params: &[QueryValue],
    ) -> Result<Option<QueryRow>>;

    /// Execute a query and return exactly one row
    ///
    /// Returns an error if no rows or more than one row is returned.
    async fn query_one(&self, query: &str, params: &[QueryValue]) -> Result<QueryRow>;

    // =========================================================================
    // Transaction Support
    // =========================================================================

    /// Begin a new database transaction
    ///
    /// Returns a transaction ID that must be used for subsequent operations
    /// within the transaction.
    async fn begin_transaction(&self) -> Result<TransactionId>;

    /// Commit a transaction
    ///
    /// # Arguments
    ///
    /// * `transaction_id` - The transaction to commit
    async fn commit_transaction(&self, transaction_id: TransactionId) -> Result<()>;

    /// Rollback a transaction
    ///
    /// # Arguments
    ///
    /// * `transaction_id` - The transaction to rollback
    async fn rollback_transaction(&self, transaction_id: TransactionId) -> Result<()>;

    /// Execute a query within a transaction
    async fn query_in_transaction(
        &self,
        transaction_id: TransactionId,
        query: &str,
        params: &[QueryValue],
    ) -> Result<Vec<QueryRow>>;

    /// Execute a statement within a transaction
    async fn execute_in_transaction(
        &self,
        transaction_id: TransactionId,
        statement: &str,
        params: &[QueryValue],
    ) -> Result<u64>;

    // =========================================================================
    // Batch Operations
    // =========================================================================

    /// Execute multiple statements in a batch (more efficient than individual executes)
    ///
    /// # Arguments
    ///
    /// * `statements` - Vector of (SQL statement, parameters) tuples
    ///
    /// # Returns
    ///
    /// Vector of row counts (one per statement)
    ///
    /// # Note
    ///
    /// This operation should be atomic - if any statement fails, all should be rolled back.
    async fn execute_batch(&self, statements: &[(&str, &[QueryValue])]) -> Result<Vec<u64>>;

    // =========================================================================
    // Migration Support
    // =========================================================================

    /// Get the current schema version
    async fn get_schema_version(&self) -> Result<i64>;

    /// Apply a migration
    ///
    /// # Arguments
    ///
    /// * `version` - Migration version number
    /// * `up_sql` - SQL to apply the migration
    async fn apply_migration(&self, version: i64, up_sql: &str) -> Result<()>;

    /// Check if a migration has been applied
    async fn is_migration_applied(&self, version: i64) -> Result<bool>;

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Get the last inserted row ID
    ///
    /// This is useful after INSERT statements to get the auto-generated ID.
    async fn last_insert_rowid(&self) -> Result<i64>;

    /// Get detailed statistics about the database
    async fn get_statistics(&self) -> Result<DatabaseStatistics>;
}

// =============================================================================
// Supporting Types
// =============================================================================

/// Unique identifier for a database transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(pub u64);

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStatistics {
    /// Total number of connections in the pool
    pub total_connections: u32,
    /// Number of idle connections
    pub idle_connections: u32,
    /// Number of active connections
    pub active_connections: u32,
    /// Database file size in bytes (if applicable)
    pub database_size_bytes: Option<u64>,
    /// Number of cached prepared statements
    pub cached_statements: usize,
}

// =============================================================================
// Helper Macros for Implementations
// =============================================================================

/// Helper macro to convert Rust types to QueryValue
#[macro_export]
macro_rules! query_value {
    ($val:expr, i64) => {
        $crate::database::QueryValue::Integer($val as i64)
    };
    ($val:expr, f64) => {
        $crate::database::QueryValue::Real($val as f64)
    };
    ($val:expr, String) => {
        $crate::database::QueryValue::Text($val.to_string())
    };
    ($val:expr, &str) => {
        $crate::database::QueryValue::Text($val.to_string())
    };
    ($val:expr, Vec<u8>) => {
        $crate::database::QueryValue::Blob($val)
    };
    (null) => {
        $crate::database::QueryValue::Null
    };
}

/// Helper macro to extract values from QueryRow
#[macro_export]
macro_rules! get_column {
    ($row:expr, $col:expr, i64) => {
        $row.get($col).and_then(|v| v.as_i64()).ok_or_else(|| {
            $crate::BridgeError::DatabaseError(format!("Missing or invalid i64 column: {}", $col))
        })?
    };
    ($row:expr, $col:expr, f64) => {
        $row.get($col).and_then(|v| v.as_f64()).ok_or_else(|| {
            $crate::BridgeError::DatabaseError(format!("Missing or invalid f64 column: {}", $col))
        })?
    };
    ($row:expr, $col:expr, String) => {
        $row.get($col).and_then(|v| v.as_string()).ok_or_else(|| {
            $crate::BridgeError::DatabaseError(format!(
                "Missing or invalid String column: {}",
                $col
            ))
        })?
    };
    ($row:expr, $col:expr, Option<String>) => {
        $row.get($col)
            .and_then(|v| if v.is_null() { None } else { v.as_string() })
    };
    ($row:expr, $col:expr, Option<i64>) => {
        $row.get($col)
            .and_then(|v| if v.is_null() { None } else { v.as_i64() })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_value_conversions() {
        let int_val = QueryValue::Integer(42);
        assert_eq!(int_val.as_i64(), Some(42));
        assert_eq!(int_val.as_f64(), Some(42.0));
        assert!(int_val.as_str().is_none());

        let text_val = QueryValue::Text("hello".to_string());
        assert_eq!(text_val.as_str(), Some("hello"));
        assert_eq!(text_val.as_string(), Some("hello".to_string()));
        assert!(text_val.as_i64().is_none());

        let null_val = QueryValue::Null;
        assert!(null_val.is_null());
        assert!(null_val.as_i64().is_none());
    }

    #[test]
    fn test_database_config_builder() {
        let config = DatabaseConfig::in_memory();
        assert_eq!(config.database_url, "sqlite::memory:");
        assert_eq!(config.min_connections, 1);
        assert_eq!(config.max_connections, 5);
        assert_eq!(config.acquire_timeout_secs, 30);
        assert!(config.enable_cache);
        assert_eq!(config.cache_capacity, 100);
    }

    #[test]
    fn test_database_config_from_path() {
        let config = DatabaseConfig::new("test.db");
        assert!(config.database_url.contains("test.db"));
    }
}
