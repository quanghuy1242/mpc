//! Artwork repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Artwork;
use crate::repositories::{Page, PageRequest};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::error::BridgeError;
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;
use std::sync::Arc;

/// Artwork repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ArtworkRepository: PlatformSendSync {
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

/// SQLite implementation of the artwork repository
pub struct SqliteArtworkRepository {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteArtworkRepository {
    /// Create a new artwork repository with the given database adapter
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Create a new artwork repository from a SQLite connection pool (native only)
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::SqliteAdapter;
        Self::new(Arc::new(SqliteAdapter::from_pool(pool)))
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

    fn get_optional_i64(row: &QueryRow, col: &str) -> Option<i64> {
        row.get(col).and_then(|v| match v {
            QueryValue::Integer(i) => Some(*i),
            QueryValue::Null => None,
            _ => None,
        })
    }

    fn get_i32(row: &QueryRow, col: &str) -> Result<i32> {
        row.get(col)
            .and_then(|v| match v {
                QueryValue::Integer(i) => Some(*i as i32),
                _ => None,
            })
            .ok_or_else(|| Self::missing_column(col))
    }

    fn get_optional_i32(row: &QueryRow, col: &str) -> Option<i32> {
        row.get(col).and_then(|v| match v {
            QueryValue::Integer(i) => Some(*i as i32),
            QueryValue::Null => None,
            _ => None,
        })
    }

    fn get_blob(row: &QueryRow, col: &str) -> Result<Vec<u8>> {
        row.get(col)
            .and_then(|v| match v {
                QueryValue::Blob(b) => Some(b.clone()),
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

    // Helper to build an optional i32 parameter
    fn opt_i32(value: Option<i32>) -> QueryValue {
        match value {
            Some(v) => QueryValue::Integer(v as i64),
            None => QueryValue::Null,
        }
    }

    // Helper to convert a QueryRow into an Artwork
    fn row_to_artwork(row: QueryRow) -> Result<Artwork> {
        Ok(Artwork {
            id: Self::get_string(&row, "id")?,
            hash: Self::get_string(&row, "hash")?,
            mime_type: Self::get_string(&row, "mime_type")?,
            binary_blob: Self::get_blob(&row, "binary_blob")?,
            width: Self::get_i64(&row, "width")?,
            height: Self::get_i64(&row, "height")?,
            file_size: Self::get_i64(&row, "file_size")?,
            dominant_color: Self::get_optional_string(&row, "dominant_color"),
            source: Self::get_string(&row, "source")?,
            created_at: Self::get_i64(&row, "created_at")?,
        })
    }

    // Helper to build insert parameters
    fn insert_params(artwork: &Artwork) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(artwork.id.clone()),
            QueryValue::Text(artwork.hash.clone()),
            QueryValue::Text(artwork.mime_type.clone()),
            QueryValue::Blob(artwork.binary_blob.clone()),
            QueryValue::Integer(artwork.width),
            QueryValue::Integer(artwork.height),
            QueryValue::Integer(artwork.file_size),
            Self::opt_text(&artwork.dominant_color),
            QueryValue::Text(artwork.source.clone()),
            QueryValue::Integer(artwork.created_at),
        ]
    }

    // Helper to build update parameters
    fn update_params(artwork: &Artwork) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(artwork.hash.clone()),
            QueryValue::Text(artwork.mime_type.clone()),
            QueryValue::Blob(artwork.binary_blob.clone()),
            QueryValue::Integer(artwork.width),
            QueryValue::Integer(artwork.height),
            QueryValue::Integer(artwork.file_size),
            Self::opt_text(&artwork.dominant_color),
            QueryValue::Text(artwork.source.clone()),
        ];
        // Add id for WHERE clause
        params.push(QueryValue::Text(artwork.id.clone()));
        params
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ArtworkRepository for SqliteArtworkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Artwork>> {
        let sql = "SELECT * FROM artworks WHERE id = ?";
        let params = vec![QueryValue::Text(id.to_string())];

        match self.adapter.query_one_optional(sql, &params).await? {
            Some(row) => Ok(Some(Self::row_to_artwork(row)?)),
            None => Ok(None),
        }
    }

    async fn insert(&self, artwork: &Artwork) -> Result<()> {
        // Validate before insertion
        artwork.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artwork".to_string(),
            message: e,
        })?;

        let sql = r#"
            INSERT INTO artworks (
                id, hash, mime_type, binary_blob, width, height,
                file_size, dominant_color, source, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let params = Self::insert_params(artwork);
        self.adapter.execute(sql, &params).await?;

        Ok(())
    }

    async fn update(&self, artwork: &Artwork) -> Result<()> {
        // Validate before update
        artwork.validate().map_err(|e| LibraryError::InvalidInput {
            field: "Artwork".to_string(),
            message: e,
        })?;

        let sql = r#"
            UPDATE artworks
            SET hash = ?, mime_type = ?, binary_blob = ?, width = ?, height = ?,
                file_size = ?, dominant_color = ?, source = ?
            WHERE id = ?
        "#;

        let params = Self::update_params(artwork);
        let rows_affected = self.adapter.execute(sql, &params).await?;

        if rows_affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Artwork".to_string(),
                id: artwork.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let sql = "DELETE FROM artworks WHERE id = ?";
        let params = vec![QueryValue::Text(id.to_string())];

        let rows_affected = self.adapter.execute(sql, &params).await?;

        Ok(rows_affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Artwork>> {
        let total = self.count().await?;

        let sql = "SELECT * FROM artworks ORDER BY created_at DESC LIMIT ? OFFSET ?";
        let params = vec![
            QueryValue::Integer(page_request.limit() as i64),
            QueryValue::Integer(page_request.offset() as i64),
        ];

        let rows = self.adapter.query(sql, &params).await?;

        let artworks = rows
            .into_iter()
            .map(Self::row_to_artwork)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(artworks, total as u64, page_request))
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<Artwork>> {
        let sql = "SELECT * FROM artworks WHERE hash = ? LIMIT 1";
        let params = vec![QueryValue::Text(hash.to_string())];

        match self.adapter.query_one_optional(sql, &params).await? {
            Some(row) => Ok(Some(Self::row_to_artwork(row)?)),
            None => Ok(None),
        }
    }

    async fn count(&self) -> Result<i64> {
        let sql = "SELECT COUNT(*) as count FROM artworks";
        let row = self.adapter.query_one(sql, &[]).await?;

        Self::get_i64(&row, "count")
    }

    async fn total_size(&self) -> Result<i64> {
        let sql = "SELECT SUM(file_size) as total FROM artworks";
        let row = self.adapter.query_one(sql, &[]).await?;

        // SUM can return NULL if table is empty
        Ok(Self::get_optional_i64(&row, "total").unwrap_or(0))
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
        let repo = SqliteArtworkRepository::from_pool(pool);

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
        let repo = SqliteArtworkRepository::from_pool(pool);

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
        let repo = SqliteArtworkRepository::from_pool(pool);

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
        let repo = SqliteArtworkRepository::from_pool(pool);

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
        let repo = SqliteArtworkRepository::from_pool(pool);

        // Create artwork with empty data
        let mut artwork = create_test_artwork("hash123");
        artwork.binary_blob = vec![];

        // Should fail validation
        let result = repo.insert(&artwork).await;
        assert!(result.is_err());
    }
}
