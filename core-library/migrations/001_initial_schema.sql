-- Migration: 001_initial_schema
-- Description: Initial database schema for music library
-- 
-- This migration creates all tables needed for the music library core:
-- - Providers: Cloud storage provider instances
-- - Artists: Music artists
-- - Albums: Music albums
-- - Tracks: Individual music tracks
-- - Playlists: User-created playlists
-- - Playlist tracks: Many-to-many relationship
-- - Folders: Provider folder structure
-- - Artworks: Album/track artwork with deduplication
-- - Lyrics: Track lyrics (synced and plain)
-- - Sync jobs: Synchronization history

-- =============================================================================
-- PROVIDERS TABLE
-- =============================================================================
-- Stores cloud storage provider configurations and sync state
CREATE TABLE providers (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for provider instance
    type TEXT NOT NULL,                        -- 'GoogleDrive' or 'OneDrive'
    display_name TEXT NOT NULL,                -- User-friendly name
    profile_id TEXT NOT NULL,                  -- Links to auth profile
    sync_cursor TEXT,                          -- Last sync position for incremental sync
    last_sync_at INTEGER,                      -- Unix timestamp of last successful sync
    created_at INTEGER NOT NULL,               -- Unix timestamp when provider was added
    
    CONSTRAINT providers_type_check CHECK (type IN ('GoogleDrive', 'OneDrive'))
);

CREATE INDEX idx_providers_profile ON providers(profile_id);
CREATE INDEX idx_providers_type ON providers(type);

-- =============================================================================
-- ARTISTS TABLE
-- =============================================================================
-- Stores music artist information
CREATE TABLE artists (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for artist
    name TEXT NOT NULL,                        -- Artist display name
    normalized_name TEXT NOT NULL,             -- Lowercase, trimmed for searching
    sort_name TEXT,                            -- For alphabetical sorting (e.g., "Beatles, The")
    created_at INTEGER NOT NULL,               -- Unix timestamp when first added
    updated_at INTEGER NOT NULL,               -- Unix timestamp of last update
    
    CONSTRAINT artists_name_not_empty CHECK (length(trim(name)) > 0)
);

CREATE UNIQUE INDEX idx_artists_normalized_name ON artists(normalized_name);
CREATE INDEX idx_artists_name ON artists(name);

-- =============================================================================
-- ALBUMS TABLE
-- =============================================================================
-- Stores album information
CREATE TABLE albums (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for album
    name TEXT NOT NULL,                        -- Album title
    normalized_name TEXT NOT NULL,             -- Lowercase, trimmed for searching
    artist_id TEXT,                            -- Album artist (can be NULL for compilations)
    year INTEGER,                              -- Release year
    artwork_id TEXT,                           -- Reference to artwork
    track_count INTEGER DEFAULT 0,             -- Cached track count for performance
    total_duration_ms INTEGER DEFAULT 0,       -- Cached total duration in milliseconds
    created_at INTEGER NOT NULL,               -- Unix timestamp when first added
    updated_at INTEGER NOT NULL,               -- Unix timestamp of last update
    
    CONSTRAINT albums_name_not_empty CHECK (length(trim(name)) > 0),
    CONSTRAINT albums_year_range CHECK (year IS NULL OR (year >= 1900 AND year <= 2100)),
    FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE SET NULL,
    FOREIGN KEY (artwork_id) REFERENCES artworks(id) ON DELETE SET NULL
);

CREATE INDEX idx_albums_artist ON albums(artist_id);
CREATE INDEX idx_albums_normalized_name ON albums(normalized_name);
CREATE INDEX idx_albums_year ON albums(year);
CREATE INDEX idx_albums_artwork ON albums(artwork_id);

-- =============================================================================
-- TRACKS TABLE
-- =============================================================================
-- Stores individual music track information
CREATE TABLE tracks (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for track
    provider_id TEXT NOT NULL,                 -- Which provider this track is from
    provider_file_id TEXT NOT NULL,            -- Provider's file identifier
    hash TEXT,                                 -- Content hash (MD5/SHA256) for deduplication
    
    -- Metadata fields
    title TEXT NOT NULL,                       -- Track title
    normalized_title TEXT NOT NULL,            -- Lowercase, trimmed for searching
    album_id TEXT,                             -- Reference to album
    artist_id TEXT,                            -- Track artist (may differ from album artist)
    album_artist_id TEXT,                      -- Album artist (for compilations)
    track_number INTEGER,                      -- Track position on album
    disc_number INTEGER DEFAULT 1,             -- Disc number for multi-disc albums
    genre TEXT,                                -- Music genre
    year INTEGER,                              -- Release year
    
    -- Audio properties
    duration_ms INTEGER NOT NULL,              -- Duration in milliseconds
    bitrate INTEGER,                           -- Bitrate in kbps
    sample_rate INTEGER,                       -- Sample rate in Hz
    channels INTEGER,                          -- Number of audio channels (1=mono, 2=stereo)
    format TEXT NOT NULL,                      -- File format (mp3, flac, m4a, etc.)
    
    -- File metadata
    file_size INTEGER,                         -- File size in bytes
    mime_type TEXT,                            -- MIME type from provider
    
    -- Enrichment status
    artwork_id TEXT,                           -- Reference to artwork (if different from album)
    lyrics_status TEXT DEFAULT 'not_fetched', -- 'not_fetched', 'fetching', 'available', 'unavailable'
    
    -- Timestamps
    created_at INTEGER NOT NULL,               -- Unix timestamp when first added
    updated_at INTEGER NOT NULL,               -- Unix timestamp of last update
    provider_modified_at INTEGER,              -- Last modified time from provider
    
    -- Constraints
    CONSTRAINT tracks_title_not_empty CHECK (length(trim(title)) > 0),
    CONSTRAINT tracks_duration_positive CHECK (duration_ms > 0),
    CONSTRAINT tracks_year_range CHECK (year IS NULL OR (year >= 1900 AND year <= 2100)),
    CONSTRAINT tracks_track_number_positive CHECK (track_number IS NULL OR track_number > 0),
    CONSTRAINT tracks_disc_number_positive CHECK (disc_number > 0),
    CONSTRAINT tracks_lyrics_status_check CHECK (
        lyrics_status IN ('not_fetched', 'fetching', 'available', 'unavailable')
    ),
    
    -- Foreign keys
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE,
    FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE SET NULL,
    FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE SET NULL,
    FOREIGN KEY (album_artist_id) REFERENCES artists(id) ON DELETE SET NULL,
    FOREIGN KEY (artwork_id) REFERENCES artworks(id) ON DELETE SET NULL
);

-- Indexes for common queries and lookups
CREATE UNIQUE INDEX idx_tracks_provider_file ON tracks(provider_id, provider_file_id);
CREATE INDEX idx_tracks_hash ON tracks(hash);
CREATE INDEX idx_tracks_album ON tracks(album_id);
CREATE INDEX idx_tracks_artist ON tracks(artist_id);
CREATE INDEX idx_tracks_album_artist ON tracks(album_artist_id);
CREATE INDEX idx_tracks_normalized_title ON tracks(normalized_title);
CREATE INDEX idx_tracks_genre ON tracks(genre);
CREATE INDEX idx_tracks_format ON tracks(format);
CREATE INDEX idx_tracks_lyrics_status ON tracks(lyrics_status);
CREATE INDEX idx_tracks_created_at ON tracks(created_at);

-- =============================================================================
-- PLAYLISTS TABLE
-- =============================================================================
-- Stores user-created playlists
CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for playlist
    name TEXT NOT NULL,                        -- Playlist name
    normalized_name TEXT NOT NULL,             -- Lowercase, trimmed for searching
    description TEXT,                          -- Optional description
    owner_type TEXT NOT NULL DEFAULT 'user',   -- 'user' or 'system' (for auto-generated)
    sort_order TEXT DEFAULT 'manual',          -- 'manual', 'title', 'artist', 'album', 'date_added'
    track_count INTEGER DEFAULT 0,             -- Cached track count
    total_duration_ms INTEGER DEFAULT 0,       -- Cached total duration
    artwork_id TEXT,                           -- Optional playlist cover art
    created_at INTEGER NOT NULL,               -- Unix timestamp when created
    updated_at INTEGER NOT NULL,               -- Unix timestamp of last update
    
    CONSTRAINT playlists_name_not_empty CHECK (length(trim(name)) > 0),
    CONSTRAINT playlists_owner_type_check CHECK (owner_type IN ('user', 'system')),
    CONSTRAINT playlists_sort_order_check CHECK (
        sort_order IN ('manual', 'title', 'artist', 'album', 'date_added', 'duration')
    ),
    FOREIGN KEY (artwork_id) REFERENCES artworks(id) ON DELETE SET NULL
);

CREATE INDEX idx_playlists_normalized_name ON playlists(normalized_name);
CREATE INDEX idx_playlists_owner_type ON playlists(owner_type);

-- =============================================================================
-- PLAYLIST_TRACKS TABLE
-- =============================================================================
-- Many-to-many relationship between playlists and tracks
CREATE TABLE playlist_tracks (
    playlist_id TEXT NOT NULL,                 -- Reference to playlist
    track_id TEXT NOT NULL,                    -- Reference to track
    position INTEGER NOT NULL,                 -- Position in playlist (1-based)
    added_at INTEGER NOT NULL,                 -- Unix timestamp when track was added
    
    PRIMARY KEY (playlist_id, track_id),
    
    CONSTRAINT playlist_tracks_position_positive CHECK (position > 0),
    FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

CREATE INDEX idx_playlist_tracks_position ON playlist_tracks(playlist_id, position);
CREATE INDEX idx_playlist_tracks_track ON playlist_tracks(track_id);

-- =============================================================================
-- FOLDERS TABLE
-- =============================================================================
-- Stores provider folder structure for organization
CREATE TABLE folders (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for folder
    provider_id TEXT NOT NULL,                 -- Which provider this folder is from
    provider_folder_id TEXT NOT NULL,          -- Provider's folder identifier
    name TEXT NOT NULL,                        -- Folder name
    normalized_name TEXT NOT NULL,             -- Lowercase, trimmed for searching
    parent_id TEXT,                            -- Parent folder (NULL for root)
    path TEXT NOT NULL,                        -- Full path for display
    created_at INTEGER NOT NULL,               -- Unix timestamp when first seen
    
    CONSTRAINT folders_name_not_empty CHECK (length(trim(name)) > 0),
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_folders_provider_folder ON folders(provider_id, provider_folder_id);
CREATE INDEX idx_folders_parent ON folders(parent_id);
CREATE INDEX idx_folders_path ON folders(path);

-- =============================================================================
-- ARTWORKS TABLE
-- =============================================================================
-- Stores album/track artwork with content-based deduplication
CREATE TABLE artworks (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for artwork
    hash TEXT NOT NULL,                        -- Content hash for deduplication
    binary_blob BLOB NOT NULL,                 -- Image data
    mime_type TEXT NOT NULL,                   -- Image format (image/jpeg, image/png, image/webp)
    width INTEGER NOT NULL,                    -- Image width in pixels
    height INTEGER NOT NULL,                   -- Image height in pixels
    file_size INTEGER NOT NULL,                -- Size in bytes
    dominant_color TEXT,                       -- Hex color code for UI theming (e.g., '#FF5733')
    source TEXT DEFAULT 'embedded',            -- 'embedded', 'remote', 'user_uploaded'
    created_at INTEGER NOT NULL,               -- Unix timestamp when first added
    
    CONSTRAINT artworks_hash_not_empty CHECK (length(hash) > 0),
    CONSTRAINT artworks_dimensions_positive CHECK (width > 0 AND height > 0),
    CONSTRAINT artworks_size_positive CHECK (file_size > 0),
    CONSTRAINT artworks_mime_type_check CHECK (
        mime_type IN ('image/jpeg', 'image/png', 'image/webp', 'image/gif')
    ),
    CONSTRAINT artworks_source_check CHECK (
        source IN ('embedded', 'remote', 'user_uploaded')
    )
);

CREATE UNIQUE INDEX idx_artworks_hash ON artworks(hash);
CREATE INDEX idx_artworks_source ON artworks(source);

-- =============================================================================
-- LYRICS TABLE
-- =============================================================================
-- Stores track lyrics (both synced LRC and plain text)
CREATE TABLE lyrics (
    track_id TEXT PRIMARY KEY NOT NULL,        -- One-to-one with track
    source TEXT NOT NULL,                      -- 'lrclib', 'musixmatch', 'embedded', 'manual'
    synced BOOLEAN NOT NULL DEFAULT 0,         -- 1 if LRC format, 0 if plain text
    body TEXT NOT NULL,                        -- Lyrics content (LRC or plain text)
    language TEXT,                             -- ISO 639-1 language code (e.g., 'en', 'ja')
    last_checked_at INTEGER NOT NULL,          -- Unix timestamp of last fetch attempt
    created_at INTEGER NOT NULL,               -- Unix timestamp when first added
    
    CONSTRAINT lyrics_body_not_empty CHECK (length(trim(body)) > 0),
    CONSTRAINT lyrics_source_check CHECK (
        source IN ('lrclib', 'musixmatch', 'embedded', 'manual', 'genius')
    ),
    FOREIGN KEY (track_id) REFERENCES tracks(id) ON DELETE CASCADE
);

CREATE INDEX idx_lyrics_source ON lyrics(source);
CREATE INDEX idx_lyrics_synced ON lyrics(synced);

-- =============================================================================
-- SYNC_JOBS TABLE
-- =============================================================================
-- Tracks synchronization job history and state
CREATE TABLE sync_jobs (
    id TEXT PRIMARY KEY NOT NULL,              -- UUID for sync job
    provider_id TEXT NOT NULL,                 -- Which provider was synced
    status TEXT NOT NULL DEFAULT 'pending',    -- 'pending', 'running', 'completed', 'failed', 'cancelled'
    sync_type TEXT NOT NULL,                   -- 'full' or 'incremental'
    
    -- Progress tracking
    items_discovered INTEGER DEFAULT 0,        -- Total files found
    items_processed INTEGER DEFAULT 0,         -- Files processed so far
    items_failed INTEGER DEFAULT 0,            -- Files that failed processing
    items_added INTEGER DEFAULT 0,             -- New tracks added
    items_updated INTEGER DEFAULT 0,           -- Existing tracks updated
    items_deleted INTEGER DEFAULT 0,           -- Tracks removed
    
    -- Error tracking
    error_message TEXT,                        -- Error description if failed
    error_details TEXT,                        -- Additional error context (JSON)
    
    -- State persistence
    cursor TEXT,                               -- Sync cursor for resumption
    
    -- Timestamps
    started_at INTEGER,                        -- Unix timestamp when job started
    completed_at INTEGER,                      -- Unix timestamp when job finished
    created_at INTEGER NOT NULL,               -- Unix timestamp when job was created
    
    CONSTRAINT sync_jobs_status_check CHECK (
        status IN ('pending', 'running', 'completed', 'failed', 'cancelled')
    ),
    CONSTRAINT sync_jobs_type_check CHECK (
        sync_type IN ('full', 'incremental')
    ),
    CONSTRAINT sync_jobs_counts_nonnegative CHECK (
        items_discovered >= 0 AND
        items_processed >= 0 AND
        items_failed >= 0 AND
        items_added >= 0 AND
        items_updated >= 0 AND
        items_deleted >= 0
    ),
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

CREATE INDEX idx_sync_jobs_provider ON sync_jobs(provider_id);
CREATE INDEX idx_sync_jobs_status ON sync_jobs(status);
CREATE INDEX idx_sync_jobs_created_at ON sync_jobs(created_at);

-- =============================================================================
-- FULL-TEXT SEARCH SETUP
-- =============================================================================
-- FTS5 virtual table for fast searching across tracks, albums, and artists

-- Track search index
CREATE VIRTUAL TABLE tracks_fts USING fts5(
    track_id UNINDEXED,                        -- Track UUID (not indexed, just stored)
    title,                                     -- Track title
    artist_name,                               -- Artist name
    album_name,                                -- Album name
    genre,                                     -- Genre
    content=tracks,                            -- Link to tracks table
    content_rowid=rowid
);

-- Triggers to keep FTS index in sync with tracks table
CREATE TRIGGER tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, track_id, title, artist_name, album_name, genre)
    SELECT 
        new.rowid,
        new.id,
        new.title,
        COALESCE((SELECT name FROM artists WHERE id = new.artist_id), ''),
        COALESCE((SELECT name FROM albums WHERE id = new.album_id), ''),
        COALESCE(new.genre, '');
END;

CREATE TRIGGER tracks_fts_update AFTER UPDATE ON tracks BEGIN
    UPDATE tracks_fts 
    SET 
        title = new.title,
        artist_name = COALESCE((SELECT name FROM artists WHERE id = new.artist_id), ''),
        album_name = COALESCE((SELECT name FROM albums WHERE id = new.album_id), ''),
        genre = COALESCE(new.genre, '')
    WHERE rowid = new.rowid;
END;

CREATE TRIGGER tracks_fts_delete AFTER DELETE ON tracks BEGIN
    DELETE FROM tracks_fts WHERE rowid = old.rowid;
END;

-- Album search index
CREATE VIRTUAL TABLE albums_fts USING fts5(
    album_id UNINDEXED,
    name,
    artist_name,
    content=albums,
    content_rowid=rowid
);

CREATE TRIGGER albums_fts_insert AFTER INSERT ON albums BEGIN
    INSERT INTO albums_fts(rowid, album_id, name, artist_name)
    SELECT 
        new.rowid,
        new.id,
        new.name,
        COALESCE((SELECT name FROM artists WHERE id = new.artist_id), '');
END;

CREATE TRIGGER albums_fts_update AFTER UPDATE ON albums BEGIN
    UPDATE albums_fts 
    SET 
        name = new.name,
        artist_name = COALESCE((SELECT name FROM artists WHERE id = new.artist_id), '')
    WHERE rowid = new.rowid;
END;

CREATE TRIGGER albums_fts_delete AFTER DELETE ON albums BEGIN
    DELETE FROM albums_fts WHERE rowid = old.rowid;
END;

-- Artist search index
CREATE VIRTUAL TABLE artists_fts USING fts5(
    artist_id UNINDEXED,
    name,
    content=artists,
    content_rowid=rowid
);

CREATE TRIGGER artists_fts_insert AFTER INSERT ON artists BEGIN
    INSERT INTO artists_fts(rowid, artist_id, name)
    VALUES (new.rowid, new.id, new.name);
END;

CREATE TRIGGER artists_fts_update AFTER UPDATE ON artists BEGIN
    UPDATE artists_fts SET name = new.name WHERE rowid = new.rowid;
END;

CREATE TRIGGER artists_fts_delete AFTER DELETE ON artists BEGIN
    DELETE FROM artists_fts WHERE rowid = old.rowid;
END;

-- =============================================================================
-- VIEWS FOR COMMON QUERIES
-- =============================================================================

-- View for track details with joined artist and album information
CREATE VIEW track_details AS
SELECT 
    t.id,
    t.title,
    t.album_id,
    a.name AS album_name,
    t.artist_id,
    art.name AS artist_name,
    t.album_artist_id,
    aa.name AS album_artist_name,
    t.track_number,
    t.disc_number,
    t.genre,
    t.year,
    t.duration_ms,
    t.bitrate,
    t.sample_rate,
    t.format,
    t.artwork_id,
    COALESCE(t.artwork_id, a.artwork_id) AS display_artwork_id,
    t.lyrics_status,
    t.provider_id,
    t.provider_file_id,
    t.created_at
FROM tracks t
LEFT JOIN albums a ON t.album_id = a.id
LEFT JOIN artists art ON t.artist_id = art.id
LEFT JOIN artists aa ON t.album_artist_id = aa.id;

-- View for album details with artist information and track counts
CREATE VIEW album_details AS
SELECT 
    alb.id,
    alb.name,
    alb.artist_id,
    art.name AS artist_name,
    alb.year,
    alb.artwork_id,
    alb.track_count,
    alb.total_duration_ms,
    COUNT(DISTINCT t.id) AS actual_track_count,
    SUM(t.duration_ms) AS actual_duration_ms,
    alb.created_at,
    alb.updated_at
FROM albums alb
LEFT JOIN artists art ON alb.artist_id = art.id
LEFT JOIN tracks t ON t.album_id = alb.id
GROUP BY alb.id;

-- =============================================================================
-- NOTE: PRAGMA SETTINGS
-- =============================================================================
-- 
-- PRAGMA settings (WAL mode, foreign keys, cache size, etc.) are configured
-- at connection time in the db.rs module, not in migrations.
-- 
-- This ensures:
-- - Settings work correctly with SQLx's transaction-based migrations
-- - Consistent configuration across all connections
-- - Proper handling of in-memory databases for testing
--
-- See core-library/src/db.rs for the connection configuration.
