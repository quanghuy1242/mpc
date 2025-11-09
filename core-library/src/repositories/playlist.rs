//! Playlist repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Playlist;
use crate::repositories::{Page, PageRequest, PlatformArc};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;

/// Playlist repository interface for data access operations
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait PlaylistRepository: PlatformSendSync {
    /// Find a playlist by its ID
    ///
    /// # Returns
    /// - `Ok(Some(playlist))` if found
    /// - `Ok(None)` if not found
    /// - `Err` if database error occurs
    async fn find_by_id(&self, id: &str) -> Result<Option<Playlist>>;

    /// Insert a new playlist
    ///
    /// # Errors
    /// Returns error if:
    /// - Playlist with same ID already exists
    /// - Playlist validation fails
    /// - Database error occurs
    async fn insert(&self, playlist: &Playlist) -> Result<()>;

    /// Update an existing playlist
    ///
    /// # Errors
    /// Returns error if:
    /// - Playlist does not exist
    /// - Playlist validation fails
    /// - Database error occurs
    async fn update(&self, playlist: &Playlist) -> Result<()>;

    /// Delete a playlist by ID
    ///
    /// # Returns
    /// - `Ok(true)` if playlist was deleted
    /// - `Ok(false)` if playlist was not found
    async fn delete(&self, id: &str) -> Result<bool>;

    /// Query playlists with pagination
    ///
    /// # Arguments
    /// * `page_request` - Pagination parameters
    ///
    /// # Returns
    /// Paginated list of playlists
    async fn query(&self, page_request: PageRequest) -> Result<Page<Playlist>>;

    /// Query playlists by owner type
    ///
    /// # Arguments
    /// * `owner_type` - Owner type ("user" or "system")
    /// * `page_request` - Pagination parameters
    async fn query_by_owner_type(
        &self,
        owner_type: &str,
        page_request: PageRequest,
    ) -> Result<Page<Playlist>>;

    /// Add track to playlist
    ///
    /// # Arguments
    /// * `playlist_id` - Playlist identifier
    /// * `track_id` - Track identifier
    /// * `position` - Position in playlist
    async fn add_track(&self, playlist_id: &str, track_id: &str, position: i32) -> Result<()>;

    /// Remove track from playlist
    ///
    /// # Arguments
    /// * `playlist_id` - Playlist identifier
    /// * `track_id` - Track identifier
    async fn remove_track(&self, playlist_id: &str, track_id: &str) -> Result<bool>;

    /// Get track IDs in playlist
    ///
    /// # Arguments
    /// * `playlist_id` - Playlist identifier
    ///
    /// # Returns
    /// List of track IDs in order
    async fn get_track_ids(&self, playlist_id: &str) -> Result<Vec<String>>;

    /// Count total playlists
    async fn count(&self) -> Result<i64>;
}

/// SQLite implementation of PlaylistRepository
pub struct SqlitePlaylistRepository {
    adapter: PlatformArc<dyn DatabaseAdapter>,
}

impl SqlitePlaylistRepository {
    /// Create a new repository using the provided database adapter.
    pub fn new(adapter: PlatformArc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    fn validate_playlist(playlist: &Playlist) -> Result<()> {
        playlist
            .validate()
            .map_err(|msg| LibraryError::InvalidInput {
                field: "Playlist".to_string(),
                message: msg,
            })
    }

    // Helper to build insert parameters for a playlist
    fn insert_params(playlist: &Playlist) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(playlist.id.clone()),
            QueryValue::Text(playlist.name.clone()),
            QueryValue::Text(playlist.normalized_name.clone()),
            opt_text(&playlist.description),
            QueryValue::Text(playlist.owner_type.clone()),
            QueryValue::Text(playlist.sort_order.clone()),
            QueryValue::Integer(playlist.is_public),
            QueryValue::Integer(playlist.track_count),
            QueryValue::Integer(playlist.total_duration_ms),
            opt_text(&playlist.artwork_id),
            QueryValue::Integer(playlist.created_at),
            QueryValue::Integer(playlist.updated_at),
        ]
    }

    // Helper to build update parameters for a playlist
    fn update_params(playlist: &Playlist) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(playlist.name.clone()),
            QueryValue::Text(playlist.normalized_name.clone()),
            opt_text(&playlist.description),
            QueryValue::Text(playlist.sort_order.clone()),
            QueryValue::Integer(playlist.is_public as i64),
            QueryValue::Integer(playlist.track_count),
            QueryValue::Integer(playlist.total_duration_ms),
            opt_text(&playlist.artwork_id),
            QueryValue::Integer(playlist.updated_at),
        ];
        params.push(QueryValue::Text(playlist.id.clone()));
        params
    }

    async fn fetch_playlists(&self, sql: &str, params: Vec<QueryValue>) -> Result<Vec<Playlist>> {
        let rows = self.adapter.query(sql, &params).await?;
        rows.into_iter().map(|row| row_to_playlist(&row)).collect()
    }

    async fn fetch_optional_playlist(
        &self,
        sql: &str,
        params: Vec<QueryValue>,
    ) -> Result<Option<Playlist>> {
        let row = self.adapter.query_one_optional(sql, &params).await?;
        row.map(|row| row_to_playlist(&row)).transpose()
    }

    async fn paginate(
        &self,
        count_sql: &str,
        count_params: Vec<QueryValue>,
        data_sql: &str,
        mut data_params: Vec<QueryValue>,
        request: PageRequest,
    ) -> Result<Page<Playlist>> {
        let total = self.count_with(count_sql, count_params).await?;
        data_params.push(QueryValue::Integer(request.limit() as i64));
        data_params.push(QueryValue::Integer(request.offset() as i64));
        let items = self.fetch_playlists(data_sql, data_params).await?;
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
impl SqlitePlaylistRepository {
    /// Convenience constructor for native targets using an existing `sqlx` pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(PlatformArc::new(SqliteAdapter::from_pool(pool)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl PlaylistRepository for SqlitePlaylistRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Playlist>> {
        self.fetch_optional_playlist(
            "SELECT * FROM playlists WHERE id = ?",
            vec![QueryValue::Text(id.to_string())],
        )
        .await
    }

    async fn insert(&self, playlist: &Playlist) -> Result<()> {
        Self::validate_playlist(playlist)?;
        self.adapter
            .execute(
                r#"
                INSERT INTO playlists (
                    id, name, normalized_name, description, owner_type, sort_order,
                    is_public, track_count, total_duration_ms, artwork_id, created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                &Self::insert_params(playlist),
            )
            .await?;
        Ok(())
    }

    async fn update(&self, playlist: &Playlist) -> Result<()> {
        Self::validate_playlist(playlist)?;
        let affected = self
            .adapter
            .execute(
                r#"
                UPDATE playlists
                SET name = ?, normalized_name = ?, description = ?, sort_order = ?, 
                    is_public = ?, track_count = ?, total_duration_ms = ?, artwork_id = ?, updated_at = ?
                WHERE id = ?
                "#,
                &Self::update_params(playlist),
            )
            .await?;
        if affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Playlist".to_string(),
                id: playlist.id.clone(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        // Delete playlist tracks first (due to foreign key)
        self.adapter
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;

        // Then delete the playlist
        let affected = self
            .adapter
            .execute(
                "DELETE FROM playlists WHERE id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Playlist>> {
        self.paginate(
            "SELECT COUNT(*) as count FROM playlists",
            vec![],
            "SELECT * FROM playlists ORDER BY name ASC LIMIT ? OFFSET ?",
            vec![],
            page_request,
        )
        .await
    }

    async fn query_by_owner_type(
        &self,
        owner_type: &str,
        page_request: PageRequest,
    ) -> Result<Page<Playlist>> {
        let params = vec![QueryValue::Text(owner_type.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM playlists WHERE owner_type = ?",
            params.clone(),
            "SELECT * FROM playlists WHERE owner_type = ? ORDER BY name ASC LIMIT ? OFFSET ?",
            params,
            page_request,
        )
        .await
    }

    async fn add_track(&self, playlist_id: &str, track_id: &str, position: i32) -> Result<()> {
        self.adapter
            .execute(
                r#"
                INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
                VALUES (?, ?, ?, ?)
                "#,
                &[
                    QueryValue::Text(playlist_id.to_string()),
                    QueryValue::Text(track_id.to_string()),
                    QueryValue::Integer(position as i64),
                    QueryValue::Integer(chrono::Utc::now().timestamp()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn remove_track(&self, playlist_id: &str, track_id: &str) -> Result<bool> {
        let affected = self
            .adapter
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?",
                &[
                    QueryValue::Text(playlist_id.to_string()),
                    QueryValue::Text(track_id.to_string()),
                ],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn get_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let rows = self
            .adapter
            .query(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ? ORDER BY position ASC",
                &[QueryValue::Text(playlist_id.to_string())],
            )
            .await?;
        let track_ids = rows
            .into_iter()
            .filter_map(|row| row.get("track_id").and_then(|v| v.as_string()))
            .collect();
        Ok(track_ids)
    }

    async fn count(&self) -> Result<i64> {
        self.count_with("SELECT COUNT(*) as count FROM playlists", vec![])
            .await
    }
}

pub(crate) fn row_to_playlist(row: &QueryRow) -> Result<Playlist> {
    Ok(Playlist {
        id: get_string(row, "id")?,
        name: get_string(row, "name")?,
        normalized_name: get_string(row, "normalized_name")?,
        description: get_optional_string(row, "description")?,
        owner_type: get_string(row, "owner_type")?,
        sort_order: get_string(row, "sort_order")?,
        is_public: get_i64(row, "is_public")?,
        track_count: get_i64(row, "track_count")?,
        total_duration_ms: get_i64(row, "total_duration_ms")?,
        artwork_id: get_optional_string(row, "artwork_id")?,
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

fn get_i32(row: &QueryRow, key: &str) -> Result<i32> {
    Ok(get_i64(row, key)? as i32)
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
    async fn test_insert_and_find_playlist() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Create and insert playlist
        let mut playlist = Playlist::new("My Playlist".to_string());
        playlist.is_public = 1;
        playlist.description = Some("Workout mix".to_string());
        repo.insert(&playlist).await.unwrap();

        // Find playlist
        let found = repo.find_by_id(&playlist.id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "My Playlist");
        assert_eq!(found.is_public, 1);
        assert_eq!(found.description.as_deref(), Some("Workout mix"));
    }

    #[core_async::test]
    async fn test_update_playlist() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Create and insert playlist
        let mut playlist = Playlist::new("Original Name".to_string());
        playlist.is_public = 0;
        repo.insert(&playlist).await.unwrap();

        // Update playlist
        playlist.name = "Updated Name".to_string();
        playlist.is_public = 1;
        playlist.updated_at = chrono::Utc::now().timestamp();
        repo.update(&playlist).await.unwrap();

        // Verify update
        let found = repo.find_by_id(&playlist.id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated Name");
        assert_eq!(found.is_public, 1);
    }

    #[core_async::test]
    async fn test_delete_playlist() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Create and insert playlist
        let playlist = Playlist::new("Test Playlist".to_string());
        repo.insert(&playlist).await.unwrap();

        // Delete playlist
        let deleted = repo.delete(&playlist.id).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let found = repo.find_by_id(&playlist.id).await.unwrap();
        assert!(found.is_none());
    }

    #[core_async::test]
    async fn test_query_by_owner_type() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Create user and system playlists
        let user_playlist = Playlist::new("User Playlist".to_string());
        let system_playlist =
            Playlist::new_system("System Playlist".to_string(), "date_added".to_string());

        repo.insert(&user_playlist).await.unwrap();
        repo.insert(&system_playlist).await.unwrap();

        // Query user playlists
        let page = repo
            .query_by_owner_type("user", PageRequest::default())
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].owner_type, "user");
    }

    #[core_async::test]
    async fn test_count_playlists() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Initially should be 0
        let count = repo.count().await.unwrap();
        assert_eq!(count, 0);

        // Insert playlists
        for i in 1..=3 {
            let playlist = Playlist::new(format!("Playlist {}", i));
            repo.insert(&playlist).await.unwrap();
        }

        // Count should be 3
        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[core_async::test]
    async fn test_playlist_validation() {
        let pool = create_test_pool().await.unwrap();
        let repo = SqlitePlaylistRepository::from_pool(pool);

        // Create playlist with empty name
        let mut playlist = Playlist::new("Test".to_string());
        playlist.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&playlist).await;
        assert!(result.is_err());
    }
}
