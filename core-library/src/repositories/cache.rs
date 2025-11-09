//! Database repository for cache metadata
//!
//! Stores cache metadata in SQLite using the DatabaseAdapter trait for cross-platform compatibility.

use crate::error::{LibraryError, Result};
use crate::models::{CachedTrack, CacheStats, CacheStatus, TrackId};
use crate::repositories::PlatformArc;
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use tracing::{debug, error, instrument};

// Schema is now managed via migrations/003_add_cache_metadata.sql

/// Repository trait for cache metadata operations.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait CacheMetadataRepository {
    /// Initialize the repository (create tables if needed).
    async fn initialize(&self) -> Result<()>;

    /// Insert a new cache entry.
    async fn insert(&self, track: &CachedTrack) -> Result<()>;

    /// Update an existing cache entry.
    async fn update(&self, track: &CachedTrack) -> Result<()>;

    /// Find a cache entry by track ID.
    async fn find_by_track_id(&self, track_id: &TrackId) -> Result<Option<CachedTrack>>;

    /// Find all cache entries with a specific status.
    async fn find_by_status(&self, status: CacheStatus) -> Result<Vec<CachedTrack>>;

    /// Find all cached tracks sorted by last accessed time (for LRU eviction).
    async fn find_for_lru_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>>;

    /// Find all cached tracks sorted by play count (for LFU eviction).
    async fn find_for_lfu_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>>;

    /// Find all cached tracks sorted by cached time (for FIFO eviction).
    async fn find_for_fifo_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>>;

    /// Find all cached tracks sorted by size descending (for largest-first eviction).
    async fn find_for_largest_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>>;

    /// Delete a cache entry by track ID.
    async fn delete(&self, track_id: &TrackId) -> Result<()>;

    /// Get cache statistics.
    async fn get_stats(&self) -> Result<CacheStats>;

    /// Get all cached tracks.
    async fn find_all(&self) -> Result<Vec<CachedTrack>>;
}

/// SQLite implementation of CacheMetadataRepository.
pub struct SqliteCacheMetadataRepository {
    db: PlatformArc<dyn DatabaseAdapter>,
}

impl SqliteCacheMetadataRepository {
    /// Create a new repository with the given database adapter.
    pub fn new(db: PlatformArc<dyn DatabaseAdapter>) -> Self {
        Self { db }
    }

    /// Convert a QueryRow to CachedTrack.
    fn row_to_cached_track(row: &QueryRow) -> Result<CachedTrack> {
        let track_id_str = get_string(row, "track_id")?;
        let track_id = TrackId::from_string(&track_id_str)
            .map_err(|e| LibraryError::CacheError(format!("Invalid track_id: {}", e)))?;

        let status_str = get_string(row, "status")?;
        let status = Self::parse_status(&status_str)?;

        Ok(CachedTrack {
            track_id,
            cache_path: get_string(row, "cache_path")?,
            file_size: get_i64(row, "file_size")? as u64,
            cached_size: get_i64(row, "cached_size")? as u64,
            content_hash: get_string(row, "content_hash")?,
            encrypted: get_i64(row, "encrypted")? != 0,
            status,
            play_count: get_i64(row, "play_count")? as u32,
            cached_at: get_i64(row, "cached_at")?,
            last_accessed_at: get_i64(row, "last_accessed_at")?,
            download_started_at: get_optional_i64(row, "download_started_at")?,
            downloaded_bytes: get_i64(row, "downloaded_bytes")? as u64,
            download_attempts: get_i64(row, "download_attempts")? as u32,
            last_error: get_optional_string(row, "last_error")?,
        })
    }

    /// Parse status string to enum.
    fn parse_status(s: &str) -> Result<CacheStatus> {
        match s {
            "not_cached" => Ok(CacheStatus::NotCached),
            "downloading" => Ok(CacheStatus::Downloading),
            "cached" => Ok(CacheStatus::Cached),
            "failed" => Ok(CacheStatus::Failed),
            "stale" => Ok(CacheStatus::Stale),
            _ => Err(LibraryError::CacheError(format!(
                "Unknown cache status: {}",
                s
            ))),
        }
    }

    /// Convert status enum to string.
    fn status_to_string(status: CacheStatus) -> &'static str {
        match status {
            CacheStatus::NotCached => "not_cached",
            CacheStatus::Downloading => "downloading",
            CacheStatus::Cached => "cached",
            CacheStatus::Failed => "failed",
            CacheStatus::Stale => "stale",
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl CacheMetadataRepository for SqliteCacheMetadataRepository {
    #[instrument(skip(self))]
    async fn initialize(&self) -> Result<()> {
        debug!("Initializing cache metadata repository");

        // Execute each statement separately
        let statements = [
            ("CREATE TABLE IF NOT EXISTS cache_metadata (
                track_id TEXT PRIMARY KEY NOT NULL,
                cache_path TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                cached_size INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                encrypted INTEGER NOT NULL,
                status TEXT NOT NULL,
                play_count INTEGER NOT NULL DEFAULT 0,
                cached_at INTEGER NOT NULL,
                last_accessed_at INTEGER NOT NULL,
                download_started_at INTEGER,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                download_attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT
            )", &[] as &[QueryValue]),
            ("CREATE INDEX IF NOT EXISTS idx_cache_status ON cache_metadata(status)", &[]),
            ("CREATE INDEX IF NOT EXISTS idx_cache_last_accessed ON cache_metadata(last_accessed_at)", &[]),
            ("CREATE INDEX IF NOT EXISTS idx_cache_play_count ON cache_metadata(play_count)", &[]),
            ("CREATE INDEX IF NOT EXISTS idx_cache_cached_at ON cache_metadata(cached_at)", &[]),
            ("CREATE INDEX IF NOT EXISTS idx_cache_size ON cache_metadata(cached_size)", &[]),
        ];

        self.db
            .execute_batch(&statements)
            .await
            .map_err(|e| {
                error!("Failed to create cache_metadata table: {}", e);
                LibraryError::CacheError(format!("Failed to initialize repository: {}", e))
            })?;

        debug!("Cache metadata repository initialized");
        Ok(())
    }

    #[instrument(skip(self, track))]
    #[instrument(skip(self, track))]
    async fn insert(&self, track: &CachedTrack) -> Result<()> {
        let sql = r#"
            INSERT INTO cache_metadata (
                track_id, cache_path, file_size, cached_size, content_hash,
                encrypted, status, play_count, cached_at, last_accessed_at,
                download_started_at, downloaded_bytes, download_attempts, last_error
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let params = vec![
            QueryValue::Text(track.track_id.to_string()),
            QueryValue::Text(track.cache_path.clone()),
            QueryValue::Integer(track.file_size as i64),
            QueryValue::Integer(track.cached_size as i64),
            QueryValue::Text(track.content_hash.clone()),
            QueryValue::Integer(if track.encrypted { 1 } else { 0 }),
            QueryValue::Text(Self::status_to_string(track.status).to_string()),
            QueryValue::Integer(track.play_count as i64),
            QueryValue::Integer(track.cached_at),
            QueryValue::Integer(track.last_accessed_at),
            track
                .download_started_at
                .map(QueryValue::Integer)
                .unwrap_or(QueryValue::Null),
            QueryValue::Integer(track.downloaded_bytes as i64),
            QueryValue::Integer(track.download_attempts as i64),
            track
                .last_error
                .as_ref()
                .map(|s| QueryValue::Text(s.clone()))
                .unwrap_or(QueryValue::Null),
        ];

        self.db.execute(sql, &params).await.map_err(|e| {
            error!("Failed to insert cache entry: {}", e);
            LibraryError::CacheError(format!("Failed to insert cache entry: {}", e))
        })?;

        Ok(())
    }

    #[instrument(skip(self, track))]
    async fn update(&self, track: &CachedTrack) -> Result<()> {
        let sql = r#"
            UPDATE cache_metadata SET
                cache_path = ?, file_size = ?, cached_size = ?, content_hash = ?,
                encrypted = ?, status = ?, play_count = ?, cached_at = ?,
                last_accessed_at = ?, download_started_at = ?, downloaded_bytes = ?,
                download_attempts = ?, last_error = ?
            WHERE track_id = ?
        "#;

        let params = vec![
            QueryValue::Text(track.cache_path.clone()),
            QueryValue::Integer(track.file_size as i64),
            QueryValue::Integer(track.cached_size as i64),
            QueryValue::Text(track.content_hash.clone()),
            QueryValue::Integer(if track.encrypted { 1 } else { 0 }),
            QueryValue::Text(Self::status_to_string(track.status).to_string()),
            QueryValue::Integer(track.play_count as i64),
            QueryValue::Integer(track.cached_at),
            QueryValue::Integer(track.last_accessed_at),
            track
                .download_started_at
                .map(QueryValue::Integer)
                .unwrap_or(QueryValue::Null),
            QueryValue::Integer(track.downloaded_bytes as i64),
            QueryValue::Integer(track.download_attempts as i64),
            track
                .last_error
                .as_ref()
                .map(|s| QueryValue::Text(s.clone()))
                .unwrap_or(QueryValue::Null),
            QueryValue::Text(track.track_id.to_string()),
        ];

        self.db.execute(sql, &params).await.map_err(|e| {
            error!("Failed to update cache entry: {}", e);
            LibraryError::CacheError(format!("Failed to update cache entry: {}", e))
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_track_id(&self, track_id: &TrackId) -> Result<Option<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata WHERE track_id = ?";
        let params = vec![QueryValue::Text(track_id.to_string())];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query cache entry: {}", e);
            LibraryError::CacheError(format!("Failed to query cache entry: {}", e))
        })?;

        if rows.is_empty() {
            return Ok(None);
        }

        Self::row_to_cached_track(&rows[0]).map(Some)
    }

    #[instrument(skip(self))]
    async fn find_by_status(&self, status: CacheStatus) -> Result<Vec<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata WHERE status = ?";
        let params = vec![QueryValue::Text(Self::status_to_string(status).to_string())];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query cache entries by status: {}", e);
            LibraryError::CacheError(format!("Failed to query by status: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }

    #[instrument(skip(self))]
    async fn find_for_lru_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata WHERE status = 'cached' ORDER BY last_accessed_at ASC LIMIT ?";
        let params = vec![QueryValue::Integer(limit as i64)];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query for LRU eviction: {}", e);
            LibraryError::CacheError(format!("Failed to query for eviction: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }

    #[instrument(skip(self))]
    async fn find_for_lfu_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata WHERE status = 'cached' ORDER BY play_count ASC LIMIT ?";
        let params = vec![QueryValue::Integer(limit as i64)];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query for LFU eviction: {}", e);
            LibraryError::CacheError(format!("Failed to query for eviction: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }

    #[instrument(skip(self))]
    async fn find_for_fifo_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>> {
        let sql =
            "SELECT * FROM cache_metadata WHERE status = 'cached' ORDER BY cached_at ASC LIMIT ?";
        let params = vec![QueryValue::Integer(limit as i64)];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query for FIFO eviction: {}", e);
            LibraryError::CacheError(format!("Failed to query for eviction: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }

    #[instrument(skip(self))]
    async fn find_for_largest_eviction(&self, limit: usize) -> Result<Vec<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata WHERE status = 'cached' ORDER BY cached_size DESC LIMIT ?";
        let params = vec![QueryValue::Integer(limit as i64)];

        let rows = self.db.query(sql, &params).await.map_err(|e| {
            error!("Failed to query for largest-first eviction: {}", e);
            LibraryError::CacheError(format!("Failed to query for eviction: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }

    #[instrument(skip(self))]
    async fn delete(&self, track_id: &TrackId) -> Result<()> {
        let sql = "DELETE FROM cache_metadata WHERE track_id = ?";
        let params = vec![QueryValue::Text(track_id.to_string())];

        self.db.execute(sql, &params).await.map_err(|e| {
            error!("Failed to delete cache entry: {}", e);
            LibraryError::CacheError(format!("Failed to delete cache entry: {}", e))
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_stats(&self) -> Result<CacheStats> {
        let sql = r#"
            SELECT 
                COUNT(*) as total_tracks,
                SUM(CASE WHEN status = 'cached' THEN 1 ELSE 0 END) as cached_tracks,
                SUM(CASE WHEN status = 'downloading' THEN 1 ELSE 0 END) as downloading_tracks,
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed_tracks,
                COALESCE(SUM(cached_size), 0) as total_bytes,
                COALESCE(SUM(file_size), 0) as total_original_bytes,
                SUM(CASE WHEN encrypted = 1 THEN 1 ELSE 0 END) as encrypted_tracks,
                COALESCE(SUM(play_count), 0) as total_plays
            FROM cache_metadata
        "#;

        let rows = self.db.query(sql, &[]).await.map_err(|e| {
            error!("Failed to get cache stats: {}", e);
            LibraryError::CacheError(format!("Failed to get stats: {}", e))
        })?;

        if rows.is_empty() {
            return Ok(CacheStats::default());
        }

        let row = &rows[0];
        Ok(CacheStats {
            total_tracks: get_i64(row, "total_tracks")? as usize,
            cached_tracks: get_i64(row, "cached_tracks")? as usize,
            downloading_tracks: get_i64(row, "downloading_tracks")? as usize,
            failed_tracks: get_i64(row, "failed_tracks")? as usize,
            total_bytes: get_i64(row, "total_bytes")? as u64,
            total_original_bytes: get_i64(row, "total_original_bytes")? as u64,
            encrypted_tracks: get_i64(row, "encrypted_tracks")? as usize,
            total_plays: get_i64(row, "total_plays")? as u64,
            tracks_pending_eviction: 0, // Calculated separately if needed
            calculated_at: chrono::Utc::now().timestamp(),
        })
    }

    #[instrument(skip(self))]
    async fn find_all(&self) -> Result<Vec<CachedTrack>> {
        let sql = "SELECT * FROM cache_metadata";

        let rows = self.db.query(sql, &[]).await.map_err(|e| {
            error!("Failed to query all cache entries: {}", e);
            LibraryError::CacheError(format!("Failed to query all entries: {}", e))
        })?;

        rows.iter().map(Self::row_to_cached_track).collect()
    }
}

// ============================================================================
// Helper functions for extracting values from QueryRow
// ============================================================================

fn get_string(row: &QueryRow, key: &str) -> Result<String> {
    row.get(key)
        .and_then(|value| value.as_string())
        .ok_or_else(|| LibraryError::CacheError(format!("Missing column: {}", key)))
}

fn get_optional_string(row: &QueryRow, key: &str) -> Result<Option<String>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(value.as_string().ok_or_else(|| {
            LibraryError::CacheError(format!("Invalid type for column: {}", key))
        })?),
    })
}

fn get_i64(row: &QueryRow, key: &str) -> Result<i64> {
    row.get(key)
        .and_then(|value| value.as_i64())
        .ok_or_else(|| LibraryError::CacheError(format!("Missing column: {}", key)))
}

fn get_optional_i64(row: &QueryRow, key: &str) -> Result<Option<i64>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(value.as_i64().ok_or_else(|| {
            LibraryError::CacheError(format!("Invalid type for column: {}", key))
        })?),
    })
}
