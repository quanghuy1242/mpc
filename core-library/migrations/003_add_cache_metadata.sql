-- Migration 003: Add offline cache metadata table
-- This table tracks downloaded tracks for offline playback

CREATE TABLE IF NOT EXISTS cache_metadata (
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
    last_error TEXT,
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_cache_status ON cache_metadata(status);
CREATE INDEX IF NOT EXISTS idx_cache_last_accessed ON cache_metadata(last_accessed_at);
CREATE INDEX IF NOT EXISTS idx_cache_play_count ON cache_metadata(play_count);
CREATE INDEX IF NOT EXISTS idx_cache_cached_at ON cache_metadata(cached_at);
CREATE INDEX IF NOT EXISTS idx_cache_size ON cache_metadata(cached_size);
