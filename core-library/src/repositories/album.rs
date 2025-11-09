//! Album repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Album;
use crate::repositories::{Page, PageRequest, PlatformArc};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;

/// Album repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait AlbumRepository: PlatformSendSync {
    /// Find an album by its ID
    ///
    /// # Returns
    /// - `Ok(Some(album))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Album>>;

    /// Insert a new album
    ///
    /// # Errors
    /// Returns error if:
    /// - Album with same ID already exists
    /// - Album validation fails
    /// - Database error occurs
    async fn insert(&self, album: &Album) -> Result<()>;

    /// Update an existing album
    ///
    /// # Errors
    /// Returns error if:
    /// - Album does not exist
    /// - Album validation fails
    /// - Database error occurs
    async fn update(&self, album: &Album) -> Result<()>;

    /// Delete an album by ID
    ///
    /// # Returns
    /// - `Ok(true)` if album was deleted
    /// - `Ok(false)` if album was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query albums with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of albums
    async fn query(&self, page_request: PageRequest) -> Result<Page<Album>>;

    /// Query albums by artist with pagination
    ///
    /// # Arguments
    /// * `artist_id` - Artist identifier
    /// * `page_request` - Pagination parameters
    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Album>>;

    /// Search albums by name
    ///
    /// Uses FTS5 full-text search for efficient searching
    ///
    /// # Arguments
    /// * `search_query` - Search query string
    /// * `page_request` - Pagination parameters
    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Album>>;

    /// Count total albums
    async fn count(&self) -> Result<i64>;

    /// Find albums by year
    ///
    /// # Arguments
    /// * `year` - Release year
    /// * `page_request` - Pagination parameters
    async fn query_by_year(&self, year: i32, page_request: PageRequest) -> Result<Page<Album>>;
}

/// SQLite implementation of AlbumRepository
pub struct SqliteAlbumRepository {
    adapter: PlatformArc<dyn DatabaseAdapter>,
}

impl SqliteAlbumRepository {
    /// Create a new repository using the provided database adapter.
    pub fn new(adapter: PlatformArc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    fn validate_album(album: &Album) -> Result<()> {
        album.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "Album".to_string(),
            message: msg,
        })
    }

    fn insert_params(album: &Album) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(album.id.clone()),
            QueryValue::Text(album.name.clone()),
            QueryValue::Text(album.normalized_name.clone()),
            opt_text(&album.artist_id),
            opt_i32(album.year),
            opt_text(&album.genre),
            opt_text(&album.artwork_id),
            QueryValue::Integer(album.track_count),
            QueryValue::Integer(album.total_duration_ms),
            QueryValue::Integer(album.created_at),
            QueryValue::Integer(album.updated_at),
        ]
    }

    fn update_params(album: &Album) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(album.name.clone()),
            QueryValue::Text(album.normalized_name.clone()),
            opt_text(&album.artist_id),
            opt_i32(album.year),
            opt_text(&album.genre),
            opt_text(&album.artwork_id),
            QueryValue::Integer(album.track_count),
            QueryValue::Integer(album.total_duration_ms),
            QueryValue::Integer(album.updated_at),
        ];
        params.push(QueryValue::Text(album.id.clone()));
        params
    }

    async fn fetch_albums(&self, sql: &str, params: Vec<QueryValue>) -> Result<Vec<Album>> {
        let rows = self.adapter.query(sql, &params).await?;
        rows.into_iter().map(|row| row_to_album(&row)).collect()
    }

    async fn fetch_optional_album(
        &self,
        sql: &str,
        params: Vec<QueryValue>,
    ) -> Result<Option<Album>> {
        let row = self.adapter.query_one_optional(sql, &params).await?;
        row.map(|row| row_to_album(&row)).transpose()
    }

    async fn paginate(
        &self,
        count_sql: &str,
        count_params: Vec<QueryValue>,
        data_sql: &str,
        mut data_params: Vec<QueryValue>,
        request: PageRequest,
    ) -> Result<Page<Album>> {
        let total = self.count_with(count_sql, count_params).await?;
        data_params.push(QueryValue::Integer(request.limit() as i64));
        data_params.push(QueryValue::Integer(request.offset() as i64));
        let items = self.fetch_albums(data_sql, data_params).await?;
        Ok(Page::new(items, total as u64, request))
    }

    async fn count_with(&self, sql: &str, params: Vec<QueryValue>) -> Result<i64> {
        let row = self.adapter.query_one(sql, &params).await?;
        row.get("count")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| missing_column("count"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SqliteAlbumRepository {
    /// Convenience constructor for native targets using an existing `sqlx` pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(PlatformArc::new(SqliteAdapter::from_pool(pool)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl AlbumRepository for SqliteAlbumRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Album>> {
        self.fetch_optional_album(
            "SELECT * FROM albums WHERE id = ?",
            vec![QueryValue::Text(id.to_string())],
        )
        .await
    }

    async fn insert(&self, album: &Album) -> Result<()> {
        Self::validate_album(album)?;
        self.adapter
            .execute(
                r#"
                INSERT INTO albums (
                    id, name, normalized_name, artist_id, year,
                    genre, artwork_id, track_count, total_duration_ms, created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                &Self::insert_params(album),
            )
            .await?;
        Ok(())
    }

    async fn update(&self, album: &Album) -> Result<()> {
        Self::validate_album(album)?;
        let affected = self
            .adapter
            .execute(
                r#"
                UPDATE albums
                SET name = ?, normalized_name = ?, artist_id = ?, year = ?,
                    genre = ?, artwork_id = ?, track_count = ?, total_duration_ms = ?, updated_at = ?
                WHERE id = ?
                "#,
                &Self::update_params(album),
            )
            .await?;
        if affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Album".to_string(),
                id: album.id.clone(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let affected = self
            .adapter
            .execute(
                "DELETE FROM albums WHERE id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Album>> {
        self.paginate(
            "SELECT COUNT(*) as count FROM albums",
            vec![],
            "SELECT * FROM albums ORDER BY name ASC LIMIT ? OFFSET ?",
            vec![],
            page_request,
        )
        .await
    }

    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Album>> {
        let params = vec![QueryValue::Text(artist_id.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM albums WHERE artist_id = ?",
            params.clone(),
            "SELECT * FROM albums WHERE artist_id = ? ORDER BY year DESC, name ASC LIMIT ? OFFSET ?",
            params,
            page_request,
        )
        .await
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Album>> {
        let params = vec![QueryValue::Text(search_query.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM albums_fts WHERE albums_fts MATCH ?",
            params.clone(),
            r#"
            SELECT a.* FROM albums a
            INNER JOIN albums_fts fts ON a.id = fts.album_id
            WHERE fts MATCH ?
            ORDER BY rank
            LIMIT ? OFFSET ?
            "#,
            params,
            page_request,
        )
        .await
    }

    async fn count(&self) -> Result<i64> {
        self.count_with("SELECT COUNT(*) as count FROM albums", vec![])
            .await
    }

    async fn query_by_year(&self, year: i32, page_request: PageRequest) -> Result<Page<Album>> {
        let params = vec![QueryValue::Integer(year as i64)];
        self.paginate(
            "SELECT COUNT(*) as count FROM albums WHERE year = ?",
            params.clone(),
            "SELECT * FROM albums WHERE year = ? ORDER BY name ASC LIMIT ? OFFSET ?",
            params,
            page_request,
        )
        .await
    }
}

pub(crate) fn row_to_album(row: &QueryRow) -> Result<Album> {
    Ok(Album {
        id: get_string(row, "id")?,
        name: get_string(row, "name")?,
        normalized_name: get_string(row, "normalized_name")?,
        artist_id: get_optional_string(row, "artist_id")?,
        year: get_optional_i32(row, "year")?,
        genre: get_optional_string(row, "genre")?,
        artwork_id: get_optional_string(row, "artwork_id")?,
        track_count: get_i64(row, "track_count")?,
        total_duration_ms: get_i64(row, "total_duration_ms")?,
        created_at: get_i64(row, "created_at")?,
        updated_at: get_i64(row, "updated_at")?,
    })
}

fn get_string(row: &QueryRow, key: &str) -> Result<String> {
    row.get(key)
        .and_then(|value| value.as_string())
        .ok_or_else(|| missing_column(key))
}

fn get_optional_string(row: &QueryRow, key: &str) -> Result<Option<String>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(value.as_string().ok_or_else(|| missing_column(key))?),
    })
}

fn get_i64(row: &QueryRow, key: &str) -> Result<i64> {
    row.get(key)
        .and_then(|value| value.as_i64())
        .ok_or_else(|| missing_column(key))
}

fn get_optional_i64(row: &QueryRow, key: &str) -> Result<Option<i64>> {
    Ok(match row.get(key) {
        Some(QueryValue::Null) | None => None,
        Some(value) => Some(value.as_i64().ok_or_else(|| missing_column(key))?),
    })
}

fn get_i32(row: &QueryRow, key: &str) -> Result<i32> {
    Ok(get_i64(row, key)? as i32)
}

fn get_optional_i32(row: &QueryRow, key: &str) -> Result<Option<i32>> {
    Ok(get_optional_i64(row, key)?.map(|value| value as i32))
}

fn missing_column(column: &str) -> LibraryError {
    LibraryError::InvalidInput {
        field: column.to_string(),
        message: "missing column in result set".to_string(),
    }
}

fn opt_text(value: &Option<String>) -> QueryValue {
    value
        .as_ref()
        .map(|v| QueryValue::Text(v.clone()))
        .unwrap_or(QueryValue::Null)
}

fn opt_i32(value: Option<i32>) -> QueryValue {
    value
        .map(|v| QueryValue::Integer(v as i64))
        .unwrap_or(QueryValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;
    use crate::models::Artist;
    use crate::repositories::artist::{ArtistRepository, SqliteArtistRepository};

    #[core_async::test]
    async fn test_insert_and_find_album() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create test artist first
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Create and insert album
        let mut album = Album::new("Test Album".to_string(), Some(artist.id.clone()));
        album.genre = Some("Rock".to_string());
        repo.insert(&album).await.unwrap();

        // Find album
        let found = repo.find_by_id(&album.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Test Album");
        assert_eq!(found.genre.as_deref(), Some("Rock"));
    }

    #[core_async::test]
    async fn test_update_album() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create test artist
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Create and insert album
        let mut album = Album::new("Original Name".to_string(), Some(artist.id.clone()));
        album.genre = Some("Rock".to_string());
        repo.insert(&album).await.unwrap();

        // Update album
        album.name = "Updated Name".to_string();
        album.normalized_name = Album::normalize(&album.name);
        album.genre = Some("Indie".to_string());
        album.updated_at = chrono::Utc::now().timestamp();
        repo.update(&album).await.unwrap();

        // Verify update
        let found = repo.find_by_id(&album.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated Name");
        assert_eq!(found.genre.as_deref(), Some("Indie"));
    }

    #[core_async::test]
    async fn test_delete_album() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create test artist
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Create and insert album
        let album = Album::new("Test Album".to_string(), Some(artist.id.clone()));
        repo.insert(&album).await.unwrap();

        // Delete album
        let deleted = repo.delete(&album.id).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let found = repo.find_by_id(&album.id).await.unwrap();
        assert!(found.is_none());
    }

    #[core_async::test]
    async fn test_query_with_pagination() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create test artist
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Insert multiple albums
        for i in 1..=5 {
            let album = Album::new(format!("Album {}", i), Some(artist.id.clone()));
            repo.insert(&album).await.unwrap();
        }

        // Query with pagination (page 1 = second page, 0-indexed)
        let page = repo
            .query(PageRequest {
                page: 1,
                page_size: 3,
            })
            .await
            .unwrap();

        assert_eq!(page.items.len(), 2); // Page 1 should have 2 items (items 3-4 of 5 total)
        assert_eq!(page.total, 5);
        assert_eq!(page.total_pages, 2);
    }

    #[core_async::test]
    async fn test_query_by_artist() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create two artists
        let artist1 = Artist::new("Artist 1".to_string());
        let artist2 = Artist::new("Artist 2".to_string());
        artist_repo.insert(&artist1).await.unwrap();
        artist_repo.insert(&artist2).await.unwrap();

        // Create albums for each artist
        let album1 = Album::new("Album 1".to_string(), Some(artist1.id.clone()));
        let album2 = Album::new("Album 2".to_string(), Some(artist1.id.clone()));
        let album3 = Album::new("Album 3".to_string(), Some(artist2.id.clone()));

        repo.insert(&album1).await.unwrap();
        repo.insert(&album2).await.unwrap();
        repo.insert(&album3).await.unwrap();

        // Query albums by artist1
        let page = repo
            .query_by_artist(&artist1.id, PageRequest::default())
            .await
            .unwrap();

        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .all(|a| a.artist_id == Some(artist1.id.clone())));
    }

    #[core_async::test]
    async fn test_count_albums() {
        let pool = create_test_pool().await.unwrap();
        let artist_repo = SqliteArtistRepository::from_pool(pool.clone());
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create test artist
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Initially should be 0
        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);

        // Insert albums
        for i in 1..=3 {
            let album = Album::new(format!("Album {}", i), Some(artist.id.clone()));
            repo.insert(&album).await.unwrap();
        }

        // Count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[core_async::test]
    async fn test_album_validation() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteAlbumRepository::from_pool(pool);

        // Create album with empty name
        let mut album = Album::new("Test".to_string(), None);
        album.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&album).await;
        assert!(result.is_err());
    }
}
