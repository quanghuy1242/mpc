//! Album repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Album;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Album repository interface for data access operations
#[async_trait]
pub trait AlbumRepository: Send + Sync {
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
    pool: SqlitePool,
}

impl SqliteAlbumRepository {
    /// Create a new SqliteAlbumRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AlbumRepository for SqliteAlbumRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Album>> {
        let album = query_as::<_, Album>("SELECT * FROM albums WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            ?;

        Ok(album)
    }

    async fn insert(&self, album: &Album) -> Result<()> {
        // Validate before insertion
        album.validate().map_err(|e| {
            LibraryError::InvalidInput { field: "Album".to_string(), message: e }
        })?;

        query(
            r#"
            INSERT INTO albums (
                id, name, normalized_name, artist_id, year,
                artwork_id, track_count, total_duration_ms, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&album.id)
        .bind(&album.name)
        .bind(&album.normalized_name)
        .bind(&album.artist_id)
        .bind(album.year)
        .bind(&album.artwork_id)
        .bind(album.track_count)
        .bind(album.total_duration_ms)
        .bind(album.created_at)
        .bind(album.updated_at)
        .execute(&self.pool)
        .await
        ?;

        Ok(())
    }

    async fn update(&self, album: &Album) -> Result<()> {
        // Validate before update
        album.validate().map_err(|e| {
            LibraryError::InvalidInput { field: "Album".to_string(), message: e }
        })?;

        let result = query(
            r#"
            UPDATE albums
            SET name = ?, normalized_name = ?, artist_id = ?, year = ?,
                artwork_id = ?, track_count = ?, total_duration_ms = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&album.name)
        .bind(&album.normalized_name)
        .bind(&album.artist_id)
        .bind(album.year)
        .bind(&album.artwork_id)
        .bind(album.track_count)
        .bind(album.total_duration_ms)
        .bind(album.updated_at)
        .bind(&album.id)
        .execute(&self.pool)
        .await
        ?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Album".to_string(),
                id: album.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let result = query("DELETE FROM albums WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            ?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Album>> {
        let total = self.count().await?;

        let albums = query_as::<_, Album>(
            "SELECT * FROM albums ORDER BY name ASC LIMIT ? OFFSET ?",
        )
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(albums, total as u64, page_request))
    }

    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Album>> {
        let total: i64 = query_as("SELECT COUNT(*) as count FROM albums WHERE artist_id = ?")
            .bind(artist_id)
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        let albums = query_as::<_, Album>(
            "SELECT * FROM albums WHERE artist_id = ? ORDER BY year DESC, name ASC LIMIT ? OFFSET ?",
        )
        .bind(artist_id)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(albums, total as u64, page_request))
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Album>> {
        // Count matching albums
        let total: i64 = query_as(
            "SELECT COUNT(*) as count FROM albums_fts WHERE albums_fts MATCH ?",
        )
        .bind(search_query)
        .fetch_one(&self.pool)
        .await
        .map(|row: (i64,)| row.0)
        ?;

        // Perform FTS5 search
        let albums = query_as::<_, Album>(
            r#"
            SELECT a.* FROM albums a
            INNER JOIN albums_fts fts ON a.id = fts.id
            WHERE fts MATCH ?
            ORDER BY rank
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(search_query)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(albums, total as u64, page_request))
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM albums")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        Ok(count)
    }

    async fn query_by_year(&self, year: i32, page_request: PageRequest) -> Result<Page<Album>> {
        let total: i64 = query_as("SELECT COUNT(*) as count FROM albums WHERE year = ?")
            .bind(year)
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)
            ?;

        let albums = query_as::<_, Album>(
            "SELECT * FROM albums WHERE year = ? ORDER BY name ASC LIMIT ? OFFSET ?",
        )
        .bind(year)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await
        ?;

        Ok(Page::new(albums, total as u64, page_request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;
    use crate::models::Artist;
    use crate::repositories::artist::{ArtistRepository, SqliteArtistRepository};

    async fn setup_test_pool() -> SqlitePool {
        create_test_pool().await.unwrap()
    }

    #[tokio::test]
    async fn test_insert_and_find_album() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

        // Create test artist first
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Create and insert album
        let album = Album::new("Test Album".to_string(), Some(artist.id.clone()));
        repo.insert(&album).await.unwrap();

        // Find album
        let found = repo.find_by_id(&album.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Album");
    }

    #[tokio::test]
    async fn test_update_album() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

        // Create test artist
        let artist = Artist::new("Test Artist".to_string());
        artist_repo.insert(&artist).await.unwrap();

        // Create and insert album
        let mut album = Album::new("Original Name".to_string(), Some(artist.id.clone()));
        repo.insert(&album).await.unwrap();

        // Update album
        album.name = "Updated Name".to_string();
        album.normalized_name = Album::normalize(&album.name);
        album.updated_at = chrono::Utc::now().timestamp();
        repo.update(&album).await.unwrap();

        // Verify update
        let found = repo.find_by_id(&album.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated Name");
    }

    #[tokio::test]
    async fn test_delete_album() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

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

    #[tokio::test]
    async fn test_query_with_pagination() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

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

    #[tokio::test]
    async fn test_query_by_artist() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

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
        assert!(page.items.iter().all(|a| a.artist_id == Some(artist1.id.clone())));
    }

    #[tokio::test]
    async fn test_count_albums() {
        let pool = setup_test_pool().await;
        let artist_repo = SqliteArtistRepository::new(pool.clone());
        let repo = SqliteAlbumRepository::new(pool);

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

    #[tokio::test]
    async fn test_album_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteAlbumRepository::new(pool);

        // Create album with empty name
        let mut album = Album::new("Test".to_string(), None);
        album.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&album).await;
        assert!(result.is_err());
    }
}
