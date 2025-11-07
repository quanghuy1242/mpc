//! Artwork repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Artwork;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Artwork repository interface for data access operations
#[async_trait]
pub trait ArtworkRepository: Send + Sync {
    /// Find artwork by its ID
    ///
    /// # Returns
    /// - `Ok(Some(artwork))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Artwork>>;

    /// Insert new artwork
    ///
    /// # Errors
    /// Returns error if:
    /// - Artwork with same ID already exists
    /// - Artwork validation fails
    /// - Database error occurs
    async fn insert(&self, artwork: &Artwork) -> Result<()>;

    /// Update existing artwork
    ///
    /// # Errors
    /// Returns error if:
    /// - Artwork does not exist
    /// - Artwork validation fails
    /// - Database error occurs
    async fn update(&self, artwork: &Artwork) -> Result<()>;

    /// Delete artwork by ID
    ///
    /// # Returns
    /// - `Ok(true)` if artwork was deleted
    /// - `Ok(false)` if artwork was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query artworks with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of artworks
    async fn query(&self, page_request: PageRequest) -> Result<Page<Artwork>>;

    /// Find artwork by content hash (for deduplication)
    ///
    /// # Arguments
    /// * `hash` - Content hash
    async fn find_by_hash(&self, hash: &str) -> Result<Option<Artwork>>;

    /// Count total artworks
    async fn count(&self) -> Result<i64>;

    /// Get total storage size of all artworks
    async fn total_size(&self) -> Result<i64>;
}

/// SQLite implementation of ArtworkRepository
pub struct SqliteArtworkRepository {
    pool: SqlitePool,
}

impl SqliteArtworkRepository {
    /// Create a new SqliteArtworkRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ArtworkRepository for SqliteArtworkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Artwork>> {
        let artwork = query_as::<_, Artwork>("SELECT * FROM artworks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(artwork)
    }

    async fn insert(&self, artwork: &Artwork) -> Result<()> {
        // Validate before insertion
        artwork.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artwork".to_string(),
            message: e,
        })?;

        query(
            r#"
            INSERT INTO artworks (
                id, hash, binary_blob, width, height,
                dominant_color, mime_type, file_size, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&artwork.id)
        .bind(&artwork.hash)
        .bind(&artwork.binary_blob)
        .bind(artwork.width)
        .bind(artwork.height)
        .bind(&artwork.dominant_color)
        .bind(&artwork.mime_type)
        .bind(artwork.file_size)
        .bind(artwork.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, artwork: &Artwork) -> Result<()> {
        // Validate before update
        artwork.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artwork".to_string(),
            message: e,
        })?;

        let result = query(
            r#"
            UPDATE artworks
            SET hash = ?, binary_blob = ?, width = ?, height = ?,
                dominant_color = ?, mime_type = ?, file_size = ?
            WHERE id = ?
            "#,
        )
        .bind(&artwork.hash)
        .bind(&artwork.binary_blob)
        .bind(artwork.width)
        .bind(artwork.height)
        .bind(&artwork.dominant_color)
        .bind(&artwork.mime_type)
        .bind(artwork.file_size)
        .bind(&artwork.id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Artwork".to_string(),
                id: artwork.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let result = query("DELETE FROM artworks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Artwork>> {
        let total = self.count().await?;

        let artworks = query_as::<_, Artwork>(
            "SELECT * FROM artworks ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(artworks, total as u64, page_request))
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<Artwork>> {
        let artwork = query_as::<_, Artwork>("SELECT * FROM artworks WHERE hash = ? LIMIT 1")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(artwork)
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM artworks")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        Ok(count)
    }

    async fn total_size(&self) -> Result<i64> {
        let size: Option<i64> = query_as("SELECT SUM(file_size) as total FROM artworks")
            .fetch_one(&self.pool)
            .await
            .map(|row: (Option<i64>,)| row.0)?;

        Ok(size.unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    async fn setup_test_pool() -> SqlitePool {
        create_test_pool().await.unwrap()
    }

    fn create_test_artwork(hash: &str) -> Artwork {
        Artwork::new(
            hash.to_string(),
            vec![1, 2, 3, 4, 5], // Small test image data
            100,
            100,
            "image/jpeg".to_string(),
        )
    }

    #[core_async::test]
    async fn test_insert_and_find_artwork() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtworkRepository::new(pool);

        // Create and insert artwork
        let artwork = create_test_artwork("hash123");
        repo.insert(&artwork).await.unwrap();

        // Find artwork
        let found = repo.find_by_id(&artwork.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.hash, "hash123");
        assert_eq!(found.width, 100);
        assert_eq!(found.height, 100);
    }

    #[core_async::test]
    async fn test_find_by_hash() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtworkRepository::new(pool);

        // Create and insert artwork
        let artwork = create_test_artwork("unique_hash");
        repo.insert(&artwork).await.unwrap();

        // Find by hash
        let found = repo.find_by_hash("unique_hash").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, artwork.id);
    }

    #[core_async::test]
    async fn test_deduplication_by_hash() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtworkRepository::new(pool);

        // Insert first artwork
        let artwork1 = create_test_artwork("duplicate_hash");
        repo.insert(&artwork1).await.unwrap();

        // Check if artwork with same hash exists before inserting
        let existing = repo.find_by_hash("duplicate_hash").await.unwrap();
        assert!(existing.is_some());

        // In a real implementation, we would skip inserting artwork2
        // This demonstrates how deduplication would work
    }

    #[core_async::test]
    async fn test_count_and_total_size() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtworkRepository::new(pool);

        // Initially should be 0
        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);
        let size = repo.total_size().await.unwrap();
        assert_eq!(size, 0);

        // Insert artworks
        for i in 1..=3 {
            let artwork = create_test_artwork(&format!("hash{}", i));
            repo.insert(&artwork).await.unwrap();
        }

        // Count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);

        // Total size should be 15 (3 artworks × 5 bytes each)
        let size = repo.total_size().await.unwrap();
        assert_eq!(size, 15);
    }

    #[core_async::test]
    async fn test_artwork_validation() {
        let pool = setup_test_pool().await;
        let repo = SqliteArtworkRepository::new(pool);

        // Create artwork with empty data
        let mut artwork = create_test_artwork("hash123");
        artwork.binary_blob = vec![];

        // Should fail validation
        let result = repo.insert(&artwork).await;
        assert!(result.is_err());
    }
}
