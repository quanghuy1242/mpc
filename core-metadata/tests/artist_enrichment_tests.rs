//! Integration tests for artist enrichment functionality
//!
//! These tests verify:
//! - EnrichmentService error handling without artist enrichment provider
//! - Integration with existing enrichment service tests

use core_library::db::create_test_pool;
use core_library::models::Artist;
use core_library::repositories::album::SqliteAlbumRepository;
use core_library::repositories::artist::{ArtistRepository, SqliteArtistRepository};
use core_library::repositories::artwork::SqliteArtworkRepository;
use core_library::repositories::lyrics::SqliteLyricsRepository;
use core_library::repositories::track::SqliteTrackRepository;
use core_metadata::enrichment_service::EnrichmentService;
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

/// Create enrichment service WITHOUT artist enrichment provider
async fn create_enrichment_service_without_artist() -> (EnrichmentService, sqlx::SqlitePool) {
    let pool = setup_test_db().await;

    let artist_repo = Arc::new(SqliteArtistRepository::new(pool.clone()));
    let album_repo = Arc::new(SqliteAlbumRepository::new(pool.clone()));
    let track_repo = Arc::new(SqliteTrackRepository::new(pool.clone()));
    let artwork_repo = Arc::new(SqliteArtworkRepository::new(pool.clone()));
    let lyrics_repo = Arc::new(SqliteLyricsRepository::new(pool.clone()));

    let artwork_service = Arc::new(ArtworkService::new(
        artwork_repo,
        10 * 1024 * 1024, // 10MB cache
    ));

    let lyrics_service = Arc::new(LyricsService::without_providers(lyrics_repo));

    let service = EnrichmentService::new(
        artist_repo,
        album_repo,
        track_repo,
        artwork_service,
        lyrics_service,
    );
    // Note: NOT calling .with_artist_enrichment()

    (service, pool)
}

#[tokio::test]
async fn test_artist_enrichment_without_provider() {
    let (service, pool) = create_enrichment_service_without_artist().await;
    let artist_repo = SqliteArtistRepository::new(pool.clone());

    // Create test artist
    let artist = Artist::new("Radiohead".to_string());
    artist_repo.insert(&artist).await.unwrap();

    // Try to enrich without provider - should return error
    let result = service.enrich_artist(&artist.id.to_string()).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Artist enrichment provider not configured"),
        "Expected 'provider not configured' error, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_batch_enrichment_without_provider() {
    let (service, pool) = create_enrichment_service_without_artist().await;
    let artist_repo = SqliteArtistRepository::new(pool.clone());

    // Create test artists
    let artist1 = Artist::new("The Beatles".to_string());
    artist_repo.insert(&artist1).await.unwrap();

    let artist2 = Artist::new("Pink Floyd".to_string());
    artist_repo.insert(&artist2).await.unwrap();

    let artist_ids = vec![artist1.id.to_string(), artist2.id.to_string()];

    // Without provider, all enrichments should fail
    let (succeeded, failed) = service.enrich_artists_batch(&artist_ids).await;

    assert_eq!(succeeded, 0);
    assert_eq!(failed, 2);
}
