//! # Database Connection Pool Module
//!
//! Provides SQLite connection pooling with optimal configuration for the music library.
//!
//! ## Features
//!
//! - **WAL Mode**: Enabled for better concurrency (multiple readers, one writer)
//! - **Connection Pooling**: Configurable min/max connections with timeouts
//! - **Statement Caching**: Automatic prepared statement caching
//! - **Foreign Keys**: Enforced for referential integrity
//! - **Automatic Migrations**: Runs on initialization
//! - **Health Checks**: Connection validation
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_library::db::{DatabaseConfig, create_pool};
//!
//! // Create a connection pool
//! let config = DatabaseConfig::new("sqlite:music.db");
//! let pool = create_pool(config).await?;
//!
//! // Use the pool for queries
//! let track = sqlx::query!("SELECT * FROM tracks WHERE id = ?", track_id)
//!     .fetch_one(&pool)
//!     .await?;
//! ```
//!
//! ## Testing
//!
//! For tests, use in-memory databases:
//!
//! ```rust,ignore
//! let pool = create_test_pool().await?;
//! ```

use crate::{LibraryError, Result};
#[cfg(not(target_arch = "wasm32"))]
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
#[cfg(not(target_arch = "wasm32"))]
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Database configuration for SQLite connection pool
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database file path or `:memory:` for in-memory database
    pub database_url: String,

    /// Minimum number of connections in the pool
    pub min_connections: u32,

    /// Maximum number of connections in the pool
    pub max_connections: u32,

    /// Maximum time to wait for a connection from the pool
    pub acquire_timeout: Duration,

    /// Maximum lifetime of a connection
    pub max_lifetime: Option<Duration>,

    /// Maximum idle time for a connection before being closed
    pub idle_timeout: Option<Duration>,

    /// Enable statement caching (number of statements to cache)
    pub statement_cache_capacity: usize,
}

impl DatabaseConfig {
    /// Create a new database configuration with the given file path
    ///
    /// # Arguments
    ///
    /// * `database_path` - Path to the SQLite database file
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let config = DatabaseConfig::new("sqlite:music.db");
    /// ```
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        let path = database_path.into();
        let database_url = format!("sqlite:{}", path.display());

        Self {
            database_url,
            min_connections: 1,
            max_connections: 5,
            acquire_timeout: Duration::from_secs(30),
            max_lifetime: Some(Duration::from_secs(1800)), // 30 minutes
            idle_timeout: Some(Duration::from_secs(600)),  // 10 minutes
            statement_cache_capacity: 100,
        }
    }

    /// Create a configuration for an in-memory database (useful for testing)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let config = DatabaseConfig::in_memory();
    /// let pool = create_pool(config).await?;
    /// ```
    pub fn in_memory() -> Self {
        Self {
            database_url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 5,
            acquire_timeout: Duration::from_secs(30),
            max_lifetime: None,
            idle_timeout: None,
            statement_cache_capacity: 100,
        }
    }

    /// Set the minimum number of connections
    pub fn min_connections(mut self, min: u32) -> Self {
        self.min_connections = min;
        self
    }

    /// Set the maximum number of connections
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Set the connection acquire timeout
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    /// Set the maximum connection lifetime
    pub fn max_lifetime(mut self, lifetime: Option<Duration>) -> Self {
        self.max_lifetime = lifetime;
        self
    }

    /// Set the idle timeout
    pub fn idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the statement cache capacity
    pub fn statement_cache_capacity(mut self, capacity: usize) -> Self {
        self.statement_cache_capacity = capacity;
        self
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self::in_memory()
    }
}

/// Create a configured SQLite connection pool
///
/// This function:
/// 1. Configures SQLite connection options (WAL mode, foreign keys, etc.)
/// 2. Creates a connection pool with the specified configuration
/// 3. Runs database migrations
/// 4. Performs a health check
///
/// # Arguments
///
/// * `config` - Database configuration
///
/// # Returns
///
/// A configured connection pool ready for use
///
/// # Errors
///
/// Returns an error if:
/// - The database file cannot be accessed
/// - Connection pool creation fails
/// - Migrations fail
/// - Health check fails
///
/// # Examples
///
/// ```rust,ignore
/// use core_library::db::{DatabaseConfig, create_pool};
///
/// let config = DatabaseConfig::new("music.db")
///     .max_connections(10)
///     .acquire_timeout(Duration::from_secs(60));
///
/// let pool = create_pool(config).await?;
/// ```
pub async fn create_pool(config: DatabaseConfig) -> Result<Pool<Sqlite>> {
    info!(
        database_url = %config.database_url,
        min_connections = config.min_connections,
        max_connections = config.max_connections,
        "Creating database connection pool"
    );

    // Parse the database URL and configure SQLite options
    let mut connect_options =
        SqliteConnectOptions::from_str(&config.database_url).map_err(LibraryError::Database)?;

    // Configure SQLite connection options
    connect_options = connect_options
        // Enable WAL mode for better concurrency
        .journal_mode(SqliteJournalMode::Wal)
        // NORMAL synchronous mode for good balance of safety and speed
        .synchronous(SqliteSynchronous::Normal)
        // Enable foreign key constraints
        .foreign_keys(true)
        // Create database if it doesn't exist
        .create_if_missing(true)
        // Optimize cache size (64MB)
        .pragma("cache_size", "-64000")
        // Use memory-mapped I/O for better performance
        .pragma("mmap_size", "268435456") // 256MB
        // Incremental auto-vacuum to prevent fragmentation
        .pragma("auto_vacuum", "INCREMENTAL")
        // Statement cache capacity
        .statement_cache_capacity(config.statement_cache_capacity);

    debug!("SQLite connection options configured");

    // Create the connection pool
    let pool = SqlitePoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout)
        .max_lifetime(config.max_lifetime)
        .idle_timeout(config.idle_timeout)
        .connect_with(connect_options)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to create connection pool");
            LibraryError::Database(e)
        })?;

    info!(
        connections = pool.size(),
        "Database connection pool created successfully"
    );

    // Run migrations
    run_migrations(&pool).await?;

    // Perform health check
    health_check(&pool).await?;

    Ok(pool)
}

/// Create a connection pool for testing with in-memory database
///
/// This is a convenience function that creates an in-memory database
/// with migrations already applied.
///
/// # Examples
///
/// ```rust,ignore
/// #[core_async::test]
/// async fn test_something() {
///     let pool = create_test_pool().await.unwrap();
///     // Use pool for testing
/// }
/// ```
pub async fn create_test_pool() -> Result<Pool<Sqlite>> {
    let config = DatabaseConfig::in_memory();
    create_pool(config).await
}

/// Insert a test provider into the database (for testing only)
///
/// This helper function inserts a default test provider into the database
/// to satisfy foreign key constraints when testing tracks and other entities.
///
/// # Arguments
///
/// * `pool` - Database connection pool
///
/// # Examples
///
/// ```rust,ignore
/// #[core_async::test]
/// async fn test_something() {
///     let pool = create_test_pool().await.unwrap();
///     insert_test_provider(&pool).await;
///     // Now you can create tracks with provider_id = "test-provider"
/// }
/// ```
pub async fn insert_test_provider(pool: &Pool<Sqlite>) {
    // Insert test provider if it doesn't already exist
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO providers (id, type, display_name, profile_id, created_at)
        VALUES ('test-provider', 'GoogleDrive', 'Test Provider', 'test-profile', 0)
        "#,
    )
    .execute(pool)
    .await
    .expect("Failed to insert test provider");
}

/// Run database migrations
///
/// This function applies all pending migrations from the `migrations/` directory.
/// Migrations are embedded in the binary at compile time using `sqlx::migrate!()`.
///
/// # Arguments
///
/// * `pool` - Database connection pool
///
/// # Errors
///
/// Returns an error if migrations fail to apply
async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
    info!("Running database migrations");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| {
            warn!(error = %e, "Migration failed");
            LibraryError::Migration(e.to_string())
        })?;

    info!("Database migrations completed successfully");
    Ok(())
}

/// Perform a health check on the connection pool
///
/// This executes a simple query to verify the database is accessible
/// and the pool is functioning correctly.
///
/// # Arguments
///
/// * `pool` - Database connection pool
///
/// # Errors
///
/// Returns an error if the health check query fails
async fn health_check(pool: &Pool<Sqlite>) -> Result<()> {
    debug!("Performing database health check");

    sqlx::query("SELECT 1").fetch_one(pool).await.map_err(|e| {
        warn!(error = %e, "Database health check failed");
        LibraryError::Database(e)
    })?;

    debug!("Database health check passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core_async::test]
    async fn test_create_in_memory_pool() {
        let config = DatabaseConfig::in_memory();
        let pool = create_pool(config).await;
        assert!(pool.is_ok(), "Should create in-memory pool successfully");
    }

    #[core_async::test]
    async fn test_create_test_pool() {
        let pool = create_test_pool().await;
        assert!(pool.is_ok(), "Should create test pool successfully");
    }

    #[core_async::test]
    async fn test_health_check() {
        let pool = create_test_pool().await.unwrap();
        let result = health_check(&pool).await;
        assert!(result.is_ok(), "Health check should pass");
    }

    #[core_async::test]
    async fn test_database_config_builder() {
        let config = DatabaseConfig::in_memory()
            .min_connections(2)
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(60))
            .statement_cache_capacity(200);

        assert_eq!(config.min_connections, 2);
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.acquire_timeout, Duration::from_secs(60));
        assert_eq!(config.statement_cache_capacity, 200);
    }

    #[core_async::test]
    async fn test_concurrent_queries() {
        let pool = create_test_pool().await.unwrap();

        // Run multiple queries concurrently
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let pool = pool.clone();
                core_async::task::spawn(async move {
                    sqlx::query("SELECT 1").fetch_one(&pool).await.unwrap();
                })
            })
            .collect();

        // Wait for all queries to complete
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[core_async::test]
    async fn test_foreign_keys_enabled() {
        let pool = create_test_pool().await.unwrap();

        // Check if foreign keys are enabled
        let result: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(result.0, 1, "Foreign keys should be enabled");
    }

    #[core_async::test]
    async fn test_wal_mode_enabled() {
        let pool = create_test_pool().await.unwrap();

        // Check journal mode
        // Note: In-memory databases use "memory" mode instead of WAL
        let result: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();

        let mode = result.0.to_lowercase();
        assert!(
            mode == "wal" || mode == "memory",
            "Journal mode should be WAL or memory (for in-memory databases), got: {}",
            mode
        );
    }

    #[core_async::test]
    async fn test_migrations_create_tables() {
        let pool = create_test_pool().await.unwrap();

        // Check if the providers table exists
        let result: (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='providers'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, 1, "Providers table should exist");

        // Check if the tracks table exists
        let result: (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tracks'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result.0, 1, "Tracks table should exist");
    }
}
