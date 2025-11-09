//! WebAssembly bindings for core-library
//!
//! This module provides complete JavaScript/TypeScript-friendly bindings for ALL core-library
//! functionality including models, repositories, query service, and database operations.
//!
//! ## Architecture
//!
//! - **Models**: All domain models (Track, Album, Artist, etc.) wrapped with JS-friendly APIs
//! - **Repositories**: Full CRUD operations for all entities via DatabaseAdapter
//! - **Query Service**: High-level query APIs with filtering, sorting, pagination, and search
//! - **Database**: WasmDbAdapter integration for JavaScript-backed database operations
//!
//! ## JavaScript Integration
//!
//! All WASM types can be:
//! - Constructed from JavaScript using `new` or factory methods
//! - Converted to/from JSON strings using `toJson()` / `fromJson()`
//! - Converted to/from JS objects using `toObject()` / `fromObject()`
//!
//! ## Philosophy
//!
//! If native has a feature, WASM must expose it. No simplified alternatives.

use crate::error::LibraryError;
use crate::models::*;
use crate::repositories::*;
use bridge_traits::database::{DatabaseAdapter, DatabaseConfig};
use bridge_wasm::database::WasmDbAdapter;
use js_sys::Promise;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

// Use `wee_alloc` as the global allocator for smaller binary size
#[cfg(feature = "wee_alloc_feature")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Set up panic hook for better error messages in the browser
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// =============================================================================
// Error Handling Utilities
// =============================================================================

/// Convert Rust Result to JS-friendly Result
fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}

/// Convert LibraryError to JsValue
#[allow(dead_code)]
fn library_error_to_js(err: LibraryError) -> JsValue {
    to_js_error(err)
}

// =============================================================================
// Utility Functions
// =============================================================================

// NOTE: These are only exported when building as standalone WASM.
// When used as a dependency (e.g., in core-playback), these are disabled
// to avoid symbol conflicts with other modules.

#[cfg(feature = "wasm-standalone")]
/// Get the library version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "wasm-standalone")]
/// Get the library name
#[wasm_bindgen]
pub fn name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

// =============================================================================
// Pagination - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible PageRequest wrapper
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsPageRequest {
    inner: PageRequest,
}

#[wasm_bindgen]
impl JsPageRequest {
    /// Create a new page request
    #[wasm_bindgen(constructor)]
    pub fn new(page: u32, page_size: u32) -> Self {
        Self {
            inner: PageRequest::new(page, page_size),
        }
    }

    /// Get the page number
    pub fn page(&self) -> u32 {
        self.inner.page
    }

    /// Get the page size
    #[wasm_bindgen(js_name = pageSize)]
    pub fn page_size(&self) -> u32 {
        self.inner.page_size
    }

    /// Get the offset
    pub fn offset(&self) -> u32 {
        self.inner.offset()
    }

    /// Get the limit
    pub fn limit(&self) -> u32 {
        self.inner.limit()
    }
}

impl From<JsPageRequest> for PageRequest {
    fn from(req: JsPageRequest) -> Self {
        req.inner
    }
}

// =============================================================================
// ID Types - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible TrackId wrapper
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsTrackId {
    inner: TrackId,
}

#[wasm_bindgen]
impl JsTrackId {
    /// Create a new random TrackId
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: TrackId::new(),
        }
    }

    /// Create TrackId from string
    #[wasm_bindgen(js_name = fromString)]
    pub fn from_string(s: &str) -> std::result::Result<JsTrackId, JsValue> {
        TrackId::from_string(s)
            .map(|inner| JsTrackId { inner })
            .map_err(to_js_error)
    }

    /// Convert to string
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        self.inner.to_string()
    }
}

/// JavaScript-accessible AlbumId wrapper
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsAlbumId {
    inner: AlbumId,
}

#[wasm_bindgen]
impl JsAlbumId {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: AlbumId::new(),
        }
    }

    #[wasm_bindgen(js_name = fromString)]
    pub fn from_string(s: &str) -> std::result::Result<JsAlbumId, JsValue> {
        AlbumId::from_string(s)
            .map(|inner| JsAlbumId { inner })
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        self.inner.to_string()
    }
}

/// JavaScript-accessible ArtistId wrapper
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsArtistId {
    inner: ArtistId,
}

#[wasm_bindgen]
impl JsArtistId {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: ArtistId::new(),
        }
    }

    #[wasm_bindgen(js_name = fromString)]
    pub fn from_string(s: &str) -> std::result::Result<JsArtistId, JsValue> {
        ArtistId::from_string(s)
            .map(|inner| JsArtistId { inner })
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        self.inner.to_string()
    }
}

/// JavaScript-accessible PlaylistId wrapper
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct JsPlaylistId {
    inner: PlaylistId,
}

#[wasm_bindgen]
impl JsPlaylistId {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: PlaylistId::new(),
        }
    }

    #[wasm_bindgen(js_name = fromString)]
    pub fn from_string(s: &str) -> std::result::Result<JsPlaylistId, JsValue> {
        PlaylistId::from_string(s)
            .map(|inner| JsPlaylistId { inner })
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        self.inner.to_string()
    }
}

// =============================================================================
// Domain Models - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible Track wrapper
#[wasm_bindgen]
pub struct JsTrack {
    inner: Track,
}

#[wasm_bindgen]
impl JsTrack {
    /// Create a new track with required fields
    /// 
    /// # Arguments
    /// * `title` - Track title
    /// * `provider_id` - Provider identifier (e.g., "google_drive")
    /// * `provider_file_id` - Provider's file identifier
    /// * `duration_ms` - Duration in milliseconds
    /// * `disc_number` - Disc number (typically 1)
    #[wasm_bindgen(constructor)]
    pub fn new(
        title: String,
        provider_id: String,
        provider_file_id: String,
        duration_ms: i64,
        disc_number: i32,
    ) -> Self {
        Self {
            inner: Track::new(title, provider_id, provider_file_id, duration_ms, disc_number),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsTrack, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsTrack { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue for JavaScript interop
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsTrack, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsTrack { inner })
            .map_err(to_js_error)
    }

    /// Validate the track
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn title(&self) -> String {
        self.inner.title.clone()
    }

    #[wasm_bindgen(js_name = providerId)]
    pub fn provider_id(&self) -> String {
        self.inner.provider_id.clone()
    }

    #[wasm_bindgen(js_name = durationMs)]
    pub fn duration_ms(&self) -> i64 {
        self.inner.duration_ms
    }

    pub fn format(&self) -> String {
        self.inner.format.clone()
    }

    #[wasm_bindgen(js_name = albumId)]
    pub fn album_id(&self) -> Option<String> {
        self.inner.album_id.clone()
    }

    #[wasm_bindgen(js_name = artistId)]
    pub fn artist_id(&self) -> Option<String> {
        self.inner.artist_id.clone()
    }

    // Setters for optional fields (builder pattern)
    #[wasm_bindgen(js_name = setAlbumId)]
    pub fn set_album_id(&mut self, album_id: Option<String>) {
        self.inner.album_id = album_id;
    }

    #[wasm_bindgen(js_name = setArtistId)]
    pub fn set_artist_id(&mut self, artist_id: Option<String>) {
        self.inner.artist_id = artist_id;
    }

    #[wasm_bindgen(js_name = setTrackNumber)]
    pub fn set_track_number(&mut self, track_number: Option<i32>) {
        self.inner.track_number = track_number;
    }

    #[wasm_bindgen(js_name = setGenre)]
    pub fn set_genre(&mut self, genre: Option<String>) {
        self.inner.genre = genre;
    }

    #[wasm_bindgen(js_name = setYear)]
    pub fn set_year(&mut self, year: Option<i32>) {
        self.inner.year = year;
    }

    #[wasm_bindgen(js_name = setBitrate)]
    pub fn set_bitrate(&mut self, bitrate: Option<i32>) {
        self.inner.bitrate = bitrate;
    }

    #[wasm_bindgen(js_name = setSampleRate)]
    pub fn set_sample_rate(&mut self, sample_rate: Option<i32>) {
        self.inner.sample_rate = sample_rate;
    }

    #[wasm_bindgen(js_name = setChannels)]
    pub fn set_channels(&mut self, channels: Option<i32>) {
        self.inner.channels = channels;
    }

    #[wasm_bindgen(js_name = setFormat)]
    pub fn set_format(&mut self, format: String) {
        self.inner.format = format;
    }

    #[wasm_bindgen(js_name = setFileSize)]
    pub fn set_file_size(&mut self, file_size: Option<i64>) {
        self.inner.file_size = file_size;
    }

    #[wasm_bindgen(js_name = setMimeType)]
    pub fn set_mime_type(&mut self, mime_type: Option<String>) {
        self.inner.mime_type = mime_type;
    }

    #[wasm_bindgen(js_name = setArtworkId)]
    pub fn set_artwork_id(&mut self, artwork_id: Option<String>) {
        self.inner.artwork_id = artwork_id;
    }
}

// Internal conversion methods
impl JsTrack {
    pub(crate) fn from_track(track: Track) -> Self {
        Self { inner: track }
    }

    pub(crate) fn to_track(&self) -> Track {
        self.inner.clone()
    }
}

/// JavaScript-accessible Album wrapper
#[wasm_bindgen]
pub struct JsAlbum {
    inner: Album,
}

#[wasm_bindgen]
impl JsAlbum {
    /// Create a new album
    #[wasm_bindgen(constructor)]
    pub fn new(name: String, artist_id: Option<String>) -> Self {
        Self {
            inner: Album::new(name, artist_id),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsAlbum, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsAlbum { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsAlbum, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsAlbum { inner })
            .map_err(to_js_error)
    }

    /// Validate the album
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    /// Normalize a string
    pub fn normalize(s: &str) -> String {
        Album::normalize(s)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[wasm_bindgen(js_name = normalizedName)]
    pub fn normalized_name(&self) -> String {
        self.inner.normalized_name.clone()
    }

    #[wasm_bindgen(js_name = trackCount)]
    pub fn track_count(&self) -> i64 {
        self.inner.track_count
    }

    #[wasm_bindgen(js_name = artistId)]
    pub fn artist_id(&self) -> Option<String> {
        self.inner.artist_id.clone()
    }

    // Setters for optional fields (builder pattern)
    #[wasm_bindgen(js_name = setArtistId)]
    pub fn set_artist_id(&mut self, artist_id: Option<String>) {
        self.inner.artist_id = artist_id;
    }

    #[wasm_bindgen(js_name = setYear)]
    pub fn set_year(&mut self, year: Option<i32>) {
        self.inner.year = year;
    }

    #[wasm_bindgen(js_name = setGenre)]
    pub fn set_genre(&mut self, genre: Option<String>) {
        self.inner.genre = genre;
    }

    #[wasm_bindgen(js_name = setArtworkId)]
    pub fn set_artwork_id(&mut self, artwork_id: Option<String>) {
        self.inner.artwork_id = artwork_id;
    }
}

// Internal conversion methods
impl JsAlbum {
    pub(crate) fn from_album(album: Album) -> Self {
        Self { inner: album }
    }

    pub(crate) fn to_album(&self) -> Album {
        self.inner.clone()
    }
}

/// JavaScript-accessible Artist wrapper
#[wasm_bindgen]
pub struct JsArtist {
    inner: Artist,
}

#[wasm_bindgen]
impl JsArtist {
    /// Create a new artist
    #[wasm_bindgen(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            inner: Artist::new(name),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsArtist, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsArtist { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsArtist, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsArtist { inner })
            .map_err(to_js_error)
    }

    /// Validate the artist
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    /// Normalize a string
    pub fn normalize(s: &str) -> String {
        Artist::normalize(s)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[wasm_bindgen(js_name = normalizedName)]
    pub fn normalized_name(&self) -> String {
        self.inner.normalized_name.clone()
    }

    // Setters for optional fields (builder pattern)
    #[wasm_bindgen(js_name = setSortName)]
    pub fn set_sort_name(&mut self, sort_name: Option<String>) {
        self.inner.sort_name = sort_name;
    }

    #[wasm_bindgen(js_name = setBio)]
    pub fn set_bio(&mut self, bio: Option<String>) {
        self.inner.bio = bio;
    }

    #[wasm_bindgen(js_name = setCountry)]
    pub fn set_country(&mut self, country: Option<String>) {
        self.inner.country = country;
    }
}

// Internal conversion methods
impl JsArtist {
    pub(crate) fn from_artist(artist: Artist) -> Self {
        Self { inner: artist }
    }

    pub(crate) fn to_artist(&self) -> Artist {
        self.inner.clone()
    }
}

/// JavaScript-accessible Playlist wrapper
#[wasm_bindgen]
pub struct JsPlaylist {
    inner: Playlist,
}

#[wasm_bindgen]
impl JsPlaylist {
    /// Create a new user playlist
    #[wasm_bindgen(constructor)]
    pub fn new(name: String) -> Self {
        Self {
            inner: Playlist::new(name),
        }
    }

    /// Create a system playlist
    #[wasm_bindgen(js_name = newSystem)]
    pub fn new_system(name: String, sort_order: String) -> Self {
        Self {
            inner: Playlist::new_system(name, sort_order),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsPlaylist, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsPlaylist { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsPlaylist, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsPlaylist { inner })
            .map_err(to_js_error)
    }

    /// Validate the playlist
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[wasm_bindgen(js_name = ownerType)]
    pub fn owner_type(&self) -> String {
        self.inner.owner_type.clone()
    }

    #[wasm_bindgen(js_name = trackCount)]
    pub fn track_count(&self) -> i64 {
        self.inner.track_count
    }

    // Setters for optional fields (builder pattern)
    #[wasm_bindgen(js_name = setDescription)]
    pub fn set_description(&mut self, description: Option<String>) {
        self.inner.description = description;
    }

    #[wasm_bindgen(js_name = setIsPublic)]
    pub fn set_is_public(&mut self, is_public: bool) {
        self.inner.is_public = if is_public { 1 } else { 0 };
    }

    #[wasm_bindgen(js_name = setArtworkId)]
    pub fn set_artwork_id(&mut self, artwork_id: Option<String>) {
        self.inner.artwork_id = artwork_id;
    }

    #[wasm_bindgen(js_name = setSortOrder)]
    pub fn set_sort_order(&mut self, sort_order: String) {
        self.inner.sort_order = sort_order;
    }
}

// Internal conversion methods
impl JsPlaylist {
    pub(crate) fn from_playlist(playlist: Playlist) -> Self {
        Self { inner: playlist }
    }

    pub(crate) fn to_playlist(&self) -> Playlist {
        self.inner.clone()
    }
}

/// JavaScript-accessible Folder wrapper
#[wasm_bindgen]
pub struct JsFolder {
    inner: Folder,
}

#[wasm_bindgen]
impl JsFolder {
    /// Create a new folder
    #[wasm_bindgen(constructor)]
    pub fn new(
        provider_id: String,
        provider_folder_id: String,
        name: String,
        parent_id: Option<String>,
        path: String,
    ) -> Self {
        Self {
            inner: Folder::new(provider_id, provider_folder_id, name, parent_id, path),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsFolder, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsFolder { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsFolder, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsFolder { inner })
            .map_err(to_js_error)
    }

    /// Validate the folder
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    pub fn path(&self) -> String {
        self.inner.path.clone()
    }

    #[wasm_bindgen(js_name = providerId)]
    pub fn provider_id(&self) -> String {
        self.inner.provider_id.clone()
    }
}

// Internal conversion
impl JsFolder {
    pub(crate) fn from_folder(folder: Folder) -> Self {
        Self { inner: folder }
    }

    pub(crate) fn to_folder(&self) -> Folder {
        self.inner.clone()
    }
}

/// JavaScript-accessible Lyrics wrapper
#[wasm_bindgen]
pub struct JsLyrics {
    inner: Lyrics,
}

#[wasm_bindgen]
impl JsLyrics {
    /// Create new lyrics
    #[wasm_bindgen(constructor)]
    pub fn new(track_id: String, source: String, synced: bool, body: String) -> Self {
        Self {
            inner: Lyrics::new(track_id, source, synced, body),
        }
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsLyrics, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsLyrics { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsLyrics, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsLyrics { inner })
            .map_err(to_js_error)
    }

    /// Validate the lyrics
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    /// Check if lyrics are in LRC format
    #[wasm_bindgen(js_name = isLrcFormat)]
    pub fn is_lrc_format(&self) -> bool {
        self.inner.is_lrc_format()
    }

    // Getters
    #[wasm_bindgen(js_name = trackId)]
    pub fn track_id(&self) -> String {
        self.inner.track_id.clone()
    }

    pub fn source(&self) -> String {
        self.inner.source.clone()
    }

    pub fn body(&self) -> String {
        self.inner.body.clone()
    }

    pub fn synced(&self) -> bool {
        self.inner.synced != 0
    }
}

// Internal conversion
impl JsLyrics {
    pub(crate) fn from_lyrics(lyrics: Lyrics) -> Self {
        Self { inner: lyrics }
    }

    pub(crate) fn to_lyrics(&self) -> Lyrics {
        self.inner.clone()
    }
}

// =============================================================================
// Database & Repository Layer
// =============================================================================

/// JavaScript-accessible database wrapper using WasmDbAdapter and repositories.
/// Repositories are created on-demand from the adapter.
/// We use Rc for the adapter since PlatformArc is Rc on WASM.
#[wasm_bindgen]
pub struct JsLibrary {
    adapter_rc: Rc<dyn DatabaseAdapter>,
}

#[wasm_bindgen]
impl JsLibrary {
    /// Create a new library instance with the given database URL
    /// Returns a Promise that resolves to JsLibrary
    pub fn create(database_url: String) -> Promise {
        future_to_promise(async move {
            let config = DatabaseConfig::new(&database_url);
            
            // Create adapter instance wrapped in Rc (PlatformArc on WASM)
            let adapter = WasmDbAdapter::new(config)
                .await
                .map_err(|e| to_js_error(format!("Failed to create database: {}", e)))?;

            let adapter_rc: Rc<dyn DatabaseAdapter> = Rc::new(adapter);

            Ok(JsValue::from(JsLibrary {
                adapter_rc,
            }))
        })
    }
    // Helper methods to create repositories on-demand (repositories use Arc)
    fn track_repo(&self) -> SqliteTrackRepository {
        SqliteTrackRepository::new(self.adapter_rc.clone())
    }

    fn album_repo(&self) -> SqliteAlbumRepository {
        SqliteAlbumRepository::new(self.adapter_rc.clone())
    }

    fn artist_repo(&self) -> SqliteArtistRepository {
        SqliteArtistRepository::new(self.adapter_rc.clone())
    }

    fn playlist_repo(&self) -> SqlitePlaylistRepository {
        SqlitePlaylistRepository::new(self.adapter_rc.clone())
    }

    fn folder_repo(&self) -> SqliteFolderRepository {
        SqliteFolderRepository::new(self.adapter_rc.clone())
    }

    fn lyrics_repo(&self) -> SqliteLyricsRepository {
        SqliteLyricsRepository::new(self.adapter_rc.clone())
    }

    /// Initialize the database (run migrations, create tables)
    /// Must be called after create and before any other operations
    pub fn initialize(&mut self) -> Promise {
        let adapter = self.adapter_rc.clone();
        future_to_promise(async move {
            // For WASM, initialization is typically handled by JS bridge
            adapter
                .health_check()
                .await
                .map_err(|e| to_js_error(format!("Health check failed: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Check database health
    #[wasm_bindgen(js_name = healthCheck)]
    pub fn health_check(&self) -> Promise {
        let adapter = self.adapter_rc.clone();
        future_to_promise(async move {
            adapter
                .health_check()
                .await
                .map_err(|e| to_js_error(format!("Health check failed: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    // =============================================================================
    // Track Operations
    // =============================================================================

    /// Insert a track into the database
    #[wasm_bindgen(js_name = insertTrack)]
    pub fn insert_track(&self, track: &JsTrack) -> Promise {
        let repo = self.track_repo();
        let track_model = track.to_track();
        future_to_promise(async move {
            repo.insert(&track_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert track: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get a track by ID
    #[wasm_bindgen(js_name = getTrack)]
    pub fn get_track(&self, id: String) -> Promise {
        let repo = self.track_repo();
        future_to_promise(async move {
            let track = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get track: {}", e)))?;

            Ok(track.map(JsTrack::from_track).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update a track
    #[wasm_bindgen(js_name = updateTrack)]
    pub fn update_track(&self, track: &JsTrack) -> Promise {
        let repo = self.track_repo();
        let track_model = track.to_track();
        future_to_promise(async move {
            repo.update(&track_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update track: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete a track by ID
    #[wasm_bindgen(js_name = deleteTrack)]
    pub fn delete_track(&self, id: String) -> Promise {
        let repo = self.track_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete track: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }

    /// List tracks with pagination
    #[wasm_bindgen(js_name = listTracks)]
    pub fn list_tracks(&self, page_request: JsPageRequest) -> Promise {
        let repo = self.track_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query(page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to list tracks: {}", e)))?;

            // Convert to JSON for JS consumption
            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Query tracks by album
    #[wasm_bindgen(js_name = queryTracksByAlbum)]
    pub fn query_tracks_by_album(&self, album_id: String, page_request: JsPageRequest) -> Promise {
        let repo = self.track_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query_by_album(&album_id, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to query tracks by album: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Query tracks by artist
    #[wasm_bindgen(js_name = queryTracksByArtist)]
    pub fn query_tracks_by_artist(&self, artist_id: String, page_request: JsPageRequest) -> Promise {
        let repo = self.track_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query_by_artist(&artist_id, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to query tracks by artist: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Search tracks by query string
    #[wasm_bindgen(js_name = searchTracks)]
    pub fn search_tracks(&self, query: String, page_request: JsPageRequest) -> Promise {
        let repo = self.track_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .search(&query, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to search tracks: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Count total tracks
    #[wasm_bindgen(js_name = countTracks)]
    pub fn count_tracks(&self) -> Promise {
        let repo = self.track_repo();
        future_to_promise(async move {
            let count = repo
                .count()
                .await
                .map_err(|e| to_js_error(format!("Failed to count tracks: {}", e)))?;
            Ok(JsValue::from_f64(count as f64))
        })
    }

    // =============================================================================
    // Album Operations
    // =============================================================================

    /// Insert an album
    #[wasm_bindgen(js_name = insertAlbum)]
    pub fn insert_album(&self, album: &JsAlbum) -> Promise {
        let repo = self.album_repo();
        let album_model = album.to_album();
        future_to_promise(async move {
            repo.insert(&album_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert album: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get an album by ID
    #[wasm_bindgen(js_name = getAlbum)]
    pub fn get_album(&self, id: String) -> Promise {
        let repo = self.album_repo();
        future_to_promise(async move {
            let album = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get album: {}", e)))?;

            Ok(album.map(JsAlbum::from_album).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update an album
    #[wasm_bindgen(js_name = updateAlbum)]
    pub fn update_album(&self, album: &JsAlbum) -> Promise {
        let repo = self.album_repo();
        let album_model = album.to_album();
        future_to_promise(async move {
            repo.update(&album_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update album: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete an album by ID
    #[wasm_bindgen(js_name = deleteAlbum)]
    pub fn delete_album(&self, id: String) -> Promise {
        let repo = self.album_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete album: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }

    /// List albums with pagination
    #[wasm_bindgen(js_name = listAlbums)]
    pub fn list_albums(&self, page_request: JsPageRequest) -> Promise {
        let repo = self.album_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query(page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to list albums: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Search albums
    #[wasm_bindgen(js_name = searchAlbums)]
    pub fn search_albums(&self, query: String, page_request: JsPageRequest) -> Promise {
        let repo = self.album_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .search(&query, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to search albums: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Count total albums
    #[wasm_bindgen(js_name = countAlbums)]
    pub fn count_albums(&self) -> Promise {
        let repo = self.album_repo();
        future_to_promise(async move {
            let count = repo
                .count()
                .await
                .map_err(|e| to_js_error(format!("Failed to count albums: {}", e)))?;
            Ok(JsValue::from_f64(count as f64))
        })
    }

    // =============================================================================
    // Artist Operations
    // =============================================================================

    /// Insert an artist
    #[wasm_bindgen(js_name = insertArtist)]
    pub fn insert_artist(&self, artist: &JsArtist) -> Promise {
        let repo = self.artist_repo();
        let artist_model = artist.to_artist();
        future_to_promise(async move {
            repo.insert(&artist_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert artist: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get an artist by ID
    #[wasm_bindgen(js_name = getArtist)]
    pub fn get_artist(&self, id: String) -> Promise {
        let repo = self.artist_repo();
        future_to_promise(async move {
            let artist = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get artist: {}", e)))?;

            Ok(artist.map(JsArtist::from_artist).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update an artist
    #[wasm_bindgen(js_name = updateArtist)]
    pub fn update_artist(&self, artist: &JsArtist) -> Promise {
        let repo = self.artist_repo();
        let artist_model = artist.to_artist();
        future_to_promise(async move {
            repo.update(&artist_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update artist: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete an artist by ID
    #[wasm_bindgen(js_name = deleteArtist)]
    pub fn delete_artist(&self, id: String) -> Promise {
        let repo = self.artist_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete artist: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }

    /// List artists with pagination
    #[wasm_bindgen(js_name = listArtists)]
    pub fn list_artists(&self, page_request: JsPageRequest) -> Promise {
        let repo = self.artist_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query(page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to list artists: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Search artists
    #[wasm_bindgen(js_name = searchArtists)]
    pub fn search_artists(&self, query: String, page_request: JsPageRequest) -> Promise {
        let repo = self.artist_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .search(&query, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to search artists: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Count total artists
    #[wasm_bindgen(js_name = countArtists)]
    pub fn count_artists(&self) -> Promise {
        let repo = self.artist_repo();
        future_to_promise(async move {
            let count = repo
                .count()
                .await
                .map_err(|e| to_js_error(format!("Failed to count artists: {}", e)))?;
            Ok(JsValue::from_f64(count as f64))
        })
    }

    // =============================================================================
    // Playlist Operations
    // =============================================================================

    /// Insert a playlist
    #[wasm_bindgen(js_name = insertPlaylist)]
    pub fn insert_playlist(&self, playlist: &JsPlaylist) -> Promise {
        let repo = self.playlist_repo();
        let playlist_model = playlist.to_playlist();
        future_to_promise(async move {
            repo.insert(&playlist_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert playlist: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get a playlist by ID
    #[wasm_bindgen(js_name = getPlaylist)]
    pub fn get_playlist(&self, id: String) -> Promise {
        let repo = self.playlist_repo();
        future_to_promise(async move {
            let playlist = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get playlist: {}", e)))?;

            Ok(playlist.map(JsPlaylist::from_playlist).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update a playlist
    #[wasm_bindgen(js_name = updatePlaylist)]
    pub fn update_playlist(&self, playlist: &JsPlaylist) -> Promise {
        let repo = self.playlist_repo();
        let playlist_model = playlist.to_playlist();
        future_to_promise(async move {
            repo.update(&playlist_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update playlist: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete a playlist by ID
    #[wasm_bindgen(js_name = deletePlaylist)]
    pub fn delete_playlist(&self, id: String) -> Promise {
        let repo = self.playlist_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete playlist: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }

    /// Add a track to a playlist
    #[wasm_bindgen(js_name = addTrackToPlaylist)]
    pub fn add_track_to_playlist(&self, playlist_id: String, track_id: String, position: i32) -> Promise {
        let repo = self.playlist_repo();
        future_to_promise(async move {
            repo.add_track(&playlist_id, &track_id, position)
                .await
                .map_err(|e| to_js_error(format!("Failed to add track to playlist: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Remove a track from a playlist
    #[wasm_bindgen(js_name = removeTrackFromPlaylist)]
    pub fn remove_track_from_playlist(&self, playlist_id: String, track_id: String) -> Promise {
        let repo = self.playlist_repo();
        future_to_promise(async move {
            let removed = repo
                .remove_track(&playlist_id, &track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to remove track from playlist: {}", e)))?;
            Ok(JsValue::from_bool(removed))
        })
    }

    /// List playlists with pagination
    #[wasm_bindgen(js_name = listPlaylists)]
    pub fn list_playlists(&self, page_request: JsPageRequest) -> Promise {
        let repo = self.playlist_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query(page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to list playlists: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    // =============================================================================
    // Folder Operations
    // =============================================================================

    /// Insert a folder
    #[wasm_bindgen(js_name = insertFolder)]
    pub fn insert_folder(&self, folder: &JsFolder) -> Promise {
        let repo = self.folder_repo();
        let folder_model = folder.to_folder();
        future_to_promise(async move {
            repo.insert(&folder_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert folder: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get a folder by ID
    #[wasm_bindgen(js_name = getFolder)]
    pub fn get_folder(&self, id: String) -> Promise {
        let repo = self.folder_repo();
        future_to_promise(async move {
            let folder = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get folder: {}", e)))?;

            Ok(folder.map(JsFolder::from_folder).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// List folders with pagination
    #[wasm_bindgen(js_name = listFolders)]
    pub fn list_folders(&self, page_request: JsPageRequest) -> Promise {
        let repo = self.folder_repo();
        let page_req = page_request.into();
        future_to_promise(async move {
            let page = repo
                .query(page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to list folders: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    // =============================================================================
    // Lyrics Operations
    // =============================================================================

    /// Insert lyrics for a track
    #[wasm_bindgen(js_name = insertLyrics)]
    pub fn insert_lyrics(&self, lyrics: &JsLyrics) -> Promise {
        let repo = self.lyrics_repo();
        let lyrics_model = lyrics.to_lyrics();
        future_to_promise(async move {
            repo.insert(&lyrics_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert lyrics: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get lyrics for a track
    #[wasm_bindgen(js_name = getLyrics)]
    pub fn get_lyrics(&self, track_id: String) -> Promise {
        let repo = self.lyrics_repo();
        future_to_promise(async move {
            let lyrics = repo
                .find_by_track_id(&track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get lyrics: {}", e)))?;

            Ok(lyrics.map(JsLyrics::from_lyrics).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update lyrics
    #[wasm_bindgen(js_name = updateLyrics)]
    pub fn update_lyrics(&self, lyrics: &JsLyrics) -> Promise {
        let repo = self.lyrics_repo();
        let lyrics_model = lyrics.to_lyrics();
        future_to_promise(async move {
            repo.update(&lyrics_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update lyrics: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete lyrics for a track
    #[wasm_bindgen(js_name = deleteLyrics)]
    pub fn delete_lyrics(&self, track_id: String) -> Promise {
        let repo = self.lyrics_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete lyrics: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }
}

impl JsLibrary {
    /// Clone the underlying database adapter for internal Rust consumers.
    pub fn adapter_handle(&self) -> Rc<dyn DatabaseAdapter> {
        self.adapter_rc.clone()
    }
}

// =============================================================================
// Filter and Sort Enums - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible TrackSort wrapper
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct JsTrackSort {
    inner: crate::query::TrackSort,
}

#[wasm_bindgen]
impl JsTrackSort {
    #[wasm_bindgen(js_name = titleAsc)]
    pub fn title_asc() -> Self {
        Self {
            inner: crate::query::TrackSort::TitleAsc,
        }
    }

    #[wasm_bindgen(js_name = titleDesc)]
    pub fn title_desc() -> Self {
        Self {
            inner: crate::query::TrackSort::TitleDesc,
        }
    }

    #[wasm_bindgen(js_name = createdAtDesc)]
    pub fn created_at_desc() -> Self {
        Self {
            inner: crate::query::TrackSort::CreatedAtDesc,
        }
    }

    #[wasm_bindgen(js_name = createdAtAsc)]
    pub fn created_at_asc() -> Self {
        Self {
            inner: crate::query::TrackSort::CreatedAtAsc,
        }
    }

    #[wasm_bindgen(js_name = durationDesc)]
    pub fn duration_desc() -> Self {
        Self {
            inner: crate::query::TrackSort::DurationDesc,
        }
    }

    #[wasm_bindgen(js_name = durationAsc)]
    pub fn duration_asc() -> Self {
        Self {
            inner: crate::query::TrackSort::DurationAsc,
        }
    }
}

impl From<JsTrackSort> for crate::query::TrackSort {
    fn from(sort: JsTrackSort) -> Self {
        sort.inner
    }
}

/// JavaScript-accessible AlbumSort wrapper
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct JsAlbumSort {
    inner: crate::query::AlbumSort,
}

#[wasm_bindgen]
impl JsAlbumSort {
    #[wasm_bindgen(js_name = nameAsc)]
    pub fn name_asc() -> Self {
        Self {
            inner: crate::query::AlbumSort::NameAsc,
        }
    }

    #[wasm_bindgen(js_name = nameDesc)]
    pub fn name_desc() -> Self {
        Self {
            inner: crate::query::AlbumSort::NameDesc,
        }
    }

    #[wasm_bindgen(js_name = yearDesc)]
    pub fn year_desc() -> Self {
        Self {
            inner: crate::query::AlbumSort::YearDesc,
        }
    }

    #[wasm_bindgen(js_name = yearAsc)]
    pub fn year_asc() -> Self {
        Self {
            inner: crate::query::AlbumSort::YearAsc,
        }
    }

    #[wasm_bindgen(js_name = updatedAtDesc)]
    pub fn updated_at_desc() -> Self {
        Self {
            inner: crate::query::AlbumSort::UpdatedAtDesc,
        }
    }

    #[wasm_bindgen(js_name = trackCountDesc)]
    pub fn track_count_desc() -> Self {
        Self {
            inner: crate::query::AlbumSort::TrackCountDesc,
        }
    }
}

impl From<JsAlbumSort> for crate::query::AlbumSort {
    fn from(sort: JsAlbumSort) -> Self {
        sort.inner
    }
}

/// JavaScript-accessible TrackFilter builder
#[wasm_bindgen]
pub struct JsTrackFilter {
    inner: crate::query::TrackFilter,
}

#[wasm_bindgen]
impl JsTrackFilter {
    /// Create a new empty filter with default sort
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: crate::query::TrackFilter::default(),
        }
    }

    /// Set album filter
    #[wasm_bindgen(js_name = setAlbumId)]
    pub fn set_album_id(&mut self, album_id: Option<String>) {
        self.inner.album_id = album_id;
    }

    /// Set artist filter
    #[wasm_bindgen(js_name = setArtistId)]
    pub fn set_artist_id(&mut self, artist_id: Option<String>) {
        self.inner.artist_id = artist_id;
    }

    /// Set album artist filter
    #[wasm_bindgen(js_name = setAlbumArtistId)]
    pub fn set_album_artist_id(&mut self, album_artist_id: Option<String>) {
        self.inner.album_artist_id = album_artist_id;
    }

    /// Set playlist filter
    #[wasm_bindgen(js_name = setPlaylistId)]
    pub fn set_playlist_id(&mut self, playlist_id: Option<String>) {
        self.inner.playlist_id = playlist_id;
    }

    /// Set provider filter
    #[wasm_bindgen(js_name = setProviderId)]
    pub fn set_provider_id(&mut self, provider_id: Option<String>) {
        self.inner.provider_id = provider_id;
    }

    /// Set genre filter
    #[wasm_bindgen(js_name = setGenre)]
    pub fn set_genre(&mut self, genre: Option<String>) {
        self.inner.genre = genre;
    }

    /// Set year filter
    #[wasm_bindgen(js_name = setYear)]
    pub fn set_year(&mut self, year: Option<i32>) {
        self.inner.year = year;
    }

    /// Set minimum duration filter (milliseconds)
    #[wasm_bindgen(js_name = setMinDurationMs)]
    pub fn set_min_duration_ms(&mut self, min_duration_ms: Option<f64>) {
        self.inner.min_duration_ms = min_duration_ms.map(|d| d as i64);
    }

    /// Set maximum duration filter (milliseconds)
    #[wasm_bindgen(js_name = setMaxDurationMs)]
    pub fn set_max_duration_ms(&mut self, max_duration_ms: Option<f64>) {
        self.inner.max_duration_ms = max_duration_ms.map(|d| d as i64);
    }

    /// Set search query
    #[wasm_bindgen(js_name = setSearch)]
    pub fn set_search(&mut self, search: Option<String>) {
        self.inner.search = search;
    }

    /// Set sort order
    #[wasm_bindgen(js_name = setSort)]
    pub fn set_sort(&mut self, sort: JsTrackSort) {
        self.inner.sort = sort.into();
    }
}

impl From<JsTrackFilter> for crate::query::TrackFilter {
    fn from(filter: JsTrackFilter) -> Self {
        filter.inner
    }
}

/// JavaScript-accessible AlbumFilter builder
#[wasm_bindgen]
pub struct JsAlbumFilter {
    inner: crate::query::AlbumFilter,
}

#[wasm_bindgen]
impl JsAlbumFilter {
    /// Create a new empty filter with default sort
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: crate::query::AlbumFilter::default(),
        }
    }

    /// Set artist filter
    #[wasm_bindgen(js_name = setArtistId)]
    pub fn set_artist_id(&mut self, artist_id: Option<String>) {
        self.inner.artist_id = artist_id;
    }

    /// Set genre filter
    #[wasm_bindgen(js_name = setGenre)]
    pub fn set_genre(&mut self, genre: Option<String>) {
        self.inner.genre = genre;
    }

    /// Set minimum year filter
    #[wasm_bindgen(js_name = setMinYear)]
    pub fn set_min_year(&mut self, min_year: Option<i32>) {
        self.inner.min_year = min_year;
    }

    /// Set maximum year filter
    #[wasm_bindgen(js_name = setMaxYear)]
    pub fn set_max_year(&mut self, max_year: Option<i32>) {
        self.inner.max_year = max_year;
    }

    /// Set search query
    #[wasm_bindgen(js_name = setSearch)]
    pub fn set_search(&mut self, search: Option<String>) {
        self.inner.search = search;
    }

    /// Set sort order
    #[wasm_bindgen(js_name = setSort)]
    pub fn set_sort(&mut self, sort: JsAlbumSort) {
        self.inner.sort = sort.into();
    }
}

impl From<JsAlbumFilter> for crate::query::AlbumFilter {
    fn from(filter: JsAlbumFilter) -> Self {
        filter.inner
    }
}

// =============================================================================
// Cache Models - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible CacheStatus enum
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct JsCacheStatus {
    inner: CacheStatus,
}

#[wasm_bindgen]
impl JsCacheStatus {
    #[wasm_bindgen(js_name = notCached)]
    pub fn not_cached() -> Self {
        Self {
            inner: CacheStatus::NotCached,
        }
    }

    pub fn downloading() -> Self {
        Self {
            inner: CacheStatus::Downloading,
        }
    }

    pub fn cached() -> Self {
        Self {
            inner: CacheStatus::Cached,
        }
    }

    pub fn failed() -> Self {
        Self {
            inner: CacheStatus::Failed,
        }
    }

    pub fn stale() -> Self {
        Self {
            inner: CacheStatus::Stale,
        }
    }

    #[wasm_bindgen(js_name = isAvailable)]
    pub fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    #[wasm_bindgen(js_name = isDownloading)]
    pub fn is_downloading(&self) -> bool {
        self.inner.is_downloading()
    }

    #[wasm_bindgen(js_name = needsDownload)]
    pub fn needs_download(&self) -> bool {
        self.inner.needs_download()
    }
}

/// JavaScript-accessible CachedTrack wrapper
#[wasm_bindgen]
pub struct JsCachedTrack {
    inner: CachedTrack,
}

#[wasm_bindgen]
impl JsCachedTrack {
    /// Create a new cache entry
    #[wasm_bindgen(constructor)]
    pub fn new(track_id: String, cache_path: String, file_size: f64) -> std::result::Result<JsCachedTrack, JsValue> {
        let track_id = TrackId::from_string(&track_id).map_err(to_js_error)?;
        Ok(Self {
            inner: CachedTrack::new(track_id, cache_path, file_size as u64),
        })
    }

    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsCachedTrack, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsCachedTrack { inner })
            .map_err(to_js_error)
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> std::result::Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner).map_err(to_js_error)
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> std::result::Result<JsCachedTrack, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsCachedTrack { inner })
            .map_err(to_js_error)
    }

    // Getters
    #[wasm_bindgen(js_name = trackId)]
    pub fn track_id(&self) -> String {
        self.inner.track_id.to_string()
    }

    #[wasm_bindgen(js_name = cachePath)]
    pub fn cache_path(&self) -> String {
        self.inner.cache_path.clone()
    }

    #[wasm_bindgen(js_name = fileSize)]
    pub fn file_size(&self) -> f64 {
        self.inner.file_size as f64
    }

    #[wasm_bindgen(js_name = cachedSize)]
    pub fn cached_size(&self) -> f64 {
        self.inner.cached_size as f64
    }

    #[wasm_bindgen(js_name = downloadProgress)]
    pub fn download_progress(&self) -> u8 {
        self.inner.download_progress()
    }
}

// Internal conversion
impl JsCachedTrack {
    pub(crate) fn from_cached_track(track: CachedTrack) -> Self {
        Self { inner: track }
    }

    pub(crate) fn to_cached_track(&self) -> CachedTrack {
        self.inner.clone()
    }
}

/// JavaScript-accessible Artwork wrapper
#[wasm_bindgen]
pub struct JsArtwork {
    inner: Artwork,
}

#[wasm_bindgen]
impl JsArtwork {
    /// Create new artwork
    #[wasm_bindgen(constructor)]
    pub fn new(hash: String, binary_data: Vec<u8>, width: f64, height: f64, mime_type: String) -> Self {
        Self {
            inner: Artwork::new(hash, binary_data, width as i64, height as i64, mime_type),
        }
    }

    /// Convert to JSON string (without binary blob)
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> std::result::Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(to_js_error)
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> std::result::Result<JsArtwork, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsArtwork { inner })
            .map_err(to_js_error)
    }

    /// Validate the artwork
    pub fn validate(&self) -> std::result::Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    // Getters
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    pub fn hash(&self) -> String {
        self.inner.hash.clone()
    }

    #[wasm_bindgen(js_name = binaryBlob)]
    pub fn binary_blob(&self) -> Vec<u8> {
        self.inner.binary_blob.clone()
    }

    #[wasm_bindgen(js_name = mimeType)]
    pub fn mime_type(&self) -> String {
        self.inner.mime_type.clone()
    }

    pub fn width(&self) -> f64 {
        self.inner.width as f64
    }

    pub fn height(&self) -> f64 {
        self.inner.height as f64
    }

    #[wasm_bindgen(js_name = fileSize)]
    pub fn file_size(&self) -> f64 {
        self.inner.file_size as f64
    }
}

// Internal conversion
impl JsArtwork {
    pub(crate) fn from_artwork(artwork: Artwork) -> Self {
        Self { inner: artwork }
    }

    pub(crate) fn to_artwork(&self) -> Artwork {
        self.inner.clone()
    }
}

// =============================================================================
// LibraryQueryService - High-Level Query APIs
// =============================================================================

/// JavaScript-accessible LibraryQueryService wrapper
#[wasm_bindgen]
pub struct JsQueryService {
    adapter: Rc<dyn DatabaseAdapter>,
}

#[wasm_bindgen]
impl JsQueryService {
    /// Create a new query service from a library instance
    #[wasm_bindgen(js_name = fromLibrary)]
    pub fn from_library(library: &JsLibrary) -> Self {
        Self {
            adapter: library.adapter_rc.clone(),
        }
    }

    /// Query tracks with filtering, sorting, and pagination
    #[wasm_bindgen(js_name = queryTracks)]
    pub fn query_tracks(&self, filter: JsTrackFilter, page_request: JsPageRequest) -> Promise {
        use crate::query::LibraryQueryService;
        let service = LibraryQueryService::new(self.adapter.clone());
        let filter = filter.into();
        let page_req = page_request.into();

        future_to_promise(async move {
            let page = service
                .query_tracks(filter, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to query tracks: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Query albums with filtering, sorting, and pagination
    #[wasm_bindgen(js_name = queryAlbums)]
    pub fn query_albums(&self, filter: JsAlbumFilter, page_request: JsPageRequest) -> Promise {
        use crate::query::LibraryQueryService;
        let service = LibraryQueryService::new(self.adapter.clone());
        let filter = filter.into();
        let page_req = page_request.into();

        future_to_promise(async move {
            let page = service
                .query_albums(filter, page_req)
                .await
                .map_err(|e| to_js_error(format!("Failed to query albums: {}", e)))?;

            serde_wasm_bindgen::to_value(&page).map_err(to_js_error)
        })
    }

    /// Perform full-text search across all entities
    pub fn search(&self, query: String) -> Promise {
        use crate::query::LibraryQueryService;
        let service = LibraryQueryService::new(self.adapter.clone());

        future_to_promise(async move {
            let results = service
                .search(&query)
                .await
                .map_err(|e| to_js_error(format!("Search failed: {}", e)))?;

            serde_wasm_bindgen::to_value(&results).map_err(to_js_error)
        })
    }

    /// Get detailed track information with all relations loaded
    #[wasm_bindgen(js_name = getTrackDetails)]
    pub fn get_track_details(&self, track_id: String) -> Promise {
        use crate::query::LibraryQueryService;
        let service = LibraryQueryService::new(self.adapter.clone());

        future_to_promise(async move {
            let details = service
                .get_track_details(&track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get track details: {}", e)))?;

            serde_wasm_bindgen::to_value(&details).map_err(to_js_error)
        })
    }
}

// =============================================================================
// Extended JsLibrary Methods for Cache and Artwork
// =============================================================================

#[wasm_bindgen]
impl JsLibrary {
    // Helper methods for cache and artwork repositories
    fn cache_repo(&self) -> SqliteCacheMetadataRepository {
        SqliteCacheMetadataRepository::new(self.adapter_rc.clone())
    }

    fn artwork_repo(&self) -> SqliteArtworkRepository {
        SqliteArtworkRepository::new(self.adapter_rc.clone())
    }

    // =============================================================================
    // Cache Operations
    // =============================================================================

    /// Insert a cached track entry
    #[wasm_bindgen(js_name = insertCachedTrack)]
    pub fn insert_cached_track(&self, track: &JsCachedTrack) -> Promise {
        let repo = self.cache_repo();
        let track_model = track.to_cached_track();
        future_to_promise(async move {
            repo.insert(&track_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert cached track: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get a cached track by track ID
    #[wasm_bindgen(js_name = getCachedTrack)]
    pub fn get_cached_track(&self, track_id: String) -> Promise {
        let repo = self.cache_repo();
        future_to_promise(async move {
            let track_id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            let track = repo
                .find_by_track_id(&track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get cached track: {}", e)))?;

            Ok(track.map(JsCachedTrack::from_cached_track).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update a cached track entry
    #[wasm_bindgen(js_name = updateCachedTrack)]
    pub fn update_cached_track(&self, track: &JsCachedTrack) -> Promise {
        let repo = self.cache_repo();
        let track_model = track.to_cached_track();
        future_to_promise(async move {
            repo.update(&track_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update cached track: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete a cached track entry
    #[wasm_bindgen(js_name = deleteCachedTrack)]
    pub fn delete_cached_track(&self, track_id: String) -> Promise {
        let repo = self.cache_repo();
        future_to_promise(async move {
            let track_id = TrackId::from_string(&track_id).map_err(to_js_error)?;
            repo.delete(&track_id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete cached track: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get cache statistics
    #[wasm_bindgen(js_name = getCacheStats)]
    pub fn get_cache_stats(&self) -> Promise {
        let repo = self.cache_repo();
        future_to_promise(async move {
            let stats = repo
                .get_stats()
                .await
                .map_err(|e| to_js_error(format!("Failed to get cache stats: {}", e)))?;

            serde_wasm_bindgen::to_value(&stats).map_err(to_js_error)
        })
    }

    /// Get all cached tracks
    #[wasm_bindgen(js_name = getAllCachedTracks)]
    pub fn get_all_cached_tracks(&self) -> Promise {
        let repo = self.cache_repo();
        future_to_promise(async move {
            let tracks = repo
                .find_all()
                .await
                .map_err(|e| to_js_error(format!("Failed to get all cached tracks: {}", e)))?;

            serde_wasm_bindgen::to_value(&tracks).map_err(to_js_error)
        })
    }

    // =============================================================================
    // Artwork Operations
    // =============================================================================

    /// Insert artwork
    #[wasm_bindgen(js_name = insertArtwork)]
    pub fn insert_artwork(&self, artwork: &JsArtwork) -> Promise {
        let repo = self.artwork_repo();
        let artwork_model = artwork.to_artwork();
        future_to_promise(async move {
            repo.insert(&artwork_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to insert artwork: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Get artwork by ID
    #[wasm_bindgen(js_name = getArtwork)]
    pub fn get_artwork(&self, id: String) -> Promise {
        let repo = self.artwork_repo();
        future_to_promise(async move {
            let artwork = repo
                .find_by_id(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to get artwork: {}", e)))?;

            Ok(artwork.map(JsArtwork::from_artwork).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Get artwork by content hash
    #[wasm_bindgen(js_name = getArtworkByHash)]
    pub fn get_artwork_by_hash(&self, hash: String) -> Promise {
        let repo = self.artwork_repo();
        future_to_promise(async move {
            let artwork = repo
                .find_by_hash(&hash)
                .await
                .map_err(|e| to_js_error(format!("Failed to get artwork by hash: {}", e)))?;

            Ok(artwork.map(JsArtwork::from_artwork).map(JsValue::from).unwrap_or(JsValue::NULL))
        })
    }

    /// Update artwork
    #[wasm_bindgen(js_name = updateArtwork)]
    pub fn update_artwork(&self, artwork: &JsArtwork) -> Promise {
        let repo = self.artwork_repo();
        let artwork_model = artwork.to_artwork();
        future_to_promise(async move {
            repo.update(&artwork_model)
                .await
                .map_err(|e| to_js_error(format!("Failed to update artwork: {}", e)))?;
            Ok(JsValue::NULL)
        })
    }

    /// Delete artwork by ID
    #[wasm_bindgen(js_name = deleteArtwork)]
    pub fn delete_artwork(&self, id: String) -> Promise {
        let repo = self.artwork_repo();
        future_to_promise(async move {
            let deleted = repo
                .delete(&id)
                .await
                .map_err(|e| to_js_error(format!("Failed to delete artwork: {}", e)))?;
            Ok(JsValue::from_bool(deleted))
        })
    }

    /// Count total artworks
    #[wasm_bindgen(js_name = countArtworks)]
    pub fn count_artworks(&self) -> Promise {
        let repo = self.artwork_repo();
        future_to_promise(async move {
            let count = repo
                .count()
                .await
                .map_err(|e| to_js_error(format!("Failed to count artworks: {}", e)))?;
            Ok(JsValue::from_f64(count as f64))
        })
    }

    /// Get total storage size of all artworks
    #[wasm_bindgen(js_name = getArtworksTotalSize)]
    pub fn get_artworks_total_size(&self) -> Promise {
        let repo = self.artwork_repo();
        future_to_promise(async move {
            let size = repo
                .total_size()
                .await
                .map_err(|e| to_js_error(format!("Failed to get artworks total size: {}", e)))?;
            Ok(JsValue::from_f64(size as f64))
        })
    }
}
