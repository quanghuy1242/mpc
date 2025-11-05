//! Lyrics repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Lyrics;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Lyrics repository interface for data access operations
#[async_trait]
pub trait LyricsRepository: Send + Sync {
    /// Find lyrics by track ID
    ///
    /// # Returns
    /// - `Ok(Some(lyrics))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_track_id(&self, track_id: &str) -> Result<Option<Lyrics>>;

    /// Insert new lyrics
    ///
    /// # Errors
    /// Returns error if:
    /// - Lyrics for track already exist
    /// - Lyrics validation fails
    /// - Database error occurs
    async fn insert(&self, lyrics: &Lyrics) -> Result<()>;

    /// Update existing lyrics
    ///
    /// # Errors
    /// Returns error if:
    /// - Lyrics do not exist
    /// - Lyrics validation fails
    /// - Database error occurs
    async fn update(&self, lyrics: &Lyrics) -> Result<()>;

    /// Delete lyrics by track ID
    ///
    /// # Returns
    /// - `Ok(true)` if lyrics were deleted
    /// - `Ok(false)` if lyrics were not found
    async fn delete(&self, track_id: &str) -> Result<bool>;

    /// Query lyrics with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of lyrics
    async fn query(&self, page_request: PageRequest) -> Result<Page<Lyrics>>;

    /// Query synced lyrics with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    async fn query_synced(&self, page_request: PageRequest) -> Result<Page<Lyrics>>;

    /// Query lyrics by source
    ///
    /// # Arguments
    /// * `source` - Lyrics source (e.g., "lrclib", "musixmatch")
    /// * `page_request` - Pagination parameters
    async fn query_by_source(
        &self,
        source: &str,
        page_request: PageRequest,
    ) -> Result<Page<Lyrics>>;

    /// Count total lyrics
    async fn count(&self) -> Result<i64>;

    /// Count synced lyrics
    async fn count_synced(&self) -> Result<i64>;
}

/// SQLite implementation of LyricsRepository
pub struct SqliteLyricsRepository {
    pool: SqlitePool,
}

impl SqliteLyricsRepository {
    /// Create a new SqliteLyricsRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LyricsRepository for SqliteLyricsRepository {
    async fn find_by_track_id(&self, track_id: &str) -> Result<Option<Lyrics>> {
        let lyrics = query_as::<_, Lyrics>("SELECT * FROM lyrics WHERE track_id = ?")
            .bind(track_id)
            .fetch_optional(&self.pool)
            .await
            ?;

        Ok(lyrics)
    }

    async fn insert(&self, lyrics: &Lyrics) -> Result<()> {
        // Validate before insertion
        lyrics.validate().map_err(|e| {
            LibraryError::InvalidInput { field: "Lyrics".to_string(), message: e }
        })?;

        query(
            r#"
            INSERT INTO lyrics (
                track_id, source, synced, body, language,
                last_checked_at, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&lyrics.track_id)
        .bind(&lyrics.source)
        .bind(lyrics.synced)
        .bind(&lyrics.body)
        .bind(&lyrics.language)
        .bind(lyrics.last_checked_at)
        .bind(lyrics.created_at)
        .execute(&self.pool)
        .await
        ?;

        Ok(())
    }

    async fn update(&self, lyrics: &Lyrics) -> Result<()> {
        // Validate before update
        lyrics.validate().map_err(|e| {
            LibraryError::InvalidInput { field: "Lyrics".to_string(), message: e }
        })?;

        let result = query(
            r#"
            UPDATE lyrics
            SET source = ?, synced = ?, body = ?, language = ?,
                last_checked_at = ?
            WHERE track_id = ?
            "#,
        )
        .bind(&lyrics.source)
        .bind(lyrics.synced)
        .bind(&lyrics.body)
        .bind(&lyrics.language)
        .bind(lyrics.last_checked_at)
        .bind(&lyrics.track_id)
        .execute(&self.pool)
        .await
        ?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound { entity_type: "Lyrics".to_string(), id: lyrics.track_id.clone() });
        }

        Ok(())
    }

    async fn delete(&self, track_id: &str) -> Result<bool> {
        let result = query("DELETE FROM lyrics WHERE track_id = ?")
            .bind(track_id)
            .execute(&self.pool)
            .await
            ?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Lyrics>> {
        let total = self.count().await?;

        let lyrics = query_as::<_, Lyrics>(
            "SELECT * FROM lyrics ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn query_synced(&self, page_request: PageRequest) -> Result<Page<Lyrics>> {
        let total = self.count_synced().await?;

        let lyrics = query_as::<_, Lyrics>(
            "SELECT * FROM lyrics WHERE synced = 1 ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn query_by_source(
        &self,
        source: &str,
        page_request: PageRequest,
    ) -> Result<Page<Lyrics>> {
        let total: i64 = query_as("SELECT COUNT(*) as count FROM lyrics WHERE source = ?")
            .bind(source)
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        let lyrics = query_as::<_, Lyrics>(
            "SELECT * FROM lyrics WHERE source = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(source)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM lyrics")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        Ok(count)
    }

    async fn count_synced(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM lyrics WHERE synced = 1")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;
    use crate::models::{Artist, Track};
    use crate::repositories::artist::{ArtistRepository, SqliteArtistRepository};
    use crate::repositories::track::{TrackRepository, SqliteTrackRepository};

    async fn setup_test_pool() -> SqlitePool {
        create_test_pool().await.unwrap()
    }

    async fn create_test_track(pool: &SqlitePool) -> Track {
        // Create provider first (required by foreign key)
        // Use unique provider ID to avoid conflicts when tests run in parallel
        let provider_id = format!("test-provider-{}", uuid::Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO providers (id, type, display_name, profile_id, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&provider_id)
        .bind("GoogleDrive")
        .bind("Test Provider")
        .bind("test-profile")
        .bind(1699200000)
        .execute(pool)
        .await
        .unwrap();

        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let track_repo = SqliteTrackRepository::new(pool.clone());

        // Create artist with unique ID and name
        let artist_id = format!("artist-{}", uuid::Uuid::new_v4());
        let artist_name = format!("Test Artist {}", uuid::Uuid::new_v4());
        let artist = Artist {
            id: artist_id.clone(),
            name: artist_name.clone(),
            normalized_name: artist_name.to_lowercase(),
            sort_name: None,
            created_at: 1699200000,
            updated_at: 1699200000,
        };
        artist_repo.insert(&artist).await.unwrap();

        // Create track with unique ID
        let track_id = format!("track-{}", uuid::Uuid::new_v4());
        let track = Track {
            id: track_id.clone(),
            provider_id: provider_id.clone(),
            provider_file_id: "file-1".to_string(),
            provider_modified_at: Some(1699200000),
            hash: Some("test-hash".to_string()),
            title: "Test Track".to_string(),
            normalized_title: "test track".to_string(),
            album_id: None,
            artist_id: Some(artist.id.clone()),
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
        };
        track_repo.insert(&track).await.unwrap();

        track
    }

    #[tokio::test]
    async fn test_insert_and_find_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());
        let track = create_test_track(&pool).await;

        // Create and insert lyrics
        let lyrics = Lyrics::new(
            track.id.clone(),
            "manual".to_string(),
            false,
            "This is a test song\nWith some lyrics".to_string(),
        );
        repo.insert(&lyrics).await.unwrap();

        // Find lyrics
        let found = repo.find_by_track_id(&track.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.track_id, track.id);
        assert_eq!(found.synced, 0); // 0 = false (plain text lyrics)
    }

    #[tokio::test]
    async fn test_update_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());
        let track = create_test_track(&pool).await;

        // Create and insert lyrics
        let mut lyrics = Lyrics::new(
            track.id.clone(),
            "manual".to_string(),
            false,
            "Original lyrics".to_string(),
        );
        repo.insert(&lyrics).await.unwrap();

        // Update lyrics
        lyrics.body = "Updated lyrics".to_string();
        lyrics.last_checked_at = chrono::Utc::now().timestamp();
        repo.update(&lyrics).await.unwrap();

        // Verify update
        let found = repo.find_by_track_id(&track.id).await.unwrap().unwrap();
        assert_eq!(found.body, "Updated lyrics");
    }

    #[tokio::test]
    async fn test_delete_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());
        let track = create_test_track(&pool).await;

        // Create and insert lyrics
        let lyrics = Lyrics::new(
            track.id.clone(),
            "manual".to_string(),
            false,
            "Test lyrics".to_string(),
        );
        repo.insert(&lyrics).await.unwrap();

        // Delete lyrics
        let deleted = repo.delete(&track.id).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let found = repo.find_by_track_id(&track.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_query_synced() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());

        // Create tracks and lyrics
        let track1 = create_test_track(&pool).await;
        let track2 = create_test_track(&pool).await;

        let lyrics1 = Lyrics::new(
            track1.id.clone(),
            "lrclib".to_string(),
            true, // synced
            "[00:01.00]Test lyrics".to_string(),
        );
        let lyrics2 = Lyrics::new(
            track2.id.clone(),
            "manual".to_string(),
            false, // not synced
            "Plain text lyrics".to_string(),
        );

        repo.insert(&lyrics1).await.unwrap();
        repo.insert(&lyrics2).await.unwrap();

        // Query synced lyrics
        let page = repo.query_synced(PageRequest::default()).await.unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].synced, 1); // 1 = true (synced/LRC lyrics)
    }

    #[tokio::test]
    async fn test_query_by_source() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());

        // Create tracks and lyrics from different sources
        let track1 = create_test_track(&pool).await;
        let track2 = create_test_track(&pool).await;

        let lyrics1 = Lyrics::new(
            track1.id.clone(),
            "lrclib".to_string(),
            true,
            "[00:01.00]Test".to_string(),
        );
        let lyrics2 = Lyrics::new(
            track2.id.clone(),
            "manual".to_string(),
            false,
            "Manual lyrics".to_string(),
        );

        repo.insert(&lyrics1).await.unwrap();
        repo.insert(&lyrics2).await.unwrap();

        // Query by source
        let page = repo
            .query_by_source("lrclib", PageRequest::default())
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].source, "lrclib");
    }

    #[tokio::test]
    async fn test_count_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());

        // Initially should be 0
        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);

        // Insert lyrics
        for i in 1..=3 {
            let track = create_test_track(&pool).await;
            let is_synced = i % 2 == 0;
            let body = if is_synced {
                format!("[00:00.00]Lyrics {}", i) // LRC format for synced
            } else {
                format!("Lyrics {}", i) // Plain text for unsynced
            };
            let lyrics = Lyrics::new(
                track.id.clone(),
                "manual".to_string(),
                is_synced,
                body,
            );
            repo.insert(&lyrics).await.unwrap();
        }

        // Total count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);

        // Synced count should be 1
        let synced_count = repo.count_synced().await.unwrap();
        assert_eq!(synced_count, 1);
    }

    #[tokio::test]
    async fn test_lyrics_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::new(pool.clone());
        let track = create_test_track(&pool).await;

        // Create lyrics with empty body
        let mut lyrics = Lyrics::new(
            track.id.clone(),
            "manual".to_string(),
            false,
            "Test".to_string(),
        );
        lyrics.body = "".to_string();

        // Should fail validation
        let result = repo.insert(&lyrics).await;
        assert!(result.is_err());
    }
}
