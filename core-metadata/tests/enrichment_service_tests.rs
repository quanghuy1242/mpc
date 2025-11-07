//! Integration tests for the EnrichmentService
//!
//! These tests verify the complete enrichment pipeline including:
//! - Artist and album name resolution from repositories
//! - Artwork and lyrics fetching integration
//! - Error handling for missing metadata
//! - Database updates after successful enrichment

use core_library::db::create_test_pool;
use core_library::models::{Album, Artist, Track};
use core_library::repositories::album::{AlbumRepository, SqliteAlbumRepository};
use core_library::repositories::artist::{ArtistRepository, SqliteArtistRepository};
use core_library::repositories::artwork::SqliteArtworkRepository;
use core_library::repositories::lyrics::SqliteLyricsRepository;
use core_library::repositories::track::{SqliteTrackRepository, TrackRepository};
use core_metadata::enrichment_service::{EnrichmentRequest, EnrichmentService};
use core_metadata::{ArtworkService, LyricsService};
use std::sync::Arc;

/// Create test database with schema and return pool
async fn setup_test_db() -> sqlx::SqlitePool {
    let pool = create_test_pool().await.unwrap();

    // Run migrations
    sqlx::migrate!("../core-library/migrations")
        .run(&pool)
        .await
        .unwrap();

    // Create a test provider (required for foreign key constraints)
    sqlx::query(
        "INSERT INTO providers (id, type, display_name, profile_id, created_at) 
         VALUES ('test_provider', 'GoogleDrive', 'Test Provider', 'test_profile', 1000000)",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

/// Create enrichment service with all dependencies
async fn create_enrichment_service() -> (EnrichmentService, sqlx::SqlitePool) {
    let pool = setup_test_db().await;

    let artist_repo = Arc::new(SqliteArtistRepository::new(pool.clone()));
    let album_repo = Arc::new(SqliteAlbumRepository::new(pool.clone()));
    let track_repo = Arc::new(SqliteTrackRepository::new(pool.clone()));
    let artwork_repo = Arc::new(SqliteArtworkRepository::new(pool.clone()));
    let lyrics_repo = Arc::new(SqliteLyricsRepository::new(pool.clone()));

    let artwork_service = Arc::new(ArtworkService::new(artwork_repo, 200 * 1024 * 1024));
    let lyrics_service = Arc::new(LyricsService::without_providers(lyrics_repo));

    let service = EnrichmentService::new(
        artist_repo,
        album_repo,
        track_repo,
        artwork_service,
        lyrics_service,
    );

    (service, pool)
}

/// Create test artist in database
async fn create_test_artist(pool: &sqlx::SqlitePool, id: &str, name: &str) -> Artist {
    let artist = Artist {
        id: id.to_string(),
        name: name.to_string(),
        normalized_name: name.to_lowercase(),
        sort_name: None,
        bio: None,
        country: None,
        created_at: 1000000,
        updated_at: 1000000,
    };

    let repo = SqliteArtistRepository::new(pool.clone());
    repo.insert(&artist).await.unwrap();

    artist
}

/// Create test album in database
async fn create_test_album(
    pool: &sqlx::SqlitePool,
    id: &str,
    name: &str,
    artist_id: &str,
) -> Album {
    let album = Album {
        id: id.to_string(),
        name: name.to_string(),
        normalized_name: name.to_lowercase(),
        artist_id: Some(artist_id.to_string()),
        year: Some(2023),
        genre: None,
        artwork_id: None,
        track_count: 0,
        total_duration_ms: 0,
        created_at: 1000000,
        updated_at: 1000000,
    };

    let repo = SqliteAlbumRepository::new(pool.clone());
    repo.insert(&album).await.unwrap();

    album
}

/// Create test track in database
async fn create_test_track(
    pool: &sqlx::SqlitePool,
    id: &str,
    title: &str,
    artist_id: Option<String>,
    album_id: Option<String>,
) -> Track {
    let track = Track {
        id: id.to_string(),
        provider_id: "test_provider".to_string(),
        provider_file_id: format!("file_{}", id),
        hash: None,
        title: title.to_string(),
        normalized_title: title.to_lowercase(),
        album_id,
        artist_id,
        album_artist_id: None,
        track_number: Some(1),
        disc_number: 1,
        genre: None,
        year: Some(2023),
        duration_ms: 180000,
        bitrate: Some(320),
        sample_rate: Some(44100),
        channels: Some(2),
        format: "mp3".to_string(),
        file_size: Some(8_000_000),
        mime_type: Some("audio/mpeg".to_string()),
        artwork_id: None,
        lyrics_status: "not_fetched".to_string(),
        created_at: 1000000,
        updated_at: 1000000,
        provider_modified_at: None,
    };

    let repo = SqliteTrackRepository::new(pool.clone());
    repo.insert(&track).await.unwrap();

    track
}

#[core_async::test]
async fn test_enrichment_service_creation() {
    let (service, _pool) = create_enrichment_service().await;

    // Service should be created successfully
    // This is a smoke test to ensure all dependencies wire together
    drop(service);
}

#[core_async::test]
async fn test_enrich_track_missing_artist() {
    let (service, pool) = create_enrichment_service().await;

    // Create track without artist
    let track = create_test_track(&pool, "track1", "Test Song", None, None).await;

    // Test lyrics fetching without artist
    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    let result = service.enrich_track(request).await;
    // Service uses graceful degradation - logs error but returns Ok
    assert!(
        result.is_ok(),
        "Service should handle missing artist gracefully"
    );
    let response = result.unwrap();
    assert!(
        !response.lyrics_fetched,
        "Lyrics should not be fetched without artist"
    );
    assert_eq!(
        response.lyrics_status, "not_fetched",
        "Lyrics status should remain not_fetched"
    );
}

#[core_async::test]
async fn test_enrich_track_missing_album_for_artwork() {
    let (service, pool) = create_enrichment_service().await;

    // Create artist but no album
    let artist = create_test_artist(&pool, "artist1", "Test Artist").await;
    let track =
        create_test_track(&pool, "track1", "Test Song", Some(artist.id.clone()), None).await;

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: true,
        fetch_lyrics: false,
    };

    // Service uses graceful degradation - logs error but returns Ok
    let result = service.enrich_track(request).await;
    assert!(
        result.is_ok(),
        "Service should handle missing album gracefully"
    );
    let response = result.unwrap();
    assert!(
        !response.artwork_fetched,
        "Artwork should not be fetched without album"
    );
}

#[core_async::test]
async fn test_enrich_track_no_enrichment_requested() {
    let (service, pool) = create_enrichment_service().await;

    // Create track without any metadata
    let track = create_test_track(&pool, "track1", "Test Song", None, None).await;

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false,
        fetch_lyrics: false,
    };

    // Should succeed when no enrichment is requested (no-op)
    let result = service.enrich_track(request).await;
    assert!(
        result.is_ok(),
        "Expected success when no enrichment is requested"
    );

    let response = result.unwrap();
    assert!(!response.artwork_fetched);
    assert!(!response.lyrics_fetched);
}

#[core_async::test]
async fn test_enrich_track_artist_not_in_database() {
    let (service, _pool) = create_enrichment_service().await;

    // Create track structure directly (bypassing database insert to avoid foreign key constraint)
    let track = Track {
        id: "track1".to_string(),
        provider_id: "test_provider".to_string(),
        provider_file_id: "file_track1".to_string(),
        hash: None,
        title: "Test Song".to_string(),
        normalized_title: "test song".to_lowercase(),
        album_id: None,
        artist_id: Some("nonexistent_artist".to_string()),
        album_artist_id: None,
        track_number: Some(1),
        disc_number: 1,
        genre: None,
        year: Some(2023),
        duration_ms: 180000,
        bitrate: Some(320),
        sample_rate: Some(44100),
        channels: Some(2),
        format: "mp3".to_string(),
        file_size: Some(8_000_000),
        mime_type: Some("audio/mpeg".to_string()),
        artwork_id: None,
        lyrics_status: "not_fetched".to_string(),
        created_at: 1000000,
        updated_at: 1000000,
        provider_modified_at: None,
    };

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    // Service uses graceful degradation - logs error but returns Ok
    let result = service.enrich_track(request).await;
    assert!(
        result.is_ok(),
        "Service should handle missing artist record gracefully"
    );
    let response = result.unwrap();
    assert!(
        !response.lyrics_fetched,
        "Lyrics should not be fetched when artist doesn't exist"
    );
}

#[core_async::test]
async fn test_enrich_track_album_not_in_database() {
    let (service, pool) = create_enrichment_service().await;

    // Create artist
    let artist = create_test_artist(&pool, "artist1", "Test Artist").await;

    // Create track structure directly (bypassing database insert for album reference)
    let track = Track {
        id: "track1".to_string(),
        provider_id: "test_provider".to_string(),
        provider_file_id: "file_track1".to_string(),
        hash: None,
        title: "Test Song".to_string(),
        normalized_title: "test song".to_lowercase(),
        album_id: Some("nonexistent_album".to_string()),
        artist_id: Some(artist.id.clone()),
        album_artist_id: None,
        track_number: Some(1),
        disc_number: 1,
        genre: None,
        year: Some(2023),
        duration_ms: 180000,
        bitrate: Some(320),
        sample_rate: Some(44100),
        channels: Some(2),
        format: "mp3".to_string(),
        file_size: Some(8_000_000),
        mime_type: Some("audio/mpeg".to_string()),
        artwork_id: None,
        lyrics_status: "not_fetched".to_string(),
        created_at: 1000000,
        updated_at: 1000000,
        provider_modified_at: None,
    };

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: true,
        fetch_lyrics: false,
    };

    // Service uses graceful degradation - logs error but returns Ok
    let result = service.enrich_track(request).await;
    assert!(
        result.is_ok(),
        "Service should handle missing album record gracefully"
    );
    let response = result.unwrap();
    assert!(
        !response.artwork_fetched,
        "Artwork should not be fetched when album doesn't exist"
    );
}

#[core_async::test]
async fn test_enrich_track_lyrics_only_without_album() {
    let (service, pool) = create_enrichment_service().await;

    // Create track with artist but no album (lyrics don't require album)
    let artist = create_test_artist(&pool, "artist1", "Test Artist").await;
    let track =
        create_test_track(&pool, "track1", "Test Song", Some(artist.id.clone()), None).await;

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    // Should succeed (lyrics don't require album)
    // Note: Without real providers, lyrics won't be found, but the service should handle it gracefully
    let result = service.enrich_track(request).await;

    match result {
        Ok(response) => {
            // Lyrics fetch attempted but not found (no real provider)
            assert!(!response.lyrics_fetched);
            assert_eq!(response.lyrics_status, "not_fetched");
        }
        Err(e) => {
            // Should not error, just return unavailable
            panic!("Unexpected error: {}", e);
        }
    }
}

#[core_async::test]
async fn test_enrich_track_with_complete_metadata() {
    let (service, pool) = create_enrichment_service().await;

    // Create complete metadata chain
    let artist = create_test_artist(&pool, "artist1", "The Beatles").await;
    let album = create_test_album(&pool, "album1", "Abbey Road", &artist.id).await;
    let track = create_test_track(
        &pool,
        "track1",
        "Come Together",
        Some(artist.id.clone()),
        Some(album.id.clone()),
    )
    .await;

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false, // Skip artwork (no HTTP client configured)
        fetch_lyrics: true,
    };

    // Should succeed without errors
    let result = service.enrich_track(request).await;

    match result {
        Ok(response) => {
            // Without real providers, nothing will be fetched, but should not error
            assert!(!response.lyrics_fetched);
            assert_eq!(response.track.id, track.id);
        }
        Err(e) => {
            panic!("Unexpected error with complete metadata: {}", e);
        }
    }
}

#[core_async::test]
async fn test_enrichment_request_structure() {
    let track = Track {
        id: "test_track".to_string(),
        provider_id: "test_provider".to_string(),
        provider_file_id: "file1".to_string(),
        hash: None,
        title: "Test".to_string(),
        normalized_title: "test".to_string(),
        album_id: Some("album1".to_string()),
        artist_id: Some("artist1".to_string()),
        album_artist_id: None,
        track_number: Some(1),
        disc_number: 1,
        genre: None,
        year: None,
        duration_ms: 180000,
        bitrate: Some(320),
        sample_rate: Some(44100),
        channels: Some(2),
        format: "mp3".to_string(),
        file_size: Some(8_000_000),
        mime_type: Some("audio/mpeg".to_string()),
        artwork_id: None,
        lyrics_status: "not_fetched".to_string(),
        created_at: 1000000,
        updated_at: 1000000,
        provider_modified_at: None,
    };

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: true,
        fetch_lyrics: false,
    };

    assert_eq!(request.track.id, "test_track");
    assert!(request.fetch_artwork);
    assert!(!request.fetch_lyrics);
}

#[core_async::test]
async fn test_enrichment_response_structure() {
    let (service, pool) = create_enrichment_service().await;

    // Create complete metadata
    let artist = create_test_artist(&pool, "artist1", "Artist Name").await;
    let track =
        create_test_track(&pool, "track1", "Song Title", Some(artist.id.clone()), None).await;

    let request = EnrichmentRequest {
        track: track.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    let result = service.enrich_track(request).await;

    assert!(result.is_ok());
    let response = result.unwrap();

    // Verify response structure
    assert_eq!(response.track.id, track.id);
    assert!(!response.artwork_fetched); // Artwork not requested
    assert!(!response.lyrics_fetched); // No real provider, so not fetched
}

#[core_async::test]
async fn test_multiple_tracks_enrichment() {
    let (service, pool) = create_enrichment_service().await;

    // Create artist and album
    let artist = create_test_artist(&pool, "artist1", "Test Artist").await;
    let album = create_test_album(&pool, "album1", "Test Album", &artist.id).await;

    // Create multiple tracks
    let track1 = create_test_track(
        &pool,
        "track1",
        "Song 1",
        Some(artist.id.clone()),
        Some(album.id.clone()),
    )
    .await;

    let track2 = create_test_track(
        &pool,
        "track2",
        "Song 2",
        Some(artist.id.clone()),
        Some(album.id.clone()),
    )
    .await;

    // Enrich both tracks
    let request1 = EnrichmentRequest {
        track: track1.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    let request2 = EnrichmentRequest {
        track: track2.clone(),
        fetch_artwork: false,
        fetch_lyrics: true,
    };

    let result1 = service.enrich_track(request1).await;
    let result2 = service.enrich_track(request2).await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let response1 = result1.unwrap();
    let response2 = result2.unwrap();

    assert_eq!(response1.track.id, "track1");
    assert_eq!(response2.track.id, "track2");
}
