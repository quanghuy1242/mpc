//! Playlist repository trait and implementation

use crate::error::{LibraryError, Result};
use crate::models::Playlist;
use crate::repositories::{Page, PageRequest};
use async_trait::async_trait;
use sqlx::{query, query_as, SqlitePool};

/// Playlist repository interface for data access operations
#[async_trait]
pub trait PlaylistRepository: Send + Sync {
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
    pool: SqlitePool,
}

impl SqlitePlaylistRepository {
    /// Create a new SqlitePlaylistRepository
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PlaylistRepository for SqlitePlaylistRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Playlist>> {
        let playlist = query_as::<_, Playlist>("SELECT * FROM playlists WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(playlist)
    }

    async fn insert(&self, playlist: &Playlist) -> Result<()> {
        // Validate before insertion
        playlist
            .validate()
            .map_err(|e| LibraryError::InvalidInput {
                field: "Playlist".to_string(),
                message: e,
            })?;

        query(
            r#"
            INSERT INTO playlists (
                id, name, normalized_name, description, owner_type, sort_order,
                is_public, track_count, total_duration_ms, artwork_id, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&playlist.id)
        .bind(&playlist.name)
        .bind(&playlist.normalized_name)
        .bind(&playlist.description)
        .bind(&playlist.owner_type)
        .bind(&playlist.sort_order)
        .bind(playlist.is_public)
        .bind(playlist.track_count)
        .bind(playlist.total_duration_ms)
        .bind(&playlist.artwork_id)
        .bind(playlist.created_at)
        .bind(playlist.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, playlist: &Playlist) -> Result<()> {
        // Validate before update
        playlist
            .validate()
            .map_err(|e| LibraryError::InvalidInput {
                field: "Playlist".to_string(),
                message: e,
            })?;

        let result = query(
            r#"
            UPDATE playlists
            SET name = ?, normalized_name = ?, description = ?, sort_order = ?, 
                is_public = ?, track_count = ?, total_duration_ms = ?, artwork_id = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&playlist.name)
        .bind(&playlist.normalized_name)
        .bind(&playlist.description)
        .bind(&playlist.sort_order)
        .bind(playlist.is_public)
        .bind(playlist.track_count)
        .bind(playlist.total_duration_ms)
        .bind(&playlist.artwork_id)
        .bind(playlist.updated_at)
        .bind(&playlist.id)
        .execute(&self.pool)
        .await
        ?;

        if result.rows_affected() == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "Playlist".to_string(),
                id: playlist.id.clone(),
            });
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        // Delete playlist tracks first (due to foreign key)
        query("DELETE FROM playlist_tracks WHERE playlist_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Then delete the playlist
        let result = query("DELETE FROM playlists WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Playlist>> {
        let total = self.count().await?;

        let playlists =
            query_as::<_, Playlist>("SELECT * FROM playlists ORDER BY name ASC LIMIT ? OFFSET ?")
                .bind(page_request.limit())
                .bind(page_request.offset())
                .fetch_all(&self.pool)
                .await?;

        Ok(Page::new(playlists, total as u64, page_request))
    }

    async fn query_by_owner_type(
        &self,
        owner_type: &str,
        page_request: PageRequest,
    ) -> Result<Page<Playlist>> {
        let total: i64 = query_as("SELECT COUNT(*) as count FROM playlists WHERE owner_type = ?")
            .bind(owner_type)
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        let playlists = query_as::<_, Playlist>(
            "SELECT * FROM playlists WHERE owner_type = ? ORDER BY name ASC LIMIT ? OFFSET ?",
        )
        .bind(owner_type)
        .bind(page_request.limit())
        .bind(page_request.offset())
        .fetch_all(&self.pool)
        .await?;

        Ok(Page::new(playlists, total as u64, page_request))
    }

    async fn add_track(&self, playlist_id: &str, track_id: &str, position: i32) -> Result<()> {
        query(
            r#"
            INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(playlist_id)
        .bind(track_id)
        .bind(position)
        .bind(chrono::Utc::now().timestamp())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_track(&self, playlist_id: &str, track_id: &str) -> Result<bool> {
        let result = query("DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?")
            .bind(playlist_id)
            .bind(track_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let track_ids = query_as::<_, (String,)>(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id = ? ORDER BY position ASC",
        )
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(|(id,)| id).collect())?;

        Ok(track_ids)
    }

    async fn count(&self) -> Result<i64> {
        let count: i64 = query_as("SELECT COUNT(*) as count FROM playlists")
            .fetch_one(&self.pool)
            .await
            .map(|row: (i64,)| row.0)?;

        Ok(count)
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
    async fn test_insert_and_find_playlist() {
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

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
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

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
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

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
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

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
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

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
        let pool = setup_test_pool().await;
        let repo = SqlitePlaylistRepository::new(pool);

        // Create playlist with empty name
        let mut playlist = Playlist::new("Test".to_string());
        playlist.name = "".to_string();

        // Should fail validation
        let result = repo.insert(&playlist).await;
        assert!(result.is_err());
    }
}
