//! Track repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Track;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query_as, SqlitePool};

/// Track repository interface for data access operations
#[async_trait]
pub trait TrackRepository: Send + Sync {
    /// Find a track by its ID
    ///
    /// # Returns
    /// - `Ok(Some(track))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>>;

    /// Insert a new track
    ///
    /// # Errors
    /// Returns error if:
    /// - Track with same ID already exists
    /// - Track validation fails
    /// - Database error occurs
    async fn insert(&self, track: &Track) -> Result<()>;

    /// Update an existing track
    ///
    /// # Errors
    /// Returns error if:
    /// - Track does not exist
    /// - Track validation fails
    /// - Database error occurs
    async fn update(&self, track: &Track) -> Result<()>;

    /// Delete a track by ID
    ///
    /// # Returns
    /// - `Ok(true)` if track was deleted
    /// - `Ok(false)` if track was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query tracks with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of tracks
    async fn query(&self, page_request: PageRequest) -> Result<Page<Track>>;

    /// Query tracks by album with pagination
    ///
    /// # Arguments
    /// * `album_id` - Album identifier
    /// * `page_request` - Pagination parameters
    async fn query_by_album(
        &self,
        album_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;

    /// Query tracks by artist with pagination
    ///
    /// # Arguments
    /// * `artist_id` - Artist identifier
    /// * `page_request` - Pagination parameters
    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;

    /// Query tracks by provider with pagination
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier
    /// * `page_request` - Pagination parameters
    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;

    /// Search tracks by title
    ///
    /// Uses FTS5 full-text search for efficient searching
    ///
    /// # Arguments
    /// * `search_query` - Search query string
    /// * `page_request` - Pagination parameters
    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Track>>;

    /// Count total tracks
    async fn count(&self) -> Result<i64>;

    /// Find track by provider file ID
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier
    /// * `provider_file_id` - Provider's file identifier
    async fn find_by_provider_file(
        &self,
        provider_id: &str,
        provider_file_id: &str,
    ) -> Result<Option<Track>>;
}

/// SQLite implementation of TrackRepository
pub struct SqliteTrackRepository {
    pool: SqlitePool,
}

impl SqliteTrackRepository {
    /// Create a new SQLite track repository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TrackRepository for SqliteTrackRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>> {
        let track = query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(track)
    }

    async fn insert(&self, track: &Track) -> Result<()> {
        // Validate track data
        track.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "track".to_string(),
            message: msg,
        })?;

        sqlx::query(
            r#"
            INSERT INTO tracks (
                id, provider_id, provider_file_id, hash,
                title, normalized_title, album_id, artist_id, album_artist_id,
                track_number, disc_number, genre, year,
                duration_ms, bitrate, sample_rate, channels, format,
                file_size, mime_type, artwork_id, lyrics_status,
                created_at, updated_at, provider_modified_at
            ) VALUES (
                ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?
            )
            "#,
        )
        .bind(&track.id)
        .bind(&track.provider_id)
        .bind(&track.provider_file_id)
        .bind(&track.hash)
        .bind(&track.title)
        .bind(&track.normalized_title)
        .bind(&track.album_id)
        .bind(&track.artist_id)
        .bind(&track.album_artist_id)
        .bind(track.track_number)
        .bind(track.disc_number)
        .bind(&track.genre)
        .bind(track.year)
        .bind(track.duration_ms)
        .bind(track.bitrate)
        .bind(track.sample_rate)
        .bind(track.channels)
        .bind(&track.format)
        .bind(track.file_size)
        .bind(&track.mime_type)
        .bind(&track.artwork_id)
        .bind(&track.lyrics_status)
        .bind(track.created_at)
        .bind(track.updated_at)
        .bind(track.provider_modified_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, track: &Track) -> Result<()> {
        // Validate track data
        track.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "track".to_string(),
            message: msg,
        })?;

        let result = sqlx::query(
            r#"
            UPDATE tracks SET
                provider_id = ?, provider_file_id = ?, hash = ?,
                title = ?, normalized_title = ?, album_id = ?, artist_id = ?, album_artist_id = ?,
                track_number = ?, disc_number = ?, genre = ?, year = ?,
                duration_ms = ?, bitrate = ?, sample_rate = ?, channels = ?, format = ?,
                file_size = ?, mime_type = ?, artwork_id = ?, lyrics_status = ?,
                updated_at = ?, provider_modified_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&track.provider_id)
        .bind(&track.provider_file_id)
        .bind(&track.hash)
        .bind(&track.title)
        .bind(&track.normalized_title)
        .bind(&track.album_id)
        .bind(&track.artist_id)
        .bind(&track.album_artist_id)
        .bind(track.track_number)
        .bind(track.disc_number)
        .bind(&track.genre)
        .bind(track.year)
        .bind(track.duration_ms)
        .bind(track.bitrate)
        .bind(track.sample_rate)
        .bind(track.channels)
        .bind(&track.format)
        .bind(track.file_size)
        .bind(&track.mime_type)
        .bind(&track.artwork_id)
        .bind(&track.lyrics_status)
        .bind(track.updated_at)
        .bind(track.provider_modified_at)
        .bind(&track.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Track".to_string(),
                id: track.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Track>> {
        // Get total count
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks")
            .fetch_one(&self.pool)
            .await?;

        // Get paginated tracks
        let tracks =
            query_as::<_, Track>("SELECT * FROM tracks ORDER BY created_at DESC LIMIT ? OFFSET ?")
                .bind(page_request.limit() as i64)
                .bind(page_request.offset() as i64)
                .fetch_all(&self.pool)
                .await?;

        Ok(Page::new(tracks, total.0 as u64, page_request))
    }

    async fn query_by_album(
        &self,
        album_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        // Get total count
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE album_id = ?")
            .bind(album_id)
            .fetch_one(&self.pool)
            .await?;

        // Get paginated tracks
        let tracks = query_as::<_, Track>(
            "SELECT * FROM tracks WHERE album_id = ? ORDER BY disc_number, track_number LIMIT ? OFFSET ?"
        )
        .bind(album_id)
        .bind(page_request.limit() as i64)
        .bind(page_request.offset() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(tracks, total.0 as u64, page_request))
    }

    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        // Get total count
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE artist_id = ?")
            .bind(artist_id)
            .fetch_one(&self.pool)
            .await?;

        // Get paginated tracks
        let tracks = query_as::<_, Track>(
            "SELECT * FROM tracks WHERE artist_id = ? ORDER BY year DESC, album_id, track_number LIMIT ? OFFSET ?"
        )
        .bind(artist_id)
        .bind(page_request.limit() as i64)
        .bind(page_request.offset() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(tracks, total.0 as u64, page_request))
    }

    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        // Get total count
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE provider_id = ?")
            .bind(provider_id)
            .fetch_one(&self.pool)
            .await?;

        // Get paginated tracks
        let tracks = query_as::<_, Track>(
            "SELECT * FROM tracks WHERE provider_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(provider_id)
        .bind(page_request.limit() as i64)
        .bind(page_request.offset() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(tracks, total.0 as u64, page_request))
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Track>> {
        // Use FTS5 for full-text search
        let search_pattern = format!("%{}%", search_query.to_lowercase());

        // Get total count
        let total: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE normalized_title LIKE ?")
                .bind(&search_pattern)
                .fetch_one(&self.pool)
                .await?;

        // Get paginated tracks
        let tracks = query_as::<_, Track>(
            "SELECT * FROM tracks WHERE normalized_title LIKE ? ORDER BY title LIMIT ? OFFSET ?",
        )
        .bind(&search_pattern)
        .bind(page_request.limit() as i64)
        .bind(page_request.offset() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(tracks, total.0 as u64, page_request))
    }

    async fn count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks")
            .fetch_one(&self.pool)
            .await?;

        Ok(count.0)
    }

    async fn find_by_provider_file(
        &self,
        provider_id: &str,
        provider_file_id: &str,
    ) -> Result<Option<Track>> {
        let track = query_as::<_, Track>(
            "SELECT * FROM tracks WHERE provider_id = ? AND provider_file_id = ?",
        )
        .bind(provider_id)
        .bind(provider_file_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    /// Helper to insert a test provider
    async fn insert_test_provider(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO providers (id, type, display_name, profile_id, created_at) 
             VALUES ('test-provider', 'GoogleDrive', 'Test Provider', 'test-profile', 1699200000)",
        )
        .execute(pool)
        .await
        .ok(); // Ignore error if already exists
    }

    async fn create_test_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            provider_id: "test-provider".to_string(),
            provider_file_id: format!("file-{}", id),
            hash: Some("test-hash".to_string()),
            title: "Test Track".to_string(),
            normalized_title: "test track".to_string(),
            album_id: None,
            artist_id: None,
            album_artist_id: None,
            track_number: Some(1),
            disc_number: 1,
            genre: Some("Rock".to_string()),
            year: Some(2024),
            duration_ms: 180000,
            bitrate: Some(320),
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(5242880),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: None,
            lyrics_status: "not_fetched".to_string(),
            created_at: 1699200000,
            updated_at: 1699200000,
            provider_modified_at: Some(1699200000),
        }
    }

    #[tokio::test]
    async fn test_insert_and_find_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);
        let track = create_test_track("track-1").await;

        // Insert track
        repo.insert(&track).await.unwrap();

        // Find track
        let found = repo.find_by_id("track-1").await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, "track-1");
        assert_eq!(found.title, "Test Track");
    }

    #[tokio::test]
    async fn test_update_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);
        let mut track = create_test_track("track-2").await;

        // Insert track
        repo.insert(&track).await.unwrap();

        // Update track
        track.title = "Updated Track".to_string();
        track.normalized_title = "updated track".to_string();
        repo.update(&track).await.unwrap();

        // Verify update
        let found = repo.find_by_id("track-2").await.unwrap().unwrap();
        assert_eq!(found.title, "Updated Track");
    }

    #[tokio::test]
    async fn test_delete_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);
        let track = create_test_track("track-3").await;

        // Insert track
        repo.insert(&track).await.unwrap();

        // Delete track
        let deleted = repo.delete("track-3").await.unwrap();
        assert!(deleted);

        // Verify deletion
        let found = repo.find_by_id("track-3").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_query_with_pagination() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);

        // Insert multiple tracks
        for i in 1..=5 {
            let track = create_test_track(&format!("track-{}", i)).await;
            repo.insert(&track).await.unwrap();
        }

        // Query first page
        let page = repo.query(PageRequest::new(0, 2)).await.unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 5);
        assert_eq!(page.total_pages, 3);
        assert!(page.has_next());
        assert!(!page.has_previous());

        // Query second page
        let page = repo.query(PageRequest::new(1, 2)).await.unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page.has_next());
        assert!(page.has_previous());
    }

    #[tokio::test]
    async fn test_find_by_provider_file() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);
        let track = create_test_track("track-4").await;

        // Insert track
        repo.insert(&track).await.unwrap();

        // Find by provider file
        let found = repo
            .find_by_provider_file("test-provider", "file-track-4")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "track-4");
    }

    #[tokio::test]
    async fn test_search_tracks() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);

        // Insert tracks with different titles
        let mut track1 = create_test_track("track-search-1").await;
        track1.title = "Rock Song".to_string();
        track1.normalized_title = "rock song".to_string();
        repo.insert(&track1).await.unwrap();

        let mut track2 = create_test_track("track-search-2").await;
        track2.title = "Jazz Melody".to_string();
        track2.normalized_title = "jazz melody".to_string();
        repo.insert(&track2).await.unwrap();

        // Search for "rock"
        let results = repo.search("rock", PageRequest::new(0, 10)).await.unwrap();
        assert_eq!(results.items.len(), 1);
        assert_eq!(results.items[0].title, "Rock Song");
    }

    #[tokio::test]
    async fn test_count_tracks() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);

        // Insert tracks
        for i in 1..=3 {
            let track = create_test_track(&format!("track-count-{}", i)).await;
            repo.insert(&track).await.unwrap();
        }

        // Count tracks
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_track_validation() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::new(pool);

        // Create invalid track (empty title)
        let mut track = create_test_track("invalid-1").await;
        track.title = "   ".to_string();

        let result = repo.insert(&track).await;
        assert!(result.is_err());

        // Create invalid track (negative duration)
        let mut track = create_test_track("invalid-2").await;
        track.title = "Valid Title".to_string();
        track.duration_ms = -100;

        let result = repo.insert(&track).await;
        assert!(result.is_err());
    }
}
