//! Artist repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Artist;
use crate::repositories::{Page, PageRequest};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;
use std::sync::Arc;

/// Artist repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ArtistRepository: PlatformSendSync {
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
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteArtistRepository {
    /// Create a new repository using the provided database adapter.
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    fn validate_artist(artist: &Artist) -> Result<()> {
        artist.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "Artist".to_string(),
            message: msg,
        })
    }

    fn insert_params(artist: &Artist) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(artist.id.clone()),
            QueryValue::Text(artist.name.clone()),
            QueryValue::Text(artist.normalized_name.clone()),
            opt_text(&artist.sort_name),
            opt_text(&artist.bio),
            opt_text(&artist.country),
            QueryValue::Integer(artist.created_at),
            QueryValue::Integer(artist.updated_at),
        ]
    }

    fn update_params(artist: &Artist) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(artist.name.clone()),
            QueryValue::Text(artist.normalized_name.clone()),
            opt_text(&artist.sort_name),
            opt_text(&artist.bio),
            opt_text(&artist.country),
            QueryValue::Integer(artist.updated_at),
        ];
        params.push(QueryValue::Text(artist.id.clone()));
        params
    }

    async fn fetch_artists(&self, sql: &str, params: Vec<QueryValue>) -> Result<Vec<Artist>> {
        let rows = self.adapter.query(sql, &params).await?;
        rows.into_iter().map(|row| row_to_artist(&row)).collect()
    }

    async fn fetch_optional_artist(
        &self,
        sql: &str,
        params: Vec<QueryValue>,
    ) -> Result<Option<Artist>> {
        let row = self.adapter.query_one_optional(sql, &params).await?;
        row.map(|row| row_to_artist(&row)).transpose()
    }

    async fn paginate(
        &self,
        count_sql: &str,
        count_params: Vec<QueryValue>,
        data_sql: &str,
        mut data_params: Vec<QueryValue>,
        request: PageRequest,
    ) -> Result<Page<Artist>> {
        let total = self.count_with(count_sql, count_params).await?;
        data_params.push(QueryValue::Integer(request.limit() as i64));
        data_params.push(QueryValue::Integer(request.offset() as i64));
        let items = self.fetch_artists(data_sql, data_params).await?;
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
impl SqliteArtistRepository {
    /// Convenience constructor for native targets using an existing `sqlx` pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(Arc::new(SqliteAdapter::from_pool(pool)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl ArtistRepository for SqliteArtistRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Artist>> {
        self.fetch_optional_artist(
            "SELECT * FROM artists WHERE id = ?",
            vec![QueryValue::Text(id.to_string())],
        )
        .await
    }

    async fn insert(&self, artist: &Artist) -> Result<()> {
        Self::validate_artist(artist)?;
        self.adapter
            .execute(
                r#"
                INSERT INTO artists (
                    id, name, normalized_name, sort_name, bio, country,
                    created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                &Self::insert_params(artist),
            )
            .await?;
        Ok(())
    }

    async fn update(&self, artist: &Artist) -> Result<()> {
        Self::validate_artist(artist)?;
        let affected = self
            .adapter
            .execute(
                r#"
                UPDATE artists
                SET name = ?, normalized_name = ?, sort_name = ?, bio = ?, country = ?,
                    updated_at = ?
                WHERE id = ?
                "#,
                &Self::update_params(artist),
            )
            .await?;
        if affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Artist".to_string(),
                id: artist.id.clone(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let affected = self
            .adapter
            .execute(
                "DELETE FROM artists WHERE id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Artist>> {
        self.paginate(
            "SELECT COUNT(*) as count FROM artists",
            vec![],
            "SELECT * FROM artists ORDER BY name ASC LIMIT ? OFFSET ?",
            vec![],
            page_request,
        )
        .await
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Artist>> {
        let params = vec![QueryValue::Text(search_query.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM artists_fts WHERE artists_fts MATCH ?",
            params.clone(),
            r#"
            SELECT a.* FROM artists a
            INNER JOIN artists_fts fts ON a.id = fts.id
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
        self.count_with("SELECT COUNT(*) as count FROM artists", vec![])
            .await
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Artist>> {
        let normalized = Artist::normalize(name);
        self.fetch_optional_artist(
            "SELECT * FROM artists WHERE normalized_name = ? LIMIT 1",
            vec![QueryValue::Text(normalized)],
        )
        .await
    }
}

pub(crate) fn row_to_artist(row: &QueryRow) -> Result<Artist> {
    Ok(Artist {
        id: get_string(row, "id")?,
        name: get_string(row, "name")?,
        normalized_name: get_string(row, "normalized_name")?,
        sort_name: get_optional_string(row, "sort_name")?,
        bio: get_optional_string(row, "bio")?,
        country: get_optional_string(row, "country")?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_pool;

    #[core_async::test]
    async fn test_insert_and_find_artist() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

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
        let pool = create_test_pool().await.unwrap();
        let repo = SqliteArtistRepository::from_pool(pool);

        // Create artist with empty name
        let mut artist = Artist::new("Test".to_string());
        artist.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&artist).await;
        assert!(result.is_err());
    }
}
