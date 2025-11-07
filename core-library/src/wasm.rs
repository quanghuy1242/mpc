//! WebAssembly bindings for core-library
//!
//! This module provides JavaScript/TypeScript-friendly bindings for the core-library models
//! and operations using wasm-bindgen.

use crate::models::*;
use wasm_bindgen::prelude::*;

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
    pub fn from_string(s: &str) -> Result<JsTrackId, JsValue> {
        TrackId::from_string(s)
            .map(|inner| JsTrackId { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
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
    pub fn from_string(s: &str) -> Result<JsAlbumId, JsValue> {
        AlbumId::from_string(s)
            .map(|inner| JsAlbumId { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
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
    pub fn from_string(s: &str) -> Result<JsArtistId, JsValue> {
        ArtistId::from_string(s)
            .map(|inner| JsArtistId { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
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
    pub fn from_string(s: &str) -> Result<JsPlaylistId, JsValue> {
        PlaylistId::from_string(s)
            .map(|inner| JsPlaylistId { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        self.inner.to_string()
    }
}

// =============================================================================
// Track - Exported to JavaScript
// =============================================================================

/// JavaScript-accessible Track wrapper
#[wasm_bindgen]
pub struct JsTrack {
    inner: Track,
}

#[wasm_bindgen]
impl JsTrack {
    /// Convert to JSON string
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<JsTrack, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsTrack { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Convert to JsValue for JavaScript interop
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> Result<JsTrack, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsTrack { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate the track
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner
            .validate()
            .map_err(|e| JsValue::from_str(&e))
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
}

// =============================================================================
// Album - Exported to JavaScript
// =============================================================================

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
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<JsAlbum, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsAlbum { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> Result<JsAlbum, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsAlbum { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate the album
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner
            .validate()
            .map_err(|e| JsValue::from_str(&e))
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
}

// =============================================================================
// Artist - Exported to JavaScript
// =============================================================================

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
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<JsArtist, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsArtist { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> Result<JsArtist, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsArtist { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate the artist
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner
            .validate()
            .map_err(|e| JsValue::from_str(&e))
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
}

// Internal conversion methods (not exported to JS)
impl JsArtist {
    pub(crate) fn from_artist(artist: Artist) -> Self {
        Self { inner: artist }
    }

    pub(crate) fn to_artist(&self) -> Artist {
        self.inner.clone()
    }
}

// =============================================================================
// Playlist - Exported to JavaScript
// =============================================================================

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
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JSON string
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<JsPlaylist, JsValue> {
        serde_json::from_str(json)
            .map(|inner| JsPlaylist { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Convert to JsValue
    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Create from JsValue
    #[wasm_bindgen(js_name = fromObject)]
    pub fn from_object(obj: JsValue) -> Result<JsPlaylist, JsValue> {
        serde_wasm_bindgen::from_value(obj)
            .map(|inner| JsPlaylist { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate the playlist
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner
            .validate()
            .map_err(|e| JsValue::from_str(&e))
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
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Get the library version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get the library name
#[wasm_bindgen]
pub fn name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

// =============================================================================
// Database/Storage Layer - Using Repository Pattern
// =============================================================================

use crate::repositories::{ArtistRepository, SqliteArtistRepository};
use bridge_traits::database::{DatabaseAdapter, DatabaseConfig};
use bridge_wasm::database::WasmDbAdapter;
use std::sync::Arc;

/// JavaScript-accessible database wrapper using proper repositories
#[wasm_bindgen]
pub struct JsDatabase {
    adapter: Arc<dyn DatabaseAdapter>,
    artist_repo: SqliteArtistRepository,
}

#[wasm_bindgen]
impl JsDatabase {
    /// Create a new database instance with the given database URL
    pub async fn create(database_url: String) -> Result<JsDatabase, JsValue> {
        let config = DatabaseConfig::new(&database_url);
        let adapter = WasmDbAdapter::new(config)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to create database: {}", e)))?;
        
        let adapter_arc = Arc::new(adapter) as Arc<dyn DatabaseAdapter>;
        let artist_repo = SqliteArtistRepository::new(adapter_arc.clone());
        
        Ok(Self {
            adapter: adapter_arc,
            artist_repo,
        })
    }

    /// Initialize the database (run migrations, create tables)
    pub async fn initialize(&self) -> Result<(), JsValue> {
        // NOTE: Mutations need to be handled through Arc properly or use interior mutability
        // For now, initialization should be done by the JavaScript bridge
        self.adapter
            .health_check()
            .await
            .map_err(|e| JsValue::from_str(&format!("Health check failed: {}", e)))
    }

    /// Check database health
    #[wasm_bindgen(js_name = healthCheck)]
    pub async fn health_check(&self) -> Result<(), JsValue> {
        self.adapter
            .health_check()
            .await
            .map_err(|e| JsValue::from_str(&format!("Health check failed: {}", e)))
    }

    // =============================================================================
    // Artist Operations - Using SqliteArtistRepository (NO RAW SQL!)
    // =============================================================================

    /// Insert an artist into the database
    #[wasm_bindgen(js_name = insertArtist)]
    pub async fn insert_artist(&self, artist: &JsArtist) -> Result<(), JsValue> {
        let artist_model = artist.to_artist();
        
        self.artist_repo
            .insert(&artist_model)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to insert artist: {}", e)))
    }

    /// Get an artist by ID
    #[wasm_bindgen(js_name = getArtist)]
    pub async fn get_artist(&self, id: String) -> Result<Option<JsArtist>, JsValue> {
        let artist = self.artist_repo
            .find_by_id(&id)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get artist: {}", e)))?;

        Ok(artist.map(JsArtist::from_artist))
    }

    /// List all artists (with pagination)
    #[wasm_bindgen(js_name = listArtists)]
    pub async fn list_artists(&self) -> Result<Vec<JsArtist>, JsValue> {
        use crate::repositories::PageRequest;
        
        // Get first 1000 artists (TODO: add pagination parameters to JS API)
        let page_request = PageRequest::new(0, 1000);
        
        let page = self.artist_repo
            .query(page_request)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to list artists: {}", e)))?;

        Ok(page.items.into_iter().map(JsArtist::from_artist).collect())
    }

    /// Delete an artist by ID
    #[wasm_bindgen(js_name = deleteArtist)]
    pub async fn delete_artist(&self, id: String) -> Result<bool, JsValue> {
        self.artist_repo
            .delete(&id)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to delete artist: {}", e)))
    }
    
    /// Count total artists
    #[wasm_bindgen(js_name = countArtists)]
    pub async fn count_artists(&self) -> Result<f64, JsValue> {
        let count = self.artist_repo
            .count()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to count artists: {}", e)))?;
        
        Ok(count as f64)
    }

    /// Search artists by name
    #[wasm_bindgen(js_name = searchArtists)]
    pub async fn search_artists(&self, query: String, limit: u32) -> Result<Vec<JsArtist>, JsValue> {
        use crate::repositories::PageRequest;
        
        let page_request = PageRequest::new(0, limit);
        
        let page = self.artist_repo
            .search(&query, page_request)
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to search artists: {}", e)))?;

        Ok(page.items.into_iter().map(JsArtist::from_artist).collect())
    }
}
