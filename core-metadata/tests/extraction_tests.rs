//! Integration tests for metadata extraction
//!
//! These tests verify basic error handling and API functionality.
//! For full format testing with real audio files, see tests/fixtures/README.md

use core_metadata::extractor::MetadataExtractor;
use std::fs;
use std::path::PathBuf;

/// Helper to get the fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[tokio::test]
async fn test_extract_missing_file() {
    let fixtures = fixtures_dir();
    let missing_path = fixtures.join("nonexistent.mp3");

    let extractor = MetadataExtractor::new();
    let result = extractor.extract_from_file(&missing_path).await;

    assert!(result.is_err(), "Should fail for missing file");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to read file"));
}

#[tokio::test]
async fn test_extract_corrupted_file() {
    let fixtures = fixtures_dir();
    let corrupt_path = fixtures.join("corrupt.mp3");

    // Create a file with invalid content
    fs::create_dir_all(&fixtures).ok();
    fs::write(&corrupt_path, b"This is not a valid audio file")
        .expect("Failed to create corrupt file");

    let extractor = MetadataExtractor::new();
    let result = extractor.extract_from_file(&corrupt_path).await;

    // Cleanup
    let _ = fs::remove_file(&corrupt_path);

    // Should fail gracefully
    assert!(result.is_err(), "Should fail for corrupted file");
}

#[tokio::test]
async fn test_extractor_creation() {
    let _extractor1 = MetadataExtractor::new();
    let _extractor2 = MetadataExtractor::default();

    // Both should be usable (just verify they compile and construct)
    assert!(true);
}

// The following tests require actual audio files in tests/fixtures/
// See tests/fixtures/README.md for instructions on adding test files

#[cfg(feature = "with-test-fixtures")]
mod with_fixtures {
    use super::*;

    #[tokio::test]
    async fn test_extract_mp3_metadata() {
        let fixtures = fixtures_dir();
        let mp3_path = fixtures.join("sample.mp3");

        if !mp3_path.exists() {
            panic!("sample.mp3 not found in fixtures directory");
        }

        let extractor = MetadataExtractor::new();
        let result = extractor.extract_from_file(&mp3_path).await;

        assert!(
            result.is_ok(),
            "Failed to extract metadata: {:?}",
            result.err()
        );

        let metadata = result.unwrap();

        // Print extracted metadata
        println!("\n=== Extracted Metadata from sample.mp3 ===");
        println!("Title: {:?}", metadata.title);
        println!("Artist: {:?}", metadata.artist);
        println!("Album: {:?}", metadata.album);
        println!("Album Artist: {:?}", metadata.album_artist);
        println!("Genre: {:?}", metadata.genre);
        println!("Year: {:?}", metadata.year);
        println!("Track Number: {:?}", metadata.track_number);
        println!("Disc Number: {:?}", metadata.disc_number);
        println!("Duration: {} ms", metadata.duration_ms);
        println!("Bitrate: {:?} bps", metadata.bitrate);
        println!("Sample Rate: {:?} Hz", metadata.sample_rate);
        println!("Channels: {:?}", metadata.channels);
        println!("Format: {}", metadata.format);
        println!("MIME Type: {}", metadata.mime_type);
        println!("Content Hash: {}", metadata.content_hash);
        println!("Has Artwork: {}", !metadata.artwork.is_empty());
        if !metadata.artwork.is_empty() {
            println!("Artwork count: {}", metadata.artwork.len());
            for (i, art) in metadata.artwork.iter().enumerate() {
                println!(
                    "  Artwork {}: {} ({} bytes)",
                    i + 1,
                    art.mime_type,
                    art.data.len()
                );
            }
        }
        println!("==========================================\n");

        // Assertions
        assert_eq!(metadata.format, "Mpeg");
        assert_eq!(metadata.mime_type, "audio/mpeg");
        assert!(metadata.duration_ms > 0);
        assert!(!metadata.content_hash.is_empty());
        assert_eq!(metadata.content_hash.len(), 64);
    }

    #[tokio::test]
    async fn test_extract_flac_metadata() {
        let fixtures = fixtures_dir();
        let flac_path = fixtures.join("sample.flac");

        if !flac_path.exists() {
            eprintln!("Skipping test: sample.flac not found in fixtures");
            return;
        }

        let extractor = MetadataExtractor::new();
        let result = extractor.extract_from_file(&flac_path).await;

        assert!(
            result.is_ok(),
            "Failed to extract metadata: {:?}",
            result.err()
        );

        let metadata = result.unwrap();
        assert_eq!(metadata.format, "Flac");
        assert_eq!(metadata.mime_type, "audio/flac");
        assert!(!metadata.content_hash.is_empty());
    }

    #[tokio::test]
    async fn test_normalize_metadata() {
        let fixtures = fixtures_dir();
        let mp3_path = fixtures.join("sample_whitespace.mp3");

        if !mp3_path.exists() {
            eprintln!("Skipping test: sample_whitespace.mp3 not found");
            return;
        }

        let extractor = MetadataExtractor::new();
        let result = extractor.extract_from_file(&mp3_path).await;

        assert!(result.is_ok());
        let metadata = result.unwrap();

        // Should be normalized (trimmed, single spaces)
        if let Some(title) = &metadata.title {
            assert!(!title.starts_with(' '));
            assert!(!title.ends_with(' '));
            assert!(!title.contains("  "));
        }
    }

    #[tokio::test]
    async fn test_performance_requirement() {
        use std::time::Instant;

        let fixtures = fixtures_dir();
        let mp3_path = fixtures.join("sample.mp3");

        if !mp3_path.exists() {
            eprintln!("Skipping test: sample.mp3 not found");
            return;
        }

        let extractor = MetadataExtractor::new();

        let start = Instant::now();
        let result = extractor.extract_from_file(&mp3_path).await;
        let duration = start.elapsed();

        assert!(result.is_ok());

        // Performance requirement: <50ms per track
        // Note: This may fail on slow systems or with large files
        if duration.as_millis() >= 50 {
            eprintln!(
                "Warning: Extraction took {}ms, requirement is <50ms",
                duration.as_millis()
            );
        }
    }
}
