//! Integration tests for metadata enrichment job

use core_library::db::create_test_pool;
use core_library::models::Track;
use core_library::repositories::artwork::SqliteArtworkRepository;
use core_library::repositories::lyrics::SqliteLyricsRepository;
use core_library::repositories::track::{SqliteTrackRepository, TrackRepository};
use core_metadata::artwork::ArtworkService;
use core_metadata::enrichment_job::{EnrichmentConfig, EnrichmentJob};
use core_metadata::lyrics::LyricsService;
use core_runtime::events::EventBus;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Helper to insert a test provider
async fn insert_test_provider(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO providers (id, type, display_name, profile_id, created_at) 
         VALUES ('test-provider', 'GoogleDrive', 'Test Provider', 'test-profile', 1699200000)",
    )
    .execute(pool)
    .await
    .ok(); // Ignore error if already exists
}

/// Create a test track
fn create_test_track(id: &str, title: &str, artwork_id: Option<String>) -> Track {
    Track {
        id: id.to_string(),
        provider_id: "test-provider".to_string(),
        provider_file_id: format!("file-{}", id),
        hash: Some(format!("hash-{}", id)),
        title: title.to_string(),
        normalized_title: title.to_lowercase(),
        album_id: None,
        artist_id: None,
        album_artist_id: None,
        track_number: Some(1),
        disc_number: 1,
        genre: Some("Test".to_string()),
        year: Some(2024),
        duration_ms: 180000,
        bitrate: Some(320),
        sample_rate: Some(44100),
        channels: Some(2),
        format: "mp3".to_string(),
        file_size: Some(5242880),
        mime_type: Some("audio/mpeg".to_string()),
        artwork_id,
        lyrics_status: "not_fetched".to_string(),
        created_at: 1699200000,
        updated_at: 1699200000,
        provider_modified_at: Some(1699200000),
    }
}

#[core_async::test]
async fn test_enrichment_config_builder() {
    let config = EnrichmentConfig::builder()
        .with_batch_size(100)
        .with_max_concurrent(10)
        .with_artwork(true)
        .with_lyrics(true)
        .with_require_wifi(false)
        .with_max_retries(5)
        .with_retry_delay_ms(200)
        .with_timeout_secs(60);

    assert_eq!(config.batch_size, 100);
    assert_eq!(config.max_concurrent, 10);
    assert!(config.enable_artwork);
    assert!(config.enable_lyrics);
    assert!(!config.require_wifi);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.base_retry_delay_ms, 200);
    assert_eq!(config.operation_timeout_secs, 60);
}

#[core_async::test]
async fn test_query_tracks_missing_artwork() {
    let pool = create_test_pool()
        .await
        .expect("Failed to create test pool");
    insert_test_provider(&pool).await;

    let repository = SqliteTrackRepository::new(pool.clone());

    // Insert tracks - all without artwork initially
    let track1 = create_test_track("track-1", "Track 1", None);
    let track2 = create_test_track("track-2", "Track 2", None);
    let track3 = create_test_track("track-3", "Track 3", None);

    repository
        .insert(&track1)
        .await
        .expect("Failed to insert track1");
    repository
        .insert(&track2)
        .await
        .expect("Failed to insert track2");
    repository
        .insert(&track3)
        .await
        .expect("Failed to insert track3");

    // Create a dummy artwork so we can set it on track2
    sqlx::query(
        "INSERT INTO artworks (id, hash, binary_blob, mime_type, width, height, file_size, created_at)
         VALUES ('artwork-1', 'hash123', X'FFD8FF', 'image/jpeg', 300, 300, 1024, 1699200000)"
    )
    .execute(&pool)
    .await
    .expect("Failed to insert dummy artwork");

    // Update track2 to have artwork
    sqlx::query("UPDATE tracks SET artwork_id = 'artwork-1' WHERE id = 'track-2'")
        .execute(&pool)
        .await
        .expect("Failed to update track2 artwork");

    // Query tracks without artwork
    let tracks = repository
        .find_by_missing_artwork()
        .await
        .expect("Failed to query tracks");

    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().any(|t| t.id == "track-1"));
    assert!(tracks.iter().any(|t| t.id == "track-3"));
    assert!(!tracks.iter().any(|t| t.id == "track-2"));
}

#[core_async::test]
async fn test_query_tracks_by_lyrics_status() {
    let pool = create_test_pool()
        .await
        .expect("Failed to create test pool");
    insert_test_provider(&pool).await;

    let repository = SqliteTrackRepository::new(pool.clone());

    // Insert tracks with different lyrics statuses
    let mut track1 = create_test_track("track-1", "Track 1", None);
    track1.lyrics_status = "not_fetched".to_string();

    let mut track2 = create_test_track("track-2", "Track 2", None);
    track2.lyrics_status = "available".to_string();

    let mut track3 = create_test_track("track-3", "Track 3", None);
    track3.lyrics_status = "not_fetched".to_string();

    repository
        .insert(&track1)
        .await
        .expect("Failed to insert track1");
    repository
        .insert(&track2)
        .await
        .expect("Failed to insert track2");
    repository
        .insert(&track3)
        .await
        .expect("Failed to insert track3");

    // Query tracks with 'not_fetched' status
    let tracks = repository
        .find_by_lyrics_status("not_fetched")
        .await
        .expect("Failed to query tracks");

    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().any(|t| t.id == "track-1"));
    assert!(tracks.iter().any(|t| t.id == "track-3"));
    assert!(!tracks.iter().any(|t| t.id == "track-2"));

    // Query tracks with 'available' status
    let tracks = repository
        .find_by_lyrics_status("available")
        .await
        .expect("Failed to query tracks");

    assert_eq!(tracks.len(), 1);
    assert!(tracks.iter().any(|t| t.id == "track-2"));
}

#[core_async::test]
async fn test_enrichment_job_initialization() {
    let pool = create_test_pool()
        .await
        .expect("Failed to create test pool");

    let artwork_repo = Arc::new(SqliteArtworkRepository::new(pool.clone()));
    let lyrics_repo = Arc::new(SqliteLyricsRepository::new(pool.clone()));
    let track_repo: Arc<dyn TrackRepository> = Arc::new(SqliteTrackRepository::new(pool.clone()));
    let artist_repo =
        Arc::new(core_library::repositories::artist::SqliteArtistRepository::new(pool.clone()));
    let album_repo =
        Arc::new(core_library::repositories::album::SqliteAlbumRepository::new(pool.clone()));
    let event_bus = Arc::new(EventBus::new(100));

    let artwork_service = Arc::new(ArtworkService::new(artwork_repo, 200 * 1024 * 1024));
    let lyrics_service = Arc::new(LyricsService::without_providers(lyrics_repo));

    // Create EnrichmentService
    let enrichment_service = Arc::new(core_metadata::enrichment_service::EnrichmentService::new(
        artist_repo,
        album_repo,
        track_repo.clone(),
        artwork_service,
        lyrics_service,
    ));

    let config = EnrichmentConfig::default();
    let _job = EnrichmentJob::new(config, enrichment_service, track_repo, event_bus);

    // Job created successfully
}

#[core_async::test]
async fn test_enrichment_progress_calculation() {
    use core_metadata::enrichment_job::EnrichmentProgress;

    let mut progress = EnrichmentProgress::new(100);
    assert_eq!(progress.total_tracks, 100);
    assert_eq!(progress.processed, 0);
    assert_eq!(progress.percent_complete, 0);

    // Process 25 tracks
    progress.processed = 25;
    progress.update();
    assert_eq!(progress.percent_complete, 25);

    // Process 50 tracks
    progress.processed = 50;
    progress.update();
    assert_eq!(progress.percent_complete, 50);

    // Complete
    progress.processed = 100;
    progress.update();
    assert_eq!(progress.percent_complete, 100);

    // Over 100% (should cap)
    progress.processed = 150;
    progress.update();
    assert_eq!(progress.percent_complete, 100);
}

#[core_async::test]
async fn test_enrichment_config_defaults() {
    let config = EnrichmentConfig::default();

    assert_eq!(config.batch_size, 50);
    assert_eq!(config.max_concurrent, 5);
    assert!(config.enable_artwork);
    assert!(config.enable_lyrics);
    assert!(!config.require_wifi);
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.base_retry_delay_ms, 100);
    assert_eq!(config.operation_timeout_secs, 30);
}
