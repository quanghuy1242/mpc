# Test Fixtures for Metadata Extraction

This directory is for audio test files used in integration tests.

## Adding Test Files

To enable full integration tests with real audio files:

1. Add audio files to this directory:
   - `sample.mp3` - A short MP3 file with ID3v2 tags
   - `sample.flac` - A short FLAC file with Vorbis comments
   - `sample_whitespace.mp3` - An MP3 with metadata containing extra whitespace
   
2. Run tests with the feature flag:
   ```bash
   cargo test --package core-metadata --features with-test-fixtures
   ```

## File Requirements

- **Size**: Keep files small (<100KB each) to avoid repository bloat
- **Copyright**: Use only files you have rights to (e.g., Creative Commons, public domain, or self-created)
- **Metadata**: Files should have proper tags set:
  - Title
  - Artist
  - Album
  - Year (optional)
  - Track number (optional)
  - Embedded artwork (optional but recommended for testing)

## Creating Test Files

You can create test files using tools like:

- **FFmpeg**: Generate silent audio with tags
  ```bash
  ffmpeg -f lavfi -i anullsrc=r=44100:cl=mono -t 1 -c:a libmp3lame -b:a 128k sample.mp3
  ```

- **Audacity**: Record/generate silence, add tags, export

- **Online tools**: Various websites offer free short audio clips (check license)

## Why Tests are Optional

The integration tests with real audio files are feature-gated because:

1. Audio files are binary and increase repository size
2. Licensing concerns with distributing music files
3. Core functionality is tested with unit tests
4. Optional fixtures allow developers to test locally without committing files

## What Gets Tested Without Fixtures

Without real audio files, the tests verify:

- Error handling (missing files, corrupted data)
- API surface (constructor, default impl)
- Unit-level functionality (normalization, hash calculation, type conversions)

With fixtures, additional tests cover:

- Actual format parsing (MP3, FLAC, M4A, etc.)
- Metadata extraction from tags
- Artwork extraction
- Performance benchmarks
- Normalization on real data
