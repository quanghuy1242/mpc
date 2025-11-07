//! Track repository trait and adapter-backed implementation.

use crate::error::{LibraryError, Result};
use crate::models::Track;
use crate::repositories::{Page, PageRequest};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use bridge_traits::platform::PlatformSendSync;
#[cfg(any(test, not(target_arch = "wasm32")))]
use sqlx::SqlitePool;
use std::sync::Arc;

const TRACK_COLUMNS: &str = "id, provider_id, provider_file_id, hash, \
    title, normalized_title, album_id, artist_id, album_artist_id, \
    track_number, disc_number, genre, year, duration_ms, bitrate, \
    sample_rate, channels, format, file_size, mime_type, artwork_id, \
    lyrics_status, created_at, updated_at, provider_modified_at";

/// Track repository interface for data access operations.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait TrackRepository: PlatformSendSync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>>;
    async fn insert(&self, track: &Track) -> Result<()>;
    async fn update(&self, track: &Track) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<bool>;
    async fn query(&self, page_request: PageRequest) -> Result<Page<Track>>;
    async fn query_by_album(
        &self,
        album_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;
    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;
    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>>;
    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Track>>;
    async fn count(&self) -> Result<i64>;
    async fn find_by_provider_file(
        &self,
        provider_id: &str,
        provider_file_id: &str,
    ) -> Result<Option<Track>>;
    async fn find_by_missing_artwork(&self) -> Result<Vec<Track>>;
    async fn find_by_lyrics_status(&self, status: &str) -> Result<Vec<Track>>;
}

/// Adapter-backed track repository (works for both native and WASM targets).
pub struct SqliteTrackRepository {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl SqliteTrackRepository {
    /// Create a new repository using the provided database adapter.
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    fn validate_track(track: &Track) -> Result<()> {
        track.validate().map_err(|msg| LibraryError::InvalidInput {
            field: "track".to_string(),
            message: msg,
        })
    }

    fn insert_params(track: &Track) -> Vec<QueryValue> {
        vec![
            QueryValue::Text(track.id.clone()),
            QueryValue::Text(track.provider_id.clone()),
            QueryValue::Text(track.provider_file_id.clone()),
            opt_text(&track.hash),
            QueryValue::Text(track.title.clone()),
            QueryValue::Text(track.normalized_title.clone()),
            opt_text(&track.album_id),
            opt_text(&track.artist_id),
            opt_text(&track.album_artist_id),
            opt_i32(track.track_number),
            QueryValue::Integer(track.disc_number as i64),
            opt_text(&track.genre),
            opt_i32(track.year),
            QueryValue::Integer(track.duration_ms),
            opt_i32(track.bitrate),
            opt_i32(track.sample_rate),
            opt_i32(track.channels),
            QueryValue::Text(track.format.clone()),
            opt_i64(track.file_size),
            opt_text(&track.mime_type),
            opt_text(&track.artwork_id),
            QueryValue::Text(track.lyrics_status.clone()),
            QueryValue::Integer(track.created_at),
            QueryValue::Integer(track.updated_at),
            opt_i64(track.provider_modified_at),
        ]
    }

    fn update_params(track: &Track) -> Vec<QueryValue> {
        let mut params = vec![
            QueryValue::Text(track.provider_id.clone()),
            QueryValue::Text(track.provider_file_id.clone()),
            opt_text(&track.hash),
            QueryValue::Text(track.title.clone()),
            QueryValue::Text(track.normalized_title.clone()),
            opt_text(&track.album_id),
            opt_text(&track.artist_id),
            opt_text(&track.album_artist_id),
            opt_i32(track.track_number),
            QueryValue::Integer(track.disc_number as i64),
            opt_text(&track.genre),
            opt_i32(track.year),
            QueryValue::Integer(track.duration_ms),
            opt_i32(track.bitrate),
            opt_i32(track.sample_rate),
            opt_i32(track.channels),
            QueryValue::Text(track.format.clone()),
            opt_i64(track.file_size),
            opt_text(&track.mime_type),
            opt_text(&track.artwork_id),
            QueryValue::Text(track.lyrics_status.clone()),
            QueryValue::Integer(track.updated_at),
            opt_i64(track.provider_modified_at),
        ];
        params.push(QueryValue::Text(track.id.clone()));
        params
    }

    async fn fetch_tracks(&self, sql: &str, params: Vec<QueryValue>) -> Result<Vec<Track>> {
        let rows = self.adapter.query(sql, &params).await?;
        rows.into_iter().map(|row| row_to_track(&row)).collect()
    }

    async fn fetch_optional_track(
        &self,
        sql: &str,
        params: Vec<QueryValue>,
    ) -> Result<Option<Track>> {
        let row = self.adapter.query_one_optional(sql, &params).await?;
        row.map(|row| row_to_track(&row)).transpose()
    }

    async fn paginate(
        &self,
        count_sql: &str,
        count_params: Vec<QueryValue>,
        data_sql: &str,
        mut data_params: Vec<QueryValue>,
        request: PageRequest,
    ) -> Result<Page<Track>> {
        let total = self.count_with(count_sql, count_params).await?;
        data_params.push(QueryValue::Integer(request.limit() as i64));
        data_params.push(QueryValue::Integer(request.offset() as i64));
        let items = self.fetch_tracks(data_sql, data_params).await?;
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
impl SqliteTrackRepository {
    /// Convenience constructor for native targets using an existing `sqlx` pool.
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(Arc::new(SqliteAdapter::from_pool(pool)))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl TrackRepository for SqliteTrackRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Track>> {
        self.fetch_optional_track(
            &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?"),
            vec![QueryValue::Text(id.to_string())],
        )
        .await
    }

    async fn insert(&self, track: &Track) -> Result<()> {
        Self::validate_track(track)?;
        self.adapter
            .execute(
                r#"
                INSERT INTO tracks (
                    id, provider_id, provider_file_id, hash,
                    title, normalized_title, album_id, artist_id, album_artist_id,
                    track_number, disc_number, genre, year,
                    duration_ms, bitrate, sample_rate, channels, format,
                    file_size, mime_type, artwork_id, lyrics_status,
                    created_at, updated_at, provider_modified_at
                ) VALUES (
                    ?, ?, ?, ?,
                    ?, ?, ?, ?, ?,
                    ?, ?, ?, ?,
                    ?, ?, ?, ?, ?,
                    ?, ?, ?, ?,
                    ?, ?, ?
                )
                "#,
                &Self::insert_params(track),
            )
            .await?;
        Ok(())
    }

    async fn update(&self, track: &Track) -> Result<()> {
        Self::validate_track(track)?;
        let affected = self
            .adapter
            .execute(
                r#"
                UPDATE tracks SET
                    provider_id = ?, provider_file_id = ?, hash = ?,
                    title = ?, normalized_title = ?, album_id = ?, artist_id = ?, album_artist_id = ?,
                    track_number = ?, disc_number = ?, genre = ?, year = ?,
                    duration_ms = ?, bitrate = ?, sample_rate = ?, channels = ?, format = ?,
                    file_size = ?, mime_type = ?, artwork_id = ?, lyrics_status = ?,
                    updated_at = ?, provider_modified_at = ?
                WHERE id = ?
                "#,
                &Self::update_params(track),
            )
            .await?;
        if affected == 0 {
            return Err(LibraryError::NotFound {
                entity_type: "track".to_string(),
                id: track.id.clone(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let affected = self
            .adapter
            .execute(
                "DELETE FROM tracks WHERE id = ?",
                &[QueryValue::Text(id.to_string())],
            )
            .await?;
        Ok(affected > 0)
    }

    async fn query(&self, page_request: PageRequest) -> Result<Page<Track>> {
        self.paginate(
            "SELECT COUNT(*) as count FROM tracks",
            vec![],
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks ORDER BY created_at DESC LIMIT ? OFFSET ?"
            ),
            vec![],
            page_request,
        )
        .await
    }

    async fn query_by_album(
        &self,
        album_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        let params = vec![QueryValue::Text(album_id.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM tracks WHERE album_id = ?",
            params.clone(),
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE album_id = ? \
                 ORDER BY disc_number, track_number LIMIT ? OFFSET ?"
            ),
            params,
            page_request,
        )
        .await
    }

    async fn query_by_artist(
        &self,
        artist_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        let params = vec![QueryValue::Text(artist_id.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM tracks WHERE artist_id = ?",
            params.clone(),
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE artist_id = ? \
                 ORDER BY year DESC, album_id, track_number LIMIT ? OFFSET ?"
            ),
            params,
            page_request,
        )
        .await
    }

    async fn query_by_provider(
        &self,
        provider_id: &str,
        page_request: PageRequest,
    ) -> Result<Page<Track>> {
        let params = vec![QueryValue::Text(provider_id.to_string())];
        self.paginate(
            "SELECT COUNT(*) as count FROM tracks WHERE provider_id = ?",
            params.clone(),
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE provider_id = ? \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?"
            ),
            params,
            page_request,
        )
        .await
    }

    async fn search(&self, search_query: &str, page_request: PageRequest) -> Result<Page<Track>> {
        let pattern = format!("%{}%", search_query.to_lowercase());
        let params = vec![QueryValue::Text(pattern.clone())];
        self.paginate(
            "SELECT COUNT(*) as count FROM tracks WHERE normalized_title LIKE ?",
            params.clone(),
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE normalized_title LIKE ? \
                 ORDER BY title LIMIT ? OFFSET ?"
            ),
            params,
            page_request,
        )
        .await
    }

    async fn count(&self) -> Result<i64> {
        self.count_with("SELECT COUNT(*) as count FROM tracks", vec![])
            .await
    }

    async fn find_by_provider_file(
        &self,
        provider_id: &str,
        provider_file_id: &str,
    ) -> Result<Option<Track>> {
        self.fetch_optional_track(
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE provider_id = ? AND provider_file_id = ?"
            ),
            vec![
                QueryValue::Text(provider_id.to_string()),
                QueryValue::Text(provider_file_id.to_string()),
            ],
        )
        .await
    }

    async fn find_by_missing_artwork(&self) -> Result<Vec<Track>> {
        self.fetch_tracks(
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE artwork_id IS NULL \
                 ORDER BY created_at DESC"
            ),
            vec![],
        )
        .await
    }

    async fn find_by_lyrics_status(&self, status: &str) -> Result<Vec<Track>> {
        self.fetch_tracks(
            &format!(
                "SELECT {TRACK_COLUMNS} FROM tracks WHERE lyrics_status = ? \
                 ORDER BY created_at DESC"
            ),
            vec![QueryValue::Text(status.to_string())],
        )
        .await
    }
}

pub(crate) fn row_to_track(row: &QueryRow) -> Result<Track> {
    Ok(Track {
        id: get_string(row, "id")?,
        provider_id: get_string(row, "provider_id")?,
        provider_file_id: get_string(row, "provider_file_id")?,
        hash: get_optional_string(row, "hash")?,
        title: get_string(row, "title")?,
        normalized_title: get_string(row, "normalized_title")?,
        album_id: get_optional_string(row, "album_id")?,
        artist_id: get_optional_string(row, "artist_id")?,
        album_artist_id: get_optional_string(row, "album_artist_id")?,
        track_number: get_optional_i32(row, "track_number")?,
        disc_number: get_i32(row, "disc_number")?,
        genre: get_optional_string(row, "genre")?,
        year: get_optional_i32(row, "year")?,
        duration_ms: get_i64(row, "duration_ms")?,
        bitrate: get_optional_i32(row, "bitrate")?,
        sample_rate: get_optional_i32(row, "sample_rate")?,
        channels: get_optional_i32(row, "channels")?,
        format: get_string(row, "format")?,
        file_size: get_optional_i64(row, "file_size")?,
        mime_type: get_optional_string(row, "mime_type")?,
        artwork_id: get_optional_string(row, "artwork_id")?,
        lyrics_status: get_string(row, "lyrics_status")?,
        created_at: get_i64(row, "created_at")?,
        updated_at: get_i64(row, "updated_at")?,
        provider_modified_at: get_optional_i64(row, "provider_modified_at")?,
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

fn opt_i64(value: Option<i64>) -> QueryValue {
    value.map(QueryValue::Integer).unwrap_or(QueryValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_test_pool, insert_test_provider};
    use crate::models::Track;
    use uuid::Uuid;

    async fn create_test_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            provider_id: "test-provider".to_string(),
            provider_file_id: format!("file-{}", id),
            hash: Some(format!("hash-{}", id)),
            title: format!("Track {}", id),
            normalized_title: format!("track {}", id).to_lowercase(),
            album_id: None,
            artist_id: None,
            album_artist_id: None,
            track_number: Some(1),
            disc_number: 1,
            genre: Some("Rock".to_string()),
            year: Some(2020),
            duration_ms: 180_000,
            bitrate: Some(320),
            sample_rate: Some(44_100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(5_000_000),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: None,
            lyrics_status: "not_fetched".to_string(),
            created_at: 0,
            updated_at: 0,
            provider_modified_at: None,
        }
    }

    #[core_async::test]
    async fn test_insert_and_find_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::from_pool(pool.clone());
        let track = create_test_track("track-1").await;

        repo.insert(&track).await.unwrap();

        let found = repo.find_by_id("track-1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Track track-1");
    }

    #[core_async::test]
    async fn test_update_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::from_pool(pool.clone());
        let mut track = create_test_track("track-2").await;

        repo.insert(&track).await.unwrap();

        track.title = "Updated Title".to_string();
        repo.update(&track).await.unwrap();

        let found = repo.find_by_id("track-2").await.unwrap();
        assert_eq!(found.unwrap().title, "Updated Title");
    }

    #[core_async::test]
    async fn test_delete_track() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let repo = SqliteTrackRepository::from_pool(pool);
        let track = create_test_track("track-3").await;

        repo.insert(&track).await.unwrap();
        let deleted = repo.delete("track-3").await.unwrap();
        assert!(deleted);

        let found = repo.find_by_id("track-3").await.unwrap();
        assert!(found.is_none());
    }
}
