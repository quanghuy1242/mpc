# Core Library - WebAssembly Build

This directory contains the WebAssembly bindings for the Music Platform Core library.

## Building for WASM

### Prerequisites

1. Install `wasm-pack`:
```bash
cargo install wasm-pack
```

2. (Optional) Install `wasm-opt` for additional size optimization:
```bash
# On Windows with chocolatey
choco install binaryen

# On macOS
brew install binaryen

# On Linux
sudo apt-get install binaryen
```

### Build Commands

#### Development Build
```bash
wasm-pack build --target web --dev
```

#### Production Build (Optimized for Size)
```bash
wasm-pack build --target web --release
```

#### Production Build with Maximum Optimization
```bash
wasm-pack build --target web --release -- -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort
```

Or use the provided build script:
```bash
# PowerShell
.\build-wasm.ps1

# Bash
./build-wasm.sh
```

## Output

The build will generate a `pkg/` directory containing:
- `core_library.js` - JavaScript bindings
- `core_library_bg.wasm` - WebAssembly binary
- `core_library.d.ts` - TypeScript definitions
- `package.json` - NPM package metadata

## Usage in JavaScript/TypeScript

```typescript
import init, { 
  JsAlbum, 
  JsArtist, 
  JsPlaylist,
  JsTrackId,
  version 
} from './pkg/core_library.js';

// Initialize the WASM module
await init();

console.log(`Core Library v${version()}`);

// Create a new artist
const artist = new JsArtist("The Beatles");
console.log(`Artist ID: ${artist.id()}`);
console.log(`Artist Name: ${artist.name()}`);

// Validate
artist.validate();

// Convert to JavaScript object
const artistObj = artist.toObject();
console.log(artistObj);

// Create an album
const album = new JsAlbum("Abbey Road", artist.id());
console.log(`Album: ${album.name()}`);

// Create a playlist
const playlist = new JsPlaylist("My Favorites");
console.log(`Playlist: ${playlist.name()}`);
```

## Size Optimization

The build is optimized for small size:
- LTO (Link Time Optimization) enabled
- Opt-level "z" (optimize for size)
- Debug symbols stripped
- Single codegen unit
- Panic = abort (smaller panic handler)
- wasm-opt post-processing (if available)

Expected final size: ~50-150 KB (gzipped)

## API Reference

See the generated TypeScript definitions in `pkg/core_library.d.ts` for the complete API.

### Available Types

- `JsTrackId` - Track identifier
- `JsAlbumId` - Album identifier  
- `JsArtistId` - Artist identifier
- `JsPlaylistId` - Playlist identifier
- `JsTrack` - Track model
- `JsAlbum` - Album model
- `JsArtist` - Artist model
- `JsPlaylist` - Playlist model

### Utility Functions

- `version()` - Get library version
- `name()` - Get library name
