//! High-level query API for the music library.
//!
//! This module composes data from the underlying repositories and database
//! structures to provide ergonomic querying capabilities for the UI layer.
//! It supports rich filtering, sorting, pagination, full-text search, and
//! eager loading of commonly accessed relations.

use crate::error::{LibraryError, Result};
use crate::models::{Album, Artist, Lyrics, Playlist, Track};
use crate::repositories::{
    album::row_to_album, artist::row_to_artist, playlist::row_to_playlist, track::row_to_track,
    AlbumRepository, ArtistRepository, LyricsRepository, Page, PageRequest, SqliteAlbumRepository,
    SqliteArtistRepository, SqliteLyricsRepository, SqliteTrackRepository, TrackRepository,
};
use bridge_traits::database::{DatabaseAdapter, QueryRow, QueryValue};
use futures::stream::{self, BoxStream};
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::Arc;

/// Item returned when querying tracks. Includes the base `Track` plus
/// commonly needed relational metadata to avoid additional round-trips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackListItem {
    /// Full track record.
    pub track: Track,
    /// Display-ready album name, if available.
    pub album_name: Option<String>,
    /// Primary track artist name.
    pub artist_name: Option<String>,
    /// Album artist (for compilations).
    pub album_artist_name: Option<String>,
    /// Artwork identifier preferring track artwork, falling back to album artwork.
    pub display_artwork_id: Option<String>,
}

/// Item returned when querying albums. Contains the album plus related metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumListItem {
    /// Full album record.
    pub album: Album,
    /// Album artist name, if present.
    pub artist_name: Option<String>,
    /// Actual track count computed from the tracks table.
    pub actual_track_count: i64,
    /// Actual summed duration across tracks in milliseconds.
    pub actual_duration_ms: i64,
}

/// Album hit returned during full-text search with a relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlbumSearchItem {
    /// Album record.
    pub album: Album,
    /// Optional album artist name.
    pub artist_name: Option<String>,
    /// FTS BM25 score (lower is more relevant).
    pub score: f64,
}

/// Artist hit returned during search with a relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistSearchItem {
    /// Artist record.
    pub artist: Artist,
    /// FTS BM25 score (lower indicates higher relevance).
    pub score: f64,
}

/// Playlist hit returned during search with a simple relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistSearchItem {
    /// Playlist record.
    pub playlist: Playlist,
    /// Simple score (0.0 exact match, 1.0 fuzzy).
    pub score: f64,
}

/// Aggregated search results for the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchResults {
    /// Track matches ordered by relevance.
    pub tracks: Vec<TrackListItem>,
    /// Album matches ordered by relevance.
    pub albums: Vec<AlbumSearchItem>,
    /// Artist matches ordered by relevance.
    pub artists: Vec<ArtistSearchItem>,
    /// Playlist matches ordered by a simple heuristic.
    pub playlists: Vec<PlaylistSearchItem>,
}

/// Detailed track information with eagerly loaded relations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDetails {
    /// Track record.
    pub track: Track,
    /// Associated album, if any.
    pub album: Option<Album>,
    /// Track artist record.
    pub artist: Option<Artist>,
    /// Album artist record (for compilations).
    pub album_artist: Option<Artist>,
    /// Lyrics for the track, if available.
    pub lyrics: Option<Lyrics>,
    /// Preferred artwork identifier.
    pub display_artwork_id: Option<String>,
}

/// Filter options for querying tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackFilter {
    pub album_id: Option<String>,
    pub artist_id: Option<String>,
    pub album_artist_id: Option<String>,
    pub playlist_id: Option<String>,
    pub provider_id: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
    pub search: Option<String>,
    pub folder_id: Option<String>,
    pub sort: TrackSort,
}

impl Default for TrackFilter {
    fn default() -> Self {
        Self {
            album_id: None,
            artist_id: None,
            album_artist_id: None,
            playlist_id: None,
            provider_id: None,
            genre: None,
            year: None,
            min_duration_ms: None,
            max_duration_ms: None,
            search: None,
            folder_id: None,
            sort: TrackSort::TitleAsc,
        }
    }
}

/// Sorting options for track queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackSort {
    #[default]
    TitleAsc,
    TitleDesc,
    CreatedAtDesc,
    CreatedAtAsc,
    DurationDesc,
    DurationAsc,
}

/// Filter options for querying albums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumFilter {
    pub artist_id: Option<String>,
    pub genre: Option<String>,
    pub min_year: Option<i32>,
    pub max_year: Option<i32>,
    pub search: Option<String>,
    pub sort: AlbumSort,
}

impl Default for AlbumFilter {
    fn default() -> Self {
        Self {
            artist_id: None,
            genre: None,
            min_year: None,
            max_year: None,
            search: None,
            sort: AlbumSort::NameAsc,
        }
    }
}

/// Sorting options for album queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AlbumSort {
    #[default]
    NameAsc,
    NameDesc,
    YearDesc,
    YearAsc,
    UpdatedAtDesc,
    TrackCountDesc,
}

/// High-level service composing complex library queries.
#[derive(Clone)]
pub struct LibraryQueryService {
    adapter: Arc<dyn DatabaseAdapter>,
}

impl LibraryQueryService {
    /// Create a new `LibraryQueryService` backed by the provided adapter.
    pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self {
        Self { adapter }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Create a new `LibraryQueryService` from a SQLite connection pool (native only).
    pub fn from_pool(pool: SqlitePool) -> Self {
        use crate::adapters::sqlite_native::SqliteAdapter;
        Self::new(Arc::new(SqliteAdapter::from_pool(pool)))
    }

    /// Query tracks with filtering, sorting, and pagination.
    pub async fn query_tracks(
        &self,
        filter: TrackFilter,
        page_request: PageRequest,
    ) -> Result<Page<TrackListItem>> {
        validate_duration_range(filter.min_duration_ms, filter.max_duration_ms)?;
        if filter.folder_id.is_some() {
            return Err(LibraryError::InvalidInput {
                field: "folder_id".to_string(),
                message: "Folder-based filtering is not supported by the current schema".into(),
            });
        }

        let spec = build_track_query_spec(&filter);
        let total = self.count_with(&spec.count_sql, &spec.binds).await?.max(0);

        let mut paginated_sql = spec.select_sql.clone();
        paginated_sql.push_str(" LIMIT ? OFFSET ?");

        let mut args = binds_to_query_values(&spec.binds);
        args.push(QueryValue::Integer(page_request.limit() as i64));
        args.push(QueryValue::Integer(page_request.offset() as i64));

        let rows = self.adapter.query(&paginated_sql, &args).await?;
        let items = rows
            .into_iter()
            .map(row_to_track_item)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(items, total as u64, page_request))
    }

    /// Stream tracks matching a filter without materializing the entire result set.
    pub fn stream_tracks(
        &self,
        filter: TrackFilter,
    ) -> Result<BoxStream<'static, Result<TrackListItem>>> {
        validate_duration_range(filter.min_duration_ms, filter.max_duration_ms)?;
        if filter.folder_id.is_some() {
            return Err(LibraryError::InvalidInput {
                field: "folder_id".to_string(),
                message: "Folder-based filtering is not supported by the current schema".into(),
            });
        }

        const STREAM_PAGE_SIZE: u32 = 200;

        let initial_state = TrackStreamState {
            service: self.clone(),
            filter,
            next_page: 0,
            buffer: VecDeque::new(),
            done: false,
            page_size: STREAM_PAGE_SIZE,
        };

        let stream = stream::try_unfold(initial_state, |mut state| async move {
            loop {
                if let Some(item) = state.buffer.pop_front() {
                    return Ok(Some((item, state)));
                }

                if state.done {
                    return Ok(None);
                }

                let page_request = PageRequest::new(state.next_page, state.page_size);
                let page = state
                    .service
                    .query_tracks(state.filter.clone(), page_request)
                    .await?;
                let has_next = page.has_next();
                let items = VecDeque::from(page.items);
                state.next_page += 1;
                state.buffer = items;
                state.done = !has_next;
            }
        });

        Ok(Box::pin(stream))
    }

    /// Query albums with filtering, sorting, pagination, and aggregated metadata.
    pub async fn query_albums(
        &self,
        filter: AlbumFilter,
        page_request: PageRequest,
    ) -> Result<Page<AlbumListItem>> {
        validate_year_range(filter.min_year, filter.max_year)?;

        let spec = build_album_query_spec(&filter);
        let total = self.count_with(&spec.count_sql, &spec.binds).await?.max(0);

        let mut paginated_sql = spec.select_sql.clone();
        paginated_sql.push_str(" LIMIT ? OFFSET ?");

        let mut args = binds_to_query_values(&spec.binds);
        args.push(QueryValue::Integer(page_request.limit() as i64));
        args.push(QueryValue::Integer(page_request.offset() as i64));

        let rows = self.adapter.query(&paginated_sql, &args).await?;
        let items = rows
            .into_iter()
            .map(row_to_album_item)
            .collect::<Result<Vec<_>>>()?;

        Ok(Page::new(items, total as u64, page_request))
    }

    /// Perform full-text search across tracks, albums, artists, and playlists.
    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(SearchResults::default());
        }

        const TRACK_LIMIT: i64 = 15;
        const ALBUM_LIMIT: i64 = 10;
        const ARTIST_LIMIT: i64 = 10;
        const PLAYLIST_LIMIT: i64 = 10;

        let mut results = SearchResults::default();

        // Track search via FTS.
        {
            let rows = self
                .adapter
                .query(
                    r#"
                SELECT
                    t.*,
                    COALESCE(t.artwork_id, alb.artwork_id) AS display_artwork_id,
                    alb.name AS album_name,
                    art.name AS artist_name,
                    aa.name AS album_artist_name,
                    0.0 AS relevance
                FROM tracks_fts
                INNER JOIN tracks t ON t.id = tracks_fts.track_id
                LEFT JOIN albums alb ON alb.id = t.album_id
                LEFT JOIN artists art ON art.id = t.artist_id
                LEFT JOIN artists aa ON aa.id = t.album_artist_id
                WHERE tracks_fts MATCH ?
                ORDER BY tracks_fts.rank ASC, t.normalized_title ASC
                LIMIT ?
                "#,
                    &[
                        QueryValue::Text(trimmed.to_string()),
                        QueryValue::Integer(TRACK_LIMIT),
                    ],
                )
                .await?;

            for row in rows {
                results.tracks.push(row_to_track_item(row)?);
            }
        }

        // Album search via FTS.
        {
            let rows = self
                .adapter
                .query(
                    r#"
                SELECT
                    alb.*,
                    art.name AS artist_name,
                    0.0 AS relevance,
                    COUNT(DISTINCT t.id) AS actual_track_count,
                    COALESCE(SUM(t.duration_ms), 0) AS actual_duration_ms
                FROM albums_fts
                INNER JOIN albums alb ON alb.id = albums_fts.album_id
                LEFT JOIN artists art ON art.id = alb.artist_id
                LEFT JOIN tracks t ON t.album_id = alb.id
                WHERE albums_fts MATCH ?
                GROUP BY alb.id
                ORDER BY albums_fts.rank ASC, alb.normalized_name ASC
                LIMIT ?
                "#,
                    &[
                        QueryValue::Text(trimmed.to_string()),
                        QueryValue::Integer(ALBUM_LIMIT),
                    ],
                )
                .await?;

            for row in rows {
                let mut album = row_to_album(&row)?;
                let artist_name = optional_string(&row, "artist_name");
                let score = row_value_f64(&row, "relevance", 0.0);
                let actual_track_count = required_i64(&row, "actual_track_count")?;
                let actual_duration_ms = required_i64(&row, "actual_duration_ms")?;

                album.track_count = actual_track_count;
                album.total_duration_ms = actual_duration_ms;

                results.albums.push(AlbumSearchItem {
                    album,
                    artist_name,
                    score,
                });
            }
        }

        // Artist search via FTS.
        {
            let rows = self
                .adapter
                .query(
                    r#"
                SELECT
                    art.*,
                    0.0 AS relevance
                FROM artists_fts
                INNER JOIN artists art ON art.id = artists_fts.artist_id
                WHERE artists_fts MATCH ?
                ORDER BY artists_fts.rank ASC, art.normalized_name ASC
                LIMIT ?
                "#,
                    &[
                        QueryValue::Text(trimmed.to_string()),
                        QueryValue::Integer(ARTIST_LIMIT),
                    ],
                )
                .await?;

            for row in rows {
                let artist = row_to_artist(&row)?;
                let score = row_value_f64(&row, "relevance", 0.0);
                results.artists.push(ArtistSearchItem { artist, score });
            }
        }

        // Playlist search using normalized LIKE matching.
        {
            let normalized = Playlist::new(trimmed.to_string()).normalized_name;
            let pattern = format!("%{}%", normalized);
            let rows = self
                .adapter
                .query(
                    r#"
                SELECT
                    pl.*,
                    CASE
                        WHEN pl.normalized_name = ? THEN 0.0
                        ELSE 1.0
                    END AS score
                FROM playlists pl
                WHERE pl.normalized_name LIKE ?
                ORDER BY score ASC, pl.normalized_name ASC
                LIMIT ?
                "#,
                    &[
                        QueryValue::Text(normalized.clone()),
                        QueryValue::Text(pattern),
                        QueryValue::Integer(PLAYLIST_LIMIT),
                    ],
                )
                .await?;

            for row in rows {
                let playlist = row_to_playlist(&row)?;
                let score = row_value_f64(&row, "score", 1.0);
                results
                    .playlists
                    .push(PlaylistSearchItem { playlist, score });
            }
        }

        Ok(results)
    }

    /// Fetch a track with eagerly loaded relations.
    pub async fn get_track_details(&self, track_id: &str) -> Result<TrackDetails> {
        let track_repo = SqliteTrackRepository::new(self.adapter.clone());
        let track =
            track_repo
                .find_by_id(track_id)
                .await?
                .ok_or_else(|| LibraryError::NotFound {
                    entity_type: "Track".to_string(),
                    id: track_id.to_string(),
                })?;

        let album_repo = SqliteAlbumRepository::new(self.adapter.clone());
        let artist_repo = SqliteArtistRepository::new(self.adapter.clone());
        let lyrics_repo = SqliteLyricsRepository::new(self.adapter.clone());

        let album = match &track.album_id {
            Some(album_id) => album_repo.find_by_id(album_id).await?,
            None => None,
        };

        let artist = match &track.artist_id {
            Some(artist_id) => artist_repo.find_by_id(artist_id).await?,
            None => None,
        };

        let album_artist = match &track.album_artist_id {
            Some(artist_id) => artist_repo.find_by_id(artist_id).await?,
            None => None,
        };

        let lyrics = lyrics_repo.find_by_track_id(&track.id).await?;

        let display_artwork_id = track
            .artwork_id
            .clone()
            .or_else(|| album.as_ref().and_then(|a| a.artwork_id.clone()));

        Ok(TrackDetails {
            track,
            album,
            artist,
            album_artist,
            lyrics,
            display_artwork_id,
        })
    }

    async fn count_with(&self, sql: &str, binds: &[BindValue]) -> Result<i64> {
        let row = self
            .adapter
            .query_one(sql, &binds_to_query_values(binds))
            .await?;
        row.get("count")
            .and_then(|value| {
                if value.is_null() {
                    None
                } else {
                    value.as_i64()
                }
            })
            .ok_or_else(|| missing_column("count"))
    }
}

#[derive(Debug, Clone)]
struct TrackQuerySpec {
    select_sql: String,
    count_sql: String,
    binds: Vec<BindValue>,
}

#[derive(Clone)]
struct TrackStreamState {
    service: LibraryQueryService,
    filter: TrackFilter,
    next_page: u32,
    buffer: VecDeque<TrackListItem>,
    done: bool,
    page_size: u32,
}

#[derive(Debug, Clone)]
struct AlbumQuerySpec {
    select_sql: String,
    count_sql: String,
    binds: Vec<BindValue>,
}

#[derive(Debug, Clone)]
enum BindValue {
    Text(String),
    I64(i64),
    I32(i32),
}

impl BindValue {
    fn to_query_value(&self) -> QueryValue {
        match self {
            BindValue::Text(value) => QueryValue::Text(value.clone()),
            BindValue::I64(value) => QueryValue::Integer(*value),
            BindValue::I32(value) => QueryValue::Integer(*value as i64),
        }
    }
}

fn binds_to_query_values(binds: &[BindValue]) -> Vec<QueryValue> {
    binds.iter().map(BindValue::to_query_value).collect()
}

fn build_track_query_spec(filter: &TrackFilter) -> TrackQuerySpec {
    let mut select_sql = String::from(
        "SELECT \
            t.*, \
            COALESCE(t.artwork_id, alb.artwork_id) AS display_artwork_id, \
            alb.name AS album_name, \
            art.name AS artist_name, \
            aa.name AS album_artist_name \
         FROM tracks t",
    );

    let mut joins = Vec::new();
    if filter.playlist_id.is_some() {
        joins.push("INNER JOIN playlist_tracks pt ON pt.track_id = t.id");
    }
    joins.push("LEFT JOIN albums alb ON alb.id = t.album_id");
    joins.push("LEFT JOIN artists art ON art.id = t.artist_id");
    joins.push("LEFT JOIN artists aa ON aa.id = t.album_artist_id");

    for join in &joins {
        select_sql.push(' ');
        select_sql.push_str(join);
    }

    let mut conditions = Vec::new();
    let mut binds = Vec::new();

    if let Some(album_id) = &filter.album_id {
        conditions.push("t.album_id = ?");
        binds.push(BindValue::Text(album_id.clone()));
    }

    if let Some(artist_id) = &filter.artist_id {
        conditions.push("t.artist_id = ?");
        binds.push(BindValue::Text(artist_id.clone()));
    }

    if let Some(album_artist_id) = &filter.album_artist_id {
        conditions.push("t.album_artist_id = ?");
        binds.push(BindValue::Text(album_artist_id.clone()));
    }

    if let Some(provider_id) = &filter.provider_id {
        conditions.push("t.provider_id = ?");
        binds.push(BindValue::Text(provider_id.clone()));
    }

    if let Some(playlist_id) = &filter.playlist_id {
        conditions.push("pt.playlist_id = ?");
        binds.push(BindValue::Text(playlist_id.clone()));
    }

    if let Some(genre) = &filter.genre {
        conditions.push("t.genre = ?");
        binds.push(BindValue::Text(genre.clone()));
    }

    if let Some(year) = filter.year {
        conditions.push("t.year = ?");
        binds.push(BindValue::I32(year));
    }

    if let Some(min_duration) = filter.min_duration_ms {
        conditions.push("t.duration_ms >= ?");
        binds.push(BindValue::I64(min_duration));
    }

    if let Some(max_duration) = filter.max_duration_ms {
        conditions.push("t.duration_ms <= ?");
        binds.push(BindValue::I64(max_duration));
    }

    if let Some(search) = &filter.search {
        let pattern = format!("%{}%", Track::normalize(search));
        conditions.push("t.normalized_title LIKE ?");
        binds.push(BindValue::Text(pattern));
    }

    if !conditions.is_empty() {
        select_sql.push_str(" WHERE ");
        select_sql.push_str(&conditions.join(" AND "));
    }

    select_sql.push_str(" ORDER BY ");
    select_sql.push_str(match filter.sort {
        TrackSort::TitleAsc => "t.normalized_title ASC, t.created_at DESC",
        TrackSort::TitleDesc => "t.normalized_title DESC, t.created_at DESC",
        TrackSort::CreatedAtDesc => "t.created_at DESC, t.normalized_title ASC",
        TrackSort::CreatedAtAsc => "t.created_at ASC, t.normalized_title ASC",
        TrackSort::DurationDesc => "t.duration_ms DESC, t.normalized_title ASC",
        TrackSort::DurationAsc => "t.duration_ms ASC, t.normalized_title ASC",
    });

    let mut count_sql = String::from("SELECT COUNT(*) AS count FROM tracks t");
    for join in &joins {
        // Only joins that affect filtering need to be replicated in the count query.
        if join.starts_with("INNER JOIN playlist_tracks") {
            count_sql.push(' ');
            count_sql.push_str(join);
        }
    }

    if !conditions.is_empty() {
        count_sql.push_str(" WHERE ");
        count_sql.push_str(&conditions.join(" AND "));
    }

    TrackQuerySpec {
        select_sql,
        count_sql,
        binds,
    }
}

fn build_album_query_spec(filter: &AlbumFilter) -> AlbumQuerySpec {
    let mut select_sql = String::from(
        "SELECT \
            alb.*, \
            art.name AS artist_name, \
            COUNT(DISTINCT t.id) AS actual_track_count, \
            COALESCE(SUM(t.duration_ms), 0) AS actual_duration_ms \
         FROM albums alb \
         LEFT JOIN artists art ON art.id = alb.artist_id \
         LEFT JOIN tracks t ON t.album_id = alb.id",
    );

    let mut conditions = Vec::new();
    let mut binds = Vec::new();

    if let Some(artist_id) = &filter.artist_id {
        conditions.push("alb.artist_id = ?");
        binds.push(BindValue::Text(artist_id.clone()));
    }

    if let Some(genre) = &filter.genre {
        conditions.push("alb.genre = ?");
        binds.push(BindValue::Text(genre.clone()));
    }

    if let Some(min_year) = filter.min_year {
        conditions.push("alb.year >= ?");
        binds.push(BindValue::I32(min_year));
    }

    if let Some(max_year) = filter.max_year {
        conditions.push("alb.year <= ?");
        binds.push(BindValue::I32(max_year));
    }

    if let Some(search) = &filter.search {
        let normalized = Album::normalize(search);
        let pattern = format!("%{}%", normalized);
        conditions.push("(alb.normalized_name LIKE ? OR art.normalized_name LIKE ?)");
        binds.push(BindValue::Text(pattern.clone()));
        binds.push(BindValue::Text(pattern));
    }

    if !conditions.is_empty() {
        select_sql.push_str(" WHERE ");
        select_sql.push_str(&conditions.join(" AND "));
    }

    select_sql.push_str(" GROUP BY alb.id");

    select_sql.push_str(" ORDER BY ");
    select_sql.push_str(match filter.sort {
        AlbumSort::NameAsc => "alb.normalized_name ASC",
        AlbumSort::NameDesc => "alb.normalized_name DESC",
        AlbumSort::YearDesc => "alb.year DESC, alb.normalized_name ASC",
        AlbumSort::YearAsc => "alb.year ASC, alb.normalized_name ASC",
        AlbumSort::UpdatedAtDesc => "alb.updated_at DESC, alb.normalized_name ASC",
        AlbumSort::TrackCountDesc => "alb.track_count DESC, alb.normalized_name ASC",
    });

    let mut count_sql = String::from(
        "SELECT COUNT(*) AS count FROM albums alb LEFT JOIN artists art ON art.id = alb.artist_id",
    );
    if !conditions.is_empty() {
        count_sql.push_str(" WHERE ");
        count_sql.push_str(&conditions.join(" AND "));
    }

    AlbumQuerySpec {
        select_sql,
        count_sql,
        binds,
    }
}

fn row_to_track_item(row: QueryRow) -> Result<TrackListItem> {
    let track = row_to_track(&row)?;
    let album_name = optional_string(&row, "album_name");
    let artist_name = optional_string(&row, "artist_name");
    let album_artist_name = optional_string(&row, "album_artist_name");
    let display_artwork_id = track
        .artwork_id
        .clone()
        .or_else(|| optional_string(&row, "display_artwork_id"));

    Ok(TrackListItem {
        track,
        album_name,
        artist_name,
        album_artist_name,
        display_artwork_id,
    })
}

fn row_to_album_item(row: QueryRow) -> Result<AlbumListItem> {
    let mut album = row_to_album(&row)?;
    let artist_name = optional_string(&row, "artist_name");
    let actual_track_count = required_i64(&row, "actual_track_count")?;
    let actual_duration_ms = required_i64(&row, "actual_duration_ms")?;

    album.track_count = actual_track_count;
    album.total_duration_ms = actual_duration_ms;

    Ok(AlbumListItem {
        album,
        artist_name,
        actual_track_count,
        actual_duration_ms,
    })
}

fn validate_duration_range(min: Option<i64>, max: Option<i64>) -> Result<()> {
    match (min, max) {
        (Some(min), Some(max)) if min > max => Err(LibraryError::InvalidInput {
            field: "duration".to_string(),
            message: format!(
                "Minimum duration {}ms cannot be greater than maximum duration {}ms",
                min, max
            ),
        }),
        _ => Ok(()),
    }
}

fn validate_year_range(min: Option<i32>, max: Option<i32>) -> Result<()> {
    match (min, max) {
        (Some(min), Some(max)) if min > max => Err(LibraryError::InvalidInput {
            field: "year".to_string(),
            message: format!("Minimum year {} cannot exceed maximum year {}", min, max),
        }),
        _ => Ok(()),
    }
}

fn optional_string(row: &QueryRow, column: &str) -> Option<String> {
    row.get(column).and_then(|value| {
        if value.is_null() {
            None
        } else {
            value.as_string()
        }
    })
}

fn required_i64(row: &QueryRow, column: &str) -> Result<i64> {
    row.get(column)
        .and_then(|value| {
            if value.is_null() {
                None
            } else {
                value.as_i64()
            }
        })
        .ok_or_else(|| missing_column(column))
}

fn row_value_f64(row: &QueryRow, column: &str, default: f64) -> f64 {
    row.get(column)
        .and_then(|value| {
            if value.is_null() {
                None
            } else {
                value.as_f64()
            }
        })
        .unwrap_or(default)
}

fn missing_column(column: &str) -> LibraryError {
    LibraryError::InvalidInput {
        field: column.to_string(),
        message: "missing column in result set".to_string(),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::db::create_test_pool;
    use crate::repositories::{PlaylistRepository, SqlitePlaylistRepository};
    use sqlx::SqlitePool;

    async fn insert_test_provider(pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO providers (id, type, display_name, profile_id, created_at) \
             VALUES ('test-provider', 'GoogleDrive', 'Test Provider', 'profile', 1700000000)",
        )
        .execute(pool)
        .await
        .ok();
    }

    async fn insert_artist(pool: &SqlitePool, id: &str, name: &str) -> Artist {
        let mut artist = Artist::new(name.to_string());
        artist.id = id.to_string();
        artist.normalized_name = Artist::normalize(name);
        artist.created_at = 1700000000;
        artist.updated_at = 1700000000;
        let repo = SqliteArtistRepository::from_pool(pool.clone());
        repo.insert(&artist).await.unwrap();
        artist
    }

    async fn insert_album(
        pool: &SqlitePool,
        id: &str,
        name: &str,
        artist_id: Option<&str>,
        genre: Option<&str>,
    ) -> Album {
        let mut album = Album::new(name.to_string(), artist_id.map(|s| s.to_string()));
        album.id = id.to_string();
        album.normalized_name = Album::normalize(name);
        album.created_at = 1700000000;
        album.updated_at = 1700000000;
        album.genre = genre.map(|g| g.to_string());
        let repo = SqliteAlbumRepository::from_pool(pool.clone());
        repo.insert(&album).await.unwrap();
        album
    }

    fn make_track(id: &str, album_id: Option<&str>, artist_id: Option<&str>) -> Track {
        Track {
            id: id.to_string(),
            provider_id: "test-provider".to_string(),
            provider_file_id: format!("file-{id}"),
            hash: Some("hash".to_string()),
            title: format!("Track {id}"),
            normalized_title: Track::normalize(&format!("Track {id}")),
            album_id: album_id.map(|s| s.to_string()),
            artist_id: artist_id.map(|s| s.to_string()),
            album_artist_id: artist_id.map(|s| s.to_string()),
            track_number: Some(1),
            disc_number: 1,
            genre: Some("Rock".to_string()),
            year: Some(2024),
            duration_ms: 180000,
            bitrate: Some(320),
            sample_rate: Some(44100),
            channels: Some(2),
            format: "mp3".to_string(),
            file_size: Some(5_242_880),
            mime_type: Some("audio/mpeg".to_string()),
            artwork_id: None,
            lyrics_status: "not_fetched".to_string(),
            created_at: 1700000000,
            updated_at: 1700000000,
            provider_modified_at: Some(1700000000),
        }
    }

    async fn insert_track(pool: &SqlitePool, track: &Track) {
        let repo = SqliteTrackRepository::from_pool(pool.clone());
        repo.insert(track).await.unwrap();
    }

    async fn insert_playlist(pool: &SqlitePool, id: &str, name: &str) -> Playlist {
        let mut playlist = Playlist::new(name.to_string());
        playlist.id = id.to_string();
        playlist.normalized_name = name.trim().to_lowercase();
        playlist.created_at = 1700000000;
        playlist.updated_at = 1700000000;
        let repo = SqlitePlaylistRepository::from_pool(pool.clone());
        repo.insert(&playlist).await.unwrap();
        playlist
    }

    #[core_async::test]
    async fn query_tracks_supports_filters_and_relations() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let artist = insert_artist(&pool, "artist-1", "Artists One").await;
        let album = insert_album(
            &pool,
            "album-1",
            "Album One",
            Some(&artist.id),
            Some("Rock"),
        )
        .await;
        let playlist = insert_playlist(&pool, "playlist-1", "Morning Mix").await;

        let track1 = make_track("track-1", Some(&album.id), Some(&artist.id));
        insert_track(&pool, &track1).await;

        let track2 = make_track("track-2", None, None);
        insert_track(&pool, &track2).await;

        let playlist_repo = SqlitePlaylistRepository::from_pool(pool.clone());
        playlist_repo
            .add_track(&playlist.id, &track1.id, 1)
            .await
            .unwrap();

        let service = LibraryQueryService::from_pool(pool.clone());
        let mut filter = TrackFilter::default();
        filter.album_id = Some(album.id.clone());
        filter.playlist_id = Some(playlist.id.clone());
        filter.sort = TrackSort::TitleAsc;

        let page = service
            .query_tracks(filter, PageRequest::new(0, 10))
            .await
            .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        let item = &page.items[0];
        assert_eq!(item.track.id, track1.id);
        assert_eq!(item.album_name.as_deref(), Some(album.name.as_str()));
        assert_eq!(item.artist_name.as_deref(), Some(artist.name.as_str()));
    }

    #[core_async::test]
    async fn query_albums_returns_aggregated_counts() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let artist = insert_artist(&pool, "artist-2", "Another Artist").await;
        let album = insert_album(
            &pool,
            "album-2",
            "Second Album",
            Some(&artist.id),
            Some("Jazz"),
        )
        .await;

        let track_a = make_track("track-a", Some(&album.id), Some(&artist.id));
        insert_track(&pool, &track_a).await;

        let mut track_b = make_track("track-b", Some(&album.id), Some(&artist.id));
        track_b.duration_ms = 240000;
        insert_track(&pool, &track_b).await;

        let mut filter = AlbumFilter::default();
        filter.genre = Some("Jazz".to_string());
        filter.sort = AlbumSort::TrackCountDesc;

        let service = LibraryQueryService::from_pool(pool.clone());
        let page = service
            .query_albums(filter, PageRequest::new(0, 10))
            .await
            .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        let item = &page.items[0];
        assert_eq!(item.album.id, album.id);
        assert_eq!(item.actual_track_count, 2);
        assert_eq!(
            item.actual_duration_ms,
            track_a.duration_ms + track_b.duration_ms
        );
        assert_eq!(item.artist_name.as_deref(), Some(artist.name.as_str()));
    }

    #[core_async::test]
    async fn search_returns_results_across_entities() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let artist = insert_artist(&pool, "artist-3", "Search Artist").await;
        let album = insert_album(
            &pool,
            "album-3",
            "Search Album",
            Some(&artist.id),
            Some("Rock"),
        )
        .await;
        let track = make_track("search-track", Some(&album.id), Some(&artist.id));
        insert_track(&pool, &track).await;
        insert_playlist(&pool, "playlist-search", "Search Playlist").await;

        let service = LibraryQueryService::from_pool(pool.clone());
        let results = service.search("search").await.unwrap();

        assert!(!results.tracks.is_empty());
        assert!(!results.albums.is_empty());
        assert!(!results.artists.is_empty());
        assert!(!results.playlists.is_empty());

        assert!(results.tracks.iter().any(|item| item.track.id == track.id));
        assert!(results.albums.iter().any(|item| item.album.id == album.id));
        assert!(results
            .artists
            .iter()
            .any(|item| item.artist.id == artist.id));
    }

    #[core_async::test]
    async fn get_track_details_eager_loads_relations() {
        let pool = create_test_pool().await.unwrap();
        insert_test_provider(&pool).await;
        let artist = insert_artist(&pool, "artist-4", "Details Artist").await;
        let album = insert_album(
            &pool,
            "album-4",
            "Details Album",
            Some(&artist.id),
            Some("Pop"),
        )
        .await;

        let track = make_track("details-track", Some(&album.id), Some(&artist.id));
        insert_track(&pool, &track).await;

        let lyrics = Lyrics::new(
            track.id.clone(),
            "manual".to_string(),
            false,
            "Test lyrics".to_string(),
        );
        let lyrics_repo = SqliteLyricsRepository::from_pool(pool.clone());
        lyrics_repo.insert(&lyrics).await.unwrap();

        let service = LibraryQueryService::from_pool(pool.clone());
        let details = service
            .get_track_details(&track.id)
            .await
            .expect("should fetch track details");

        assert_eq!(details.track.id, track.id);
        assert_eq!(
            details.album.as_ref().map(|alb| alb.id.clone()),
            Some(album.id.clone())
        );
        assert_eq!(
            details.artist.as_ref().map(|art| art.id.clone()),
            Some(artist.id.clone())
        );
        assert!(details.lyrics.is_some());
    }
}
