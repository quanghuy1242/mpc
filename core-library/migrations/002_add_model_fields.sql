-- Migration: 002_add_model_fields
-- Description: Add missing fields from domain models to database schema
--
-- This migration adds optional enrichment fields that were designed in the domain
-- models but were not included in the initial schema. These fields support future
-- features like artist biographies, genre filtering, and playlist sharing.
--
-- IMPORTANT: All new fields are either optional (NULL allowed) or have default values
-- to ensure backward compatibility with existing data.

-- =============================================================================
-- ARTIST ENHANCEMENTS
-- =============================================================================
-- Add rich metadata fields for artist information
ALTER TABLE artists ADD COLUMN bio TEXT;
ALTER TABLE artists ADD COLUMN country TEXT;

-- Note: These fields enable future features:
-- - bio: Artist biography/description for rich UI display
-- - country: Country of origin for filtering and music discovery

-- =============================================================================
-- PLAYLIST ENHANCEMENTS
-- =============================================================================
-- Add sharing capability field
ALTER TABLE playlists ADD COLUMN is_public INTEGER NOT NULL DEFAULT 0;

-- Note: is_public enables playlist sharing features:
-- - 0 (false): Private playlist, only visible to owner
-- - 1 (true): Public playlist, can be shared with others

-- =============================================================================
-- ALBUM ENHANCEMENTS
-- =============================================================================
-- Add genre classification field
ALTER TABLE albums ADD COLUMN genre TEXT;

-- Note: genre enables:
-- - Genre-based filtering and browsing
-- - Music discovery by genre
-- - Better organization of music library

-- =============================================================================
-- FOLDER ENHANCEMENTS
-- =============================================================================
-- Add modification tracking for sync optimization
ALTER TABLE folders ADD COLUMN updated_at INTEGER NOT NULL DEFAULT (unixepoch());

-- Note: updated_at enables:
-- - Tracking when folder structure changes
-- - Optimizing incremental sync operations
-- - Detecting remote folder modifications

-- =============================================================================
-- LYRICS ENHANCEMENTS
-- =============================================================================
-- Add modification tracking for cache invalidation
ALTER TABLE lyrics ADD COLUMN updated_at INTEGER NOT NULL DEFAULT (unixepoch());

-- Note: updated_at enables:
-- - Tracking when lyrics are refreshed
-- - Cache invalidation for stale lyrics
-- - Re-fetching lyrics from better sources

-- =============================================================================
-- FULL-TEXT SEARCH INDEX UPDATES
-- =============================================================================
-- Update albums FTS to include genre for comprehensive search
-- Note: We need to drop and recreate FTS tables when adding columns

DROP TRIGGER IF EXISTS albums_fts_insert;
DROP TRIGGER IF EXISTS albums_fts_update;
DROP TRIGGER IF EXISTS albums_fts_delete;
DROP TABLE IF EXISTS albums_fts;

CREATE VIRTUAL TABLE albums_fts USING fts5(
    album_id UNINDEXED,
    name,
    artist_name,
    genre
);

-- Populate the new FTS table
INSERT INTO albums_fts(rowid, album_id, name, artist_name, genre)
SELECT 
    a.rowid,
    a.id,
    a.name,
    COALESCE(ar.name, ''),
    COALESCE(a.genre, '')
FROM albums a
LEFT JOIN artists ar ON a.artist_id = ar.id;

-- Recreate triggers to keep FTS in sync
CREATE TRIGGER albums_fts_insert AFTER INSERT ON albums BEGIN
    INSERT INTO albums_fts(rowid, album_id, name, artist_name, genre)
    SELECT 
        new.rowid,
        new.id,
        new.name,
        COALESCE((SELECT name FROM artists WHERE id = new.artist_id), ''),
        COALESCE(new.genre, '');
END;

CREATE TRIGGER albums_fts_update AFTER UPDATE ON albums BEGIN
    UPDATE albums_fts 
    SET 
        name = new.name,
        artist_name = COALESCE((SELECT name FROM artists WHERE id = new.artist_id), ''),
        genre = COALESCE(new.genre, '')
    WHERE rowid = new.rowid;
END;

CREATE TRIGGER albums_fts_delete AFTER DELETE ON albums BEGIN
    DELETE FROM albums_fts WHERE rowid = old.rowid;
END;

-- =============================================================================
-- INDEXES FOR NEW FIELDS
-- =============================================================================
-- Add indexes for new queryable fields to maintain performance

CREATE INDEX idx_artists_country ON artists(country);
CREATE INDEX idx_albums_genre ON albums(genre);
CREATE INDEX idx_playlists_is_public ON playlists(is_public);

-- Note: We don't index bio as it's for display, not filtering
-- Note: updated_at fields are indexed by temporal queries if needed
