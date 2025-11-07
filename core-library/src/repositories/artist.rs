//! Artist repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Artist;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Artist repository interface for data access operations
#[async_trait]
pub trait ArtistRepository: Send + Sync {
    /// Find an artist by its ID
    ///
    /// # Returns
    /// - `Ok(Some(artist))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Artist>>;

    /// Insert a new artist
    ///
    /// # Errors
    /// Returns error if:
    /// - Artist with same ID already exists
    /// - Artist validation fails
    /// - Database error occurs
    async fn insert(&self, artist: &Artist) -> Result<()>;

    /// Update an existing artist
    ///
    /// # Errors
    /// Returns error if:
    /// - Artist does not exist
    /// - Artist validation fails
    /// - Database error occurs
    async fn update(&self, artist: &Artist) -> Result<()>;

    /// Delete an artist by ID
    ///
    /// # Returns
    /// - `Ok(true)` if artist was deleted
    /// - `Ok(false)` if artist was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query artists with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of artists
    async fn query(&self, page_request: PageRequest) -> Result<Page<Artist>>;

    /// Search artists by name
    ///
    /// Uses FTS5 full-text search for efficient searching
    ///
    /// # Arguments
    /// * `search_query` - Search query string
    /// * `page_request` - Pagination parameters
    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Artist>>;

    /// Count total artists
    async fn count(&self) -> Result<i64>;

    /// Find artist by exact name
    ///
    /// # Arguments
    /// * `name` - Artist name
    async fn find_by_name(&self, name: &str) -> Result<Option<Artist>>;
}

/// SQLite implementation of ArtistRepository
pub struct SqliteArtistRepository {
    pool: SqlitePool,
}

impl SqliteArtistRepository {
    /// Create a new SqliteArtistRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ArtistRepository for SqliteArtistRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Artist>> {
        let artist = query_as::<_, Artist>("SELECT * FROM artists WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(artist)
    }

    async fn insert(&self, artist: &Artist) -> Result<()> {
        // Validate before insertion
        artist.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artist".to_string(),
            message: e,
        })?;

        query(
            r#"
            INSERT INTO artists (
                id, name, normalized_name, sort_name, bio, country,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&artist.id)
        .bind(&artist.name)
        .bind(&artist.normalized_name)
        .bind(&artist.sort_name)
        .bind(&artist.bio)
        .bind(&artist.country)
        .bind(artist.created_at)
        .bind(artist.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, artist: &Artist) -> Result<()> {
        // Validate before update
        artist.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artist".to_string(),
            message: e,
        })?;

        let result = query(
            r#"
            UPDATE artists
            SET name = ?, normalized_name = ?, sort_name = ?, bio = ?, country = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&artist.name)
        .bind(&artist.normalized_name)
        .bind(&artist.sort_name)
        .bind(&artist.bio)
        .bind(&artist.country)
        .bind(artist.updated_at)
        .bind(&artist.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Artist".to_string(),
                id: artist.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let result = query("DELETE FROM artists WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Artist>> {
        let total = self.count().await?;

        let artists =
            query_as::<_, Artist>("SELECT * FROM artists ORDER BY name ASC LIMIT ? OFFSET ?")
                .bind(page_request.limit())
                .bind(page_request.offset())
                .fetch_all(&self.pool)
                .await?;

        Ok(Page::new(artists, total as u64, page_request))
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Artist>> {
        // Count matching artists
        let total: i64 =
            query_as("SELECT COUNT(*) as count FROM artists_fts WHERE artists_fts MATCH ?")
                .bind(search_query)
                .fetch_one(&self.pool)
                .await
                .map(|row: (i64,)| row.0)?;

        // Perform FTS5 search
        let artists = query_as::<_, Artist>(
            r#"
            SELECT a.* FROM artists a
            INNER JOIN artists_fts fts ON a.id = fts.id
            WHERE fts MATCH ?
            ORDER BY rank
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(search_query)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(artists, total as u64, page_request))
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM artists")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        Ok(count)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Artist>> {
        let normalized = Artist::normalize(name);
        let artist =
            query_as::<_, Artist>("SELECT * FROM artists WHERE normalized_name = ? LIMIT 1")
                .bind(normalized)
                .fetch_optional(&self.pool)
                .await?;

        Ok(artist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    async fn setup_test_pool() -> SqlitePool {
        create_test_pool().await.unwrap()
    }

    #[core_async::test]
    async fn test_insert_and_find_artist() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Create and insert artist
        let mut artist = Artist::new("Test Artist".to_string());
        artist.bio = Some("Legendary artist spanning multiple genres".to_string());
        artist.country = Some("US".to_string());
        repo.insert(&artist).await.unwrap();

        // Find artist
        let found = repo.find_by_id(&artist.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Test Artist");
        assert_eq!(
            found.bio.as_deref(),
            Some("Legendary artist spanning multiple genres")
        );
        assert_eq!(found.country.as_deref(), Some("US"));
    }

    #[core_async::test]
    async fn test_update_artist() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Create and insert artist
        let mut artist = Artist::new("Original Name".to_string());
        artist.bio = Some("Initial bio".to_string());
        artist.country = Some("US".to_string());
        repo.insert(&artist).await.unwrap();

        // Update artist
        artist.name = "Updated Name".to_string();
        artist.normalized_name = Artist::normalize(&artist.name);
        artist.bio = Some("Updated bio".to_string());
        artist.country = Some("GB".to_string());
        artist.updated_at = chrono::Utc::now().timestamp();
        repo.update(&artist).await.unwrap();

        // Verify update
        let found = repo.find_by_id(&artist.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated Name");
        assert_eq!(found.bio.as_deref(), Some("Updated bio"));
        assert_eq!(found.country.as_deref(), Some("GB"));
    }

    #[core_async::test]
    async fn test_delete_artist() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Create and insert artist
        let artist = Artist::new("Test Artist".to_string());
        repo.insert(&artist).await.unwrap();

        // Delete artist
        let deleted = repo.delete(&artist.id).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let found = repo.find_by_id(&artist.id).await.unwrap();
        assert!(found.is_none());
    }

    #[core_async::test]
    async fn test_query_with_pagination() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Insert multiple artists
        for i in 1..=5 {
            let artist = Artist::new(format!("Artist {}", i));
            repo.insert(&artist).await.unwrap();
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
    async fn test_find_by_name() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Create and insert artist
        let artist = Artist::new("Test Artist".to_string());
        repo.insert(&artist).await.unwrap();

        // Find by exact name
        let found = repo.find_by_name("Test Artist").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, artist.id);

        // Find by different case (should work due to normalization)
        let found = repo.find_by_name("TEST ARTIST").await.unwrap();
        assert!(found.is_some());
    }

    #[core_async::test]
    async fn test_count_artists() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Initially should be 0
        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);

        // Insert artists
        for i in 1..=3 {
            let artist = Artist::new(format!("Artist {}", i));
            repo.insert(&artist).await.unwrap();
        }

        // Count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[core_async::test]
    async fn test_artist_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtistRepository::new(pool);

        // Create artist with empty name
        let mut artist = Artist::new("Test".to_string());
        artist.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&artist).await;
        assert!(result.is_err());
    }
}
