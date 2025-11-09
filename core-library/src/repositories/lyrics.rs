//! Lyrics repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Lyrics;
use crate::repositories::{Page, PageRequest, PlatformArc};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::error::BridgeError;
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;

/// Lyrics repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait LyricsRepository: PlatformSendSync {
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
    adapter: PlatformArc<dyn DatabaseAdapter>,
}

impl SqliteLyricsRepository {
    /// Create a new lyrics repository with the given database adapter
    pub fn new(adapter: PlatformArc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Create a new lyrics repository from a SQLite connection pool (native only)
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::SqliteAdapter;
        Self::new(PlatformArc::new(SqliteAdapter::from_pool(pool)))
    }

    // Helper functions for extracting values from QueryRow
    fn get_string(row: &QueryRow, col: &str) -> Result<String> {
        row.get(col)
            .and_then(|v| match v {
                QueryValue::Text(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| Self::missing_column(col))
    }

    fn get_optional_string(row: &QueryRow, col: &str) -> Option<String> {
        row.get(col).and_then(|v| match v {
            QueryValue::Text(s) => Some(s.clone()),
            QueryValue::Null => None,
            _ => None,
        })
    }

    fn get_i64(row: &QueryRow, col: &str) -> Result<i64> {
        row.get(col)
            .and_then(|v| match v {
                QueryValue::Integer(i) => Some(*i),
                _ => None,
            })
            .ok_or_else(|| Self::missing_column(col))
    }

    fn missing_column(col: &str) -> LibraryError {
        LibraryError::Bridge(BridgeError::DatabaseError(format!(
            "Missing or invalid column: {}",
            col
        )))
    }

    // Helper to build an optional text parameter
    fn opt_text(value: &Option<String>) -> QueryValue {
        match value {
            Some(v) => QueryValue::Text(v.clone()),
            None => QueryValue::Null,
        }
    }

    // Helper to convert a QueryRow into Lyrics
    fn row_to_lyrics(row: QueryRow) -> Result<Lyrics> {
        Ok(Lyrics {
            track_id: Self::get_string(&row, "track_id")?,
            source: Self::get_string(&row, "source")?,
            synced: Self::get_i64(&row, "synced")?,
            body: Self::get_string(&row, "body")?,
            language: Self::get_optional_string(&row, "language"),
            last_checked_at: Self::get_i64(&row, "last_checked_at")?,
            created_at: Self::get_i64(&row, "created_at")?,
            updated_at: Self::get_i64(&row, "updated_at")?,
        })
    }

    // Helper to build insert parameters
    fn insert_params(lyrics: &Lyrics) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(lyrics.track_id.clone()),
            QueryValue::Text(lyrics.source.clone()),
            QueryValue::Integer(lyrics.synced),
            QueryValue::Text(lyrics.body.clone()),
            Self::opt_text(&lyrics.language),
            QueryValue::Integer(lyrics.last_checked_at),
            QueryValue::Integer(lyrics.created_at),
            QueryValue::Integer(lyrics.updated_at),
        ]
    }

    // Helper to build update parameters
    fn update_params(lyrics: &Lyrics) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(lyrics.source.clone()),
            QueryValue::Integer(lyrics.synced),
            QueryValue::Text(lyrics.body.clone()),
            Self::opt_text(&lyrics.language),
            QueryValue::Integer(lyrics.last_checked_at),
            QueryValue::Integer(lyrics.updated_at),
        ];
        // Add track_id for WHERE clause
        params.push(QueryValue::Text(lyrics.track_id.clone()));
        params
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl LyricsRepository for SqliteLyricsRepository {
    async fn find_by_track_id(&self, track_id: &str) -> Result<Option<Lyrics>> {
        let sql = "SELECT * FROM lyrics WHERE track_id = ?";
        let params = vec![QueryValue::Text(track_id.to_string())];

        match self.adapter.query_one_optional(sql, &params).await? {
            Some(row) => Ok(Some(Self::row_to_lyrics(row)?)),
            None => Ok(None),
        }
    }

    async fn insert(&self, lyrics: &Lyrics) -> Result<()> {
        // Validate before insertion
        lyrics.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Lyrics".to_string(),
            message: e,
        })?;

        let sql = r#"
            INSERT INTO lyrics (
                track_id, source, synced, body, language,
                last_checked_at, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let params = Self::insert_params(lyrics);
        self.adapter.execute(sql, &params).await?;

        Ok(())
    }

    async fn update(&self, lyrics: &Lyrics) -> Result<()> {
        // Validate before update
        lyrics.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Lyrics".to_string(),
            message: e,
        })?;

        let sql = r#"
            UPDATE lyrics
            SET source = ?, synced = ?, body = ?, language = ?,
                last_checked_at = ?, updated_at = ?
            WHERE track_id = ?
        "#;

        let params = Self::update_params(lyrics);
        let rows_affected = self.adapter.execute(sql, &params).await?;

        if rows_affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Lyrics".to_string(),
                id: lyrics.track_id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, track_id: &str) -> Result<bool> {
        let sql = "DELETE FROM lyrics WHERE track_id = ?";
        let params = vec![QueryValue::Text(track_id.to_string())];

        let rows_affected = self.adapter.execute(sql, &params).await?;

        Ok(rows_affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Lyrics>> {
        let total = self.count().await?;

        let sql = "SELECT * FROM lyrics ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let params = vec![
            QueryValue::Integer(page_request.limit() as i64),
            QueryValue::Integer(page_request.offset() as i64),
        ];

        let rows = self.adapter.query(sql, &params).await?;

        let lyrics = rows
            .into_iter()
            .map(Self::row_to_lyrics)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn query_synced(&self, page_request: PageRequest) -> Result<Page<Lyrics>> {
        let total = self.count_synced().await?;

        let sql = "SELECT * FROM lyrics WHERE synced = 1 ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let params = vec![
            QueryValue::Integer(page_request.limit() as i64),
            QueryValue::Integer(page_request.offset() as i64),
        ];

        let rows = self.adapter.query(sql, &params).await?;

        let lyrics = rows
            .into_iter()
            .map(Self::row_to_lyrics)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn query_by_source(
        &self,
        source: &str,
        page_request: PageRequest,
    ) -> Result<Page<Lyrics>> {
        let sql_count = "SELECT COUNT(*) as count FROM lyrics WHERE source = ?";
        let params_count = vec![QueryValue::Text(source.to_string())];

        let row = self.adapter.query_one(sql_count, &params_count).await?;

        let total = Self::get_i64(&row, "count")?;

        let sql = "SELECT * FROM lyrics WHERE source = ? ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let params = vec![
            QueryValue::Text(source.to_string()),
            QueryValue::Integer(page_request.limit() as i64),
            QueryValue::Integer(page_request.offset() as i64),
        ];

        let rows = self.adapter.query(sql, &params).await?;

        let lyrics = rows
            .into_iter()
            .map(Self::row_to_lyrics)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(lyrics, total as u64, page_request))
    }

    async fn count(&self) -> Result<i64> {
        let sql = "SELECT COUNT(*) as count FROM lyrics";
        let row = self.adapter.query_one(sql, &[]).await?;

        Self::get_i64(&row, "count")
    }

    async fn count_synced(&self) -> Result<i64> {
        let sql = "SELECT COUNT(*) as count FROM lyrics WHERE synced = 1";
        let row = self.adapter.query_one(sql, &[]).await?;

        Self::get_i64(&row, "count")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;
    use crate::models::{Artist, Track};
    use crate::repositories::artist::{ArtistRepository, SqliteArtistRepository};
    use crate::repositories::track::{SqliteTrackRepository, TrackRepository};

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

        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let track_repo = SqliteTrackRepository::from_pool(pool.clone());

        // Create artist with unique ID and name
        let artist_id = format!("artist-{}", uuid::Uuid::new_v4());
        let artist_name = format!("Test Artist {}", uuid::Uuid::new_v4());
        let artist = Artist {
            id: artist_id.clone(),
            name: artist_name.clone(),
            normalized_name: artist_name.to_lowercase(),
            sort_name: None,
            bio: None,
            country: None,
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

    #[core_async::test]
    async fn test_insert_and_find_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());
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
        assert_eq!(found.updated_at, lyrics.updated_at);
    }

    #[core_async::test]
    async fn test_update_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());
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
        lyrics.updated_at = lyrics.last_checked_at;
        repo.update(&lyrics).await.unwrap();

        // Verify update
        let found = repo.find_by_track_id(&track.id).await.unwrap().unwrap();
        assert_eq!(found.body, "Updated lyrics");
        assert_eq!(found.updated_at, lyrics.updated_at);
    }

    #[core_async::test]
    async fn test_delete_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());
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

    #[core_async::test]
    async fn test_query_synced() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());

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

    #[core_async::test]
    async fn test_query_by_source() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());

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

    #[core_async::test]
    async fn test_count_lyrics() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());

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
            let lyrics = Lyrics::new(track.id.clone(), "manual".to_string(), is_synced, body);
            repo.insert(&lyrics).await.unwrap();
        }

        // Total count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);

        // Synced count should be 1
        let synced_count = repo.count_synced().await.unwrap();
        assert_eq!(synced_count, 1);
    }

    #[core_async::test]
    async fn test_lyrics_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteLyricsRepository::from_pool(pool.clone());
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
