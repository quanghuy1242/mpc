# Core Runtime - WASM Build

This directory contains the WebAssembly build of the `core-runtime` module.

## Features

- **Logging System**: Browser console integration with tracing-wasm
- **Event Bus**: Publish-subscribe event system for decoupled communication
- **Configuration**: Feature flags and metadata API configuration
- **TypeScript Support**: Full type definitions generated automatically

## Building

```powershell
# Development build
.\build-wasm.ps1

# Release build (optimized)
.\build-wasm.ps1 -Release
```

## What's Included

### Logging
- `JsLoggingConfig`: Configure logging format, level, and filters
- `initLogging()`: Initialize the logging system
- Browser console integration via tracing-wasm

### Events
- `JsEventBus`: Central event bus for pub/sub messaging
- `JsEventReceiver`: Async event receiver
- Event constructors for all event types:
  - Auth events (SignedIn, SignedOut, TokenRefreshed, etc.)
  - Sync events (Started, Progress, Completed, Failed)
  - Library events (TrackAdded, PlaylistCreated, etc.)
  - Playback events (Started, Paused, Completed, etc.)

### Configuration
- `JsFeatureFlags`: Enable/disable optional features
- `JsMetadataApiConfig`: Configure MusicBrainz and Last.fm APIs

### Utilities
- `version()`: Get package version
- `name()`: Get package name
- Event type and severity helpers

## Usage Example

```typescript
import init, {
  JsLoggingConfig,
  initLogging,
  JsEventBus,
  createAuthSignedInEvent,
} from './core_runtime.js';

// Initialize WASM
await init();

// Setup logging
const logConfig = new JsLoggingConfig();
logConfig.setLevel(2); // Info level
initLogging(logConfig);

// Create event bus
const eventBus = new JsEventBus(100);

// Subscribe to events
const receiver = eventBus.subscribe();

// Listen for events
(async () => {
  while (true) {
    try {
      const eventJson = await receiver.recv();
      const event = JSON.parse(eventJson);
      console.log('Event received:', event);
    } catch (e) {
      console.error('Event error:', e);
      break;
    }
  }
})();

// Emit events
const event = createAuthSignedInEvent('profile-123', 'GoogleDrive');
eventBus.emit(event);
```

## Package Structure

```
pkg/
├── core_runtime_bg.wasm      # Compiled WebAssembly binary
├── core_runtime_bg.wasm.d.ts # TypeScript definitions for WASM
├── core_runtime.js            # JavaScript glue code
├── core_runtime.d.ts          # TypeScript definitions
├── package.json               # NPM package metadata
└── README.md                  # This file
```

## Integration

### Browser

```html
<script type="module">
  import init from './core_runtime.js';
  await init();
  // Use the API...
</script>
```

### Node.js

```javascript
import init from './core_runtime.js';
import { readFile } from 'fs/promises';

const wasm = await readFile('./core_runtime_bg.wasm');
await init(wasm);
// Use the API...
```

## Event Types

All events are represented as JSON and follow this structure:

### Auth Events
```typescript
{
  "type": "Auth",
  "payload": {
    "event": "SignedIn",
    "profile_id": "profile-123",
    "provider": "GoogleDrive"
  }
}
```

### Sync Events
```typescript
{
  "type": "Sync",
  "payload": {
    "event": "Progress",
    "job_id": "job-456",
    "items_processed": 50,
    "total_items": 100,
    "percent": 50,
    "phase": "Processing"
  }
}
```

### Library Events
```typescript
{
  "type": "Library",
  "payload": {
    "event": "TrackAdded",
    "track_id": "track-789",
    "title": "My Song",
    "artist": "Artist Name",
    "album": "Album Name"
  }
}
```

### Playback Events
```typescript
{
  "type": "Playback",
  "payload": {
    "event": "Started",
    "track_id": "track-789",
    "title": "My Song"
  }
}
```

## Performance Considerations

- **Event Buffer**: Set appropriate buffer size when creating `JsEventBus`
- **Slow Subscribers**: Receivers that fall behind will get lagged errors
- **Memory**: Events are cloned for each subscriber
- **Async**: All event operations are async

## Browser Compatibility

- Modern browsers with WebAssembly support
- ES modules required
- Async/await support required

## Size

- **WASM binary**: ~150-200 KB (release build)
- **JS glue code**: ~10-15 KB
- **Total**: ~160-215 KB (gzip: ~50-70 KB)

## License

See LICENSE file in the root repository.
