# Core Playback

Production-grade audio playback and decoding module for the Music Platform Core.

## Features

- **Multi-Format Decoding**: MP3, AAC, FLAC, Vorbis, Opus, WAV, ALAC
- **Symphonia Integration**: High-performance audio codec library
- **Platform Abstractions**: Cross-platform playback adapters
- **Ring Buffer**: Lock-free producer-consumer audio streaming
- **Offline Cache**: Optional encrypted cache for offline playback
- **WebAssembly**: Full WASM support for browser environments

## Architecture

```
┌─────────────────────┐
│  StreamingService   │  Producer (Background Thread)
│   AudioDecoder      │
└──────────┬──────────┘
           │ PCM Samples
           ▼
┌─────────────────────┐
│    Ring Buffer      │  Shared, Thread-Safe
└──────────┬──────────┘
           │ PCM Samples
           ▼
┌─────────────────────┐
│  PlaybackAdapter    │  Consumer (Audio Thread)
│  Platform Audio     │
└─────────────────────┘
```

## Installation

### Rust/Native

Add to `Cargo.toml`:

```toml
[dependencies]
core-playback = { path = "../core-playback" }
```

### WebAssembly

Build to WASM:

```bash
wasm-pack build --target web --release \
  --no-default-features \
  --features decoder-mp3,decoder-aac,core-decoder
```

See [WASM_USAGE.md](./WASM_USAGE.md) for JavaScript/TypeScript usage.

## Usage

### Basic Audio Decoding

```rust
use core_playback::{AudioDecoder, AudioSource, SymphoniaDecoder};

async fn decode_audio() -> Result<()> {
    // Load audio file
    let source = AudioSource::from_path("song.mp3").await?;
    let mut decoder = SymphoniaDecoder::new(source).await?;
    
    // Probe format
    let probe = decoder.probe().await?;
    println!("Sample rate: {}", probe.format.sample_rate);
    println!("Duration: {:?}", probe.duration);
    
    // Decode frames
    while let Some(chunk) = decoder.decode_frames(4096).await? {
        // Process PCM samples
        process_audio(&chunk.samples);
    }
    
    Ok(())
}
```

### Streaming Service

```rust
use core_playback::{StreamingService, StreamingRequest};

async fn start_streaming(track_id: String) {
    let config = StreamingConfig::default();
    let service = StreamingService::new(config, adapter, cache);
    
    let request = StreamingRequest {
        track_id,
        start_position: None,
        preload_next: true,
    };
    
    service.start_streaming(request).await?;
}
```

## Feature Flags

### Core Features

- `core-decoder` - Enable Symphonia-based decoder
- `offline-cache` - Enable encrypted offline cache

### Codec Features

- `decoder-mp3` - MP3 support via Symphonia
- `decoder-aac` - AAC/M4A support
- `decoder-flac` - FLAC support
- `decoder-vorbis` - Ogg Vorbis support
- `decoder-opus` - Opus support
- `decoder-wav` - WAV support (built-in)
- `decoder-alac` - ALAC support (built-in)
- `decoder-all` - All codecs (default)

### Platform Features

- `http-streaming` - HTTP streaming support (native only)
- `wasm` - WebAssembly support

## WebAssembly Support

Full WASM support with TypeScript definitions:

```typescript
import init, { JsAudioDecoder } from './pkg/core_playback';

await init();

const decoder = await JsAudioDecoder.create(audioData, 'song.mp3');
const samples = await decoder.decodeBatch(32768, 50);
```

See documentation:
- [WASM_USAGE.md](./WASM_USAGE.md) - Complete usage guide
- [WASM_BUILD_SUMMARY.md](./WASM_BUILD_SUMMARY.md) - Build details
- [demo-playback.html](./demo-playback.html) - Interactive demo

## Performance

- **MP3 Decoding**: ~40-50x realtime
- **AAC Decoding**: ~35-45x realtime
- **Ring Buffer**: Lock-free, zero-copy streaming
- **Batch Processing**: Minimizes WASM/JS boundary crossings

## Platform Support

- ✅ **Native (Desktop)**: Full multi-threaded operation
- ✅ **WebAssembly**: Single-threaded with Web Workers
- ✅ **Mobile**: Via Tauri with native audio

## Dependencies

- **symphonia**: Audio codec library
- **bytes**: Zero-copy buffer management
- **parking_lot**: High-performance synchronization
- **thiserror**: Error handling

## License

Part of Music Platform Core. See workspace LICENSE file.

## Documentation

- [API Documentation](./docs/) - Detailed API reference
- [Usage Examples](./examples/) - Code examples
- [WASM Guide](./WASM_USAGE.md) - WebAssembly usage
- [Architecture](../docs/core_architecture.md) - System design
