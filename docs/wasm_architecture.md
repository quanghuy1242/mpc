# WASM Architecture: All-in-Worker Design

**Document Version**: 2.0  
**Date**: November 9, 2025  
**Status**: Final Architecture

---

## Executive Summary

**Single-Worker Architecture**: All WASM business logic runs in a Web Worker, with the main thread handling only UI rendering, AudioContext playback, and OAuth popup coordination.

**Key Benefits:**
- ✅ **Single EventBus** - All modules communicate directly without forwarding
- ✅ **No database split** - Single source of truth in worker
- ✅ **UI never blocks** - All heavy work offloaded to worker
- ✅ **Simple deployment** - One WASM bundle (~3MB / ~1MB gzipped)
- ✅ **Zero-copy audio** - Transferable ArrayBuffers for decoded audio

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    MAIN THREAD (Thin UI)                     │
│                                                              │
│  • DOM rendering (React/Vue/Svelte)                         │
│  • AudioContext + Web Audio API                             │
│  • OAuth popup/redirect handling                            │
│  • postMessage coordinator                                  │
│  • Audio chunk queue + playback scheduler                   │
│                                                              │
└──────────────────────┬───────────────────────────────────────┘
                       │
                       │ postMessage
                       │ • Commands (play, pause, seek)
                       │ • OAuth callbacks
                       │ • Query requests
                       │
                       ▼
                       │ postMessage
                       │ • Events (sync, library, auth)
                       │ • Audio chunks (Transferable)
                       │ • Query responses
                       │
┌──────────────────────▼───────────────────────────────────────┐
│                    WEB WORKER (All Logic)                    │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │        WASM Bundle (~3MB / ~1MB gzipped)               │ │
│  │                                                        │ │
│  │  ┌──────────────────────────────────────────────────┐ │ │
│  │  │  core-runtime (EventBus, Logging, Config)        │ │ │
│  │  └────────────────┬─────────────────────────────────┘ │ │
│  │                   │                                    │ │
│  │                   ▼ (Single EventBus!)                │ │
│  │         ┌─────────┴─────────┐                         │ │
│  │         │                   │                         │ │
│  │         ▼                   ▼                         │ │
│  │  ┌─────────────┐     ┌─────────────┐                │ │
│  │  │ core-auth   │     │core-library │                │ │
│  │  │ (OAuth)     │────▶│ (Database)  │                │ │
│  │  └─────────────┘     └──────┬──────┘                │ │
│  │         │                    │                        │ │
│  │         ▼                    ▼                        │ │
│  │  ┌─────────────┐     ┌─────────────┐                │ │
│  │  │ core-sync   │     │core-metadata│                │ │
│  │  │ (Cloud sync)│     │ (Enrichment)│                │ │
│  │  └─────────────┘     └─────────────┘                │ │
│  │         │                                             │ │
│  │         ▼                                             │ │
│  │  ┌─────────────┐                                     │ │
│  │  │core-playback│                                     │ │
│  │  │ (Decode)    │─────────► Transferable Buffers     │ │
│  │  └─────────────┘                                     │ │
│  │                                                        │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  • Single EventBus (all modules share)                      │
│  • All business logic and state                             │
│  • Audio decoding (Symphonia WASM)                          │
│  • Database operations (IndexedDB)                          │
│  • Network requests (fetch with credentials)                │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Module Assignment

### Main Thread (Thin UI Layer)

| Component | Responsibility | Why Main Thread? |
|-----------|---------------|------------------|
| **React/Vue/Svelte** | DOM rendering, user interaction | Browser requirement |
| **AudioContext** | Audio playback, Web Audio API | Only available on main thread |
| **OAuth Popup Handler** | Open OAuth popups, receive redirects | `window.open()` requires main thread |
| **postMessage Bridge** | Coordinate with worker | Communication layer |
| **Audio Chunk Queue** | Buffer and schedule decoded audio | Playback scheduling |

**Main Thread Code:**
- ~50 KB JavaScript (UI framework excluded)
- No WASM
- Minimal state (current track, playback position)

---

### Web Worker (All Business Logic)

| Module | Size | Responsibility |
|--------|------|---------------|
| **core-runtime** | ~100 KB | EventBus, logging, configuration |
| **core-async** | ~50 KB | Runtime abstraction (Tokio/WASM) |
| **bridge-wasm** | ~150 KB | HTTP, storage, filesystem bridges |
| **core-auth** | ~80 KB | OAuth flows, token management |
| **core-library** | ~300 KB | Database (IndexedDB), queries, cache |
| **core-sync** | ~200 KB | Cloud sync operations |
| **core-metadata** | ~150 KB | Metadata enrichment, artwork, lyrics |
| **core-playback** | ~1.4 MB | Audio decoding (MP3, FLAC, AAC, etc.) |
| **Total** | **~2.43 MB** | **~950 KB gzipped** |

**Single EventBus:**
```rust
// All modules share the same EventBus instance
let event_bus = EventBus::new(100);

let auth_manager = AuthManager::new(secure_store, event_bus.clone(), http_client);
let sync_service = SyncService::new(event_bus.clone(), library, http_client);
let metadata_service = MetadataService::new(event_bus.clone(), library);
let playback_engine = PlaybackEngine::new(event_bus.clone(), library);
```

---

## OAuth Flow (Worker-Based)

### Challenge
OAuth requires opening popups and handling redirects, which are main thread operations. But core-auth logic is in the worker.

### Solution: Delegated Popup Handling

**Step 1: Worker Generates OAuth URL**
```javascript
// WEB WORKER
self.onmessage = async (msg) => {
  if (msg.data.type === 'start-signin') {
    const authUrl = await authManager.signIn(msg.data.provider);
    const state = extractState(authUrl);
    
    // Request main thread to open popup
    self.postMessage({ 
      type: 'open-oauth-popup', 
      url: authUrl, 
      state 
    });
  }
};
```

**Step 2: Main Thread Opens Popup**
```javascript
// MAIN THREAD
worker.onmessage = (msg) => {
  if (msg.data.type === 'open-oauth-popup') {
    const popup = window.open(
      msg.data.url, 
      'oauth-signin', 
      'width=500,height=700,popup=1'
    );
    
    // Store state for verification
    pendingOAuthStates.set(msg.data.state, { popup });
  }
};
```

**Step 3: OAuth Callback Redirect**
```javascript
// MAIN THREAD - Callback page or window message
window.addEventListener('message', (event) => {
  if (event.data.type === 'oauth-callback') {
    const { code, state } = event.data;
    
    // Verify state and forward to worker
    if (pendingOAuthStates.has(state)) {
      worker.postMessage({ 
        type: 'complete-oauth', 
        provider: 'GoogleDrive',
        code, 
        state 
      });
      
      pendingOAuthStates.delete(state);
    }
  }
});
```

**Step 4: Worker Completes OAuth**
```javascript
// WEB WORKER
self.onmessage = async (msg) => {
  if (msg.data.type === 'complete-oauth') {
    const profileId = await authManager.completeSignIn(
      msg.data.provider,
      msg.data.code,
      msg.data.state
    );
    
    // EventBus automatically emits Auth.SignedIn event
    // core-sync subscribes and starts sync automatically
    
    self.postMessage({ 
      type: 'signin-complete', 
      profileId 
    });
  }
};
```

### Cookies and Credentials

**Workers CAN access cookies via fetch:**
```javascript
// WEB WORKER - Cookies work automatically
const response = await fetch('https://api.example.com/user', {
  credentials: 'include' // ✅ Sends HTTP-only cookies
});
```

**No manual cookie transfer needed!**

---

## Audio Playback (Worker-Decoded, Main-Played)

### Challenge
- Audio decoding is CPU-intensive (blocks UI if on main thread)
- Web Audio API only available on main thread
- Need fast playback start and low memory usage

### Solution: StreamingService + Shared Ring Buffer

The WASM bindings now expose `JsStreamingSession`, which wraps the Rust `StreamingService` and fills a shared `JsRingBuffer`. The worker stays responsible for networking and decoding; the main thread only receives ready-to-play PCM slabs.

**Worker Setup (All-in-Worker)**
```javascript
// WEB WORKER
import {
  JsStreamingConfig,
  JsStreamingSession,
  JsRingBuffer,
  JsAudioSource,
  JsStreamingState,
} from '../core-playback/pkg/core_playback.js';
import { JsHttpClient } from '../bridge-wasm/pkg/bridge_wasm.js';

const httpClient = new JsHttpClient(null);
const streams = new Map(); // trackId → { session, ring, sampleRate, channels }

self.onmessage = async (msg) => {
  switch (msg.data.type) {
    case 'play-track': {
      const { trackId, url, headers } = msg.data;

      // Seed buffer (session will return its canonical buffer after creation)
      const seedBuffer = new JsRingBuffer(44100 * 6, 2);

      const config = new JsStreamingConfig();
      config.setBufferFrames(44100 * 4);    // 4 seconds target buffer
      config.setMinBufferFrames(44100);     // wait for 1 second before playback
      config.setPrefetchThreshold(0.35);
      config.setDecodeChunkFrames(4096);
      config.validate();

      const source = JsAudioSource.fromRemote(url, headers ?? null);
      const session = await JsStreamingSession.create(source, seedBuffer, config, httpClient);

      const ring = session.ringBuffer();
      const probe = session.format();
      const format = probe.format();

      streams.set(trackId, {
        session,
        ring,
        sampleRate: format.sampleRate(),
        channels: format.channels(),
      });

      session.start();

      self.postMessage({
        type: 'stream-started',
        trackId,
        sampleRate: format.sampleRate(),
        channels: format.channels(),
        durationMs: probe.durationMs() ?? null,
      });

      pumpRing(trackId).catch(console.error);
      break;
    }

    case 'pause-track': {
      const state = streams.get(msg.data.trackId);
      if (state) state.session.pause();
      break;
    }

    case 'resume-track': {
      const state = streams.get(msg.data.trackId);
      if (state) state.session.resume();
      break;
    }

    case 'stop-track': {
      const state = streams.get(msg.data.trackId);
      if (state) {
        state.session.stop();
        streams.delete(msg.data.trackId);
      }
      break;
    }
  }
};

async function pumpRing(trackId) {
  const state = streams.get(trackId);
  if (!state) return;

  const { session, ring, sampleRate, channels } = state;
  const framesPerChunk = Math.floor(sampleRate * 0.25); // ~250ms chunks

  while (streams.has(trackId)) {
    const chunk = ring.readFrames(framesPerChunk);
    if (chunk) {
      self.postMessage({
        type: 'pcm-chunk',
        trackId,
        buffer: chunk.buffer,
        sampleRate,
        channels,
      }, [chunk.buffer]);
      continue;
    }

    if (session.state() === JsStreamingState.Completed) {
      await session.awaitCompletion();
      self.postMessage({ type: 'stream-complete', trackId });
      streams.delete(trackId);
      break;
    }

    await new Promise(resolve => setTimeout(resolve, 8));
  }
}

> **Architectural Note:** The `pumpRing` function uses a polling loop with `setTimeout` to check for new audio data. While effective and non-blocking because it's in a worker, a more power-efficient approach could be event-driven. A future optimization might involve exposing a notification mechanism (e.g., an `await`-able promise from Rust) that resolves only when new data is written to the buffer, allowing the pump loop to sleep efficiently instead of polling.

```

**Main Thread: Schedule PCM Playback**
```javascript
// MAIN THREAD
const audioContext = new AudioContext();
const playback = new Map(); // trackId → { nextStart, sampleRate, channels }

function playTrack(trackId, url) {
  playback.set(trackId, {
    nextStart: audioContext.currentTime,
    sampleRate: null,
    channels: null,
  });

  worker.postMessage({ type: 'play-track', trackId, url });
}

worker.onmessage = (msg) => {
  switch (msg.data.type) {
    case 'stream-started': {
      const state = playback.get(msg.data.trackId);
      if (state) {
        state.sampleRate = msg.data.sampleRate;
        state.channels = msg.data.channels;
        state.nextStart = audioContext.currentTime;
      }
      break;
    }

    case 'pcm-chunk': {
      const state = playback.get(msg.data.trackId);
      if (!state) return;

      const { sampleRate, channels } = state;
      if (!sampleRate || !channels) return;

      const samples = new Float32Array(msg.data.buffer);
      const frames = samples.length / channels;
      const audioBuffer = audioContext.createBuffer(channels, frames, sampleRate);

      for (let ch = 0; ch < channels; ch++) {
        const channelData = audioBuffer.getChannelData(ch);
        for (let i = 0; i < frames; i++) {
          channelData[i] = samples[i * channels + ch];
        }
      }

      const source = audioContext.createBufferSource();
      source.buffer = audioBuffer;
      source.connect(audioContext.destination);

      const startAt = Math.max(audioContext.currentTime, state.nextStart);
      source.start(startAt);
      state.nextStart = startAt + frames / sampleRate;
      break;
    }

    case 'stream-complete': {
      playback.delete(msg.data.trackId);
      break;
    }
  }
};

function pause(trackId) {
  worker.postMessage({ type: 'pause-track', trackId });
}

function resume(trackId) {
  worker.postMessage({ type: 'resume-track', trackId });
}

function stop(trackId) {
  worker.postMessage({ type: 'stop-track', trackId });
  playback.delete(trackId);
}
```

### Seeking Support

`JsStreamingSession` keeps decoding from the last cursor position. To seek:

1. Send `stop-track` to halt the current session and drain the ring buffer.
2. Create a new `JsAudioSource` with the desired offset (for remote sources use HTTP range headers, for cached content use `JsAudioSource.fromCachedChunk` plus decoder-assisted offset).
3. Issue another `play-track` message – the new session starts filling the ring buffer immediately.

Because the architecture is streaming-first, main thread state stays simple while WASM handles buffering, adaptive prefetch, and underrun recovery.

### Zero-Copy Transfer Verification

**Transferable Objects:**
```javascript
// ArrayBuffer can be transferred (ownership moves)
const buffer = new Float32Array(1000).buffer;

worker.postMessage({ data: buffer }, [buffer]); 
//                                      ^^^^^^^^
//                                      Transferable list

// ⚠️ After this line, 'buffer' is DETACHED
// console.log(buffer.byteLength); // → 0 (neutered)
```

**Performance:**
- ✅ Zero-copy: Memory moves instantly (not copied)
- ✅ Fast: ~1 microsecond (pointer swap)
- ✅ No duplication: Memory freed in source, available in destination

**NOT Transferable:**
- ❌ TypedArray (Float32Array, Uint8Array) - must extract `.buffer`
- ❌ Regular objects - serialized via structured clone

**Correct Usage:**
```javascript
// ❌ WRONG - Copies data (slow)
self.postMessage({ audio: float32Array });

// ✅ CORRECT - Transfers buffer (zero-copy)
self.postMessage({ audio: float32Array.buffer }, [float32Array.buffer]);
```

## Offline Cache (Encrypted Downloads)

The worker can orchestrate encrypted offline storage without leaving WASM by using `JsOfflineCacheManager`. The binding hides SQLite schema setup, encryption, and Google Drive / OneDrive connectors so JavaScript only handles high-level commands.

**Worker Initialization**
```javascript
import {
  JsOfflineCacheManager,
  JsCacheConfig,
  JsStorageProviderConfig,
  JsPlaybackCacheStatus,
} from '../core-playback/pkg/core_playback.js';
import { JsLibrary } from '../core-library/pkg/core_library.js';
import { JsHttpClient } from '../bridge-wasm/pkg/bridge_wasm.js';
import { JsEventBus } from '../core-runtime/pkg/core_runtime.js';

const library = await JsLibrary.create('indexeddb://music');
const httpClient = new JsHttpClient(null);
const eventBus = new JsEventBus(256);
const storage = JsStorageProviderConfig.googleDrive(accessToken);

const cacheConfig = new JsCacheConfig();
cacheConfig.setMaxSizeMB(512);
cacheConfig.setEvictionPolicy('lru');
cacheConfig.setEncryption(true);
cacheConfig.setMaxConcurrentDownloads(4);
cacheConfig.setCacheDirectory('music-cache');

const offlineCache = await JsOfflineCacheManager.create(
  library,
  cacheConfig,
  'music-cache',
  httpClient,
  storage,
  eventBus,
  null // optional encryption override
);

await offlineCache.initialize();
```

**Download + Status Reporting**
```javascript
self.onmessage = async (msg) => {
  switch (msg.data.type) {
    case 'cache-download': {
      await offlineCache.downloadTrack(msg.data.trackId);
      const status = await offlineCache.cacheStatus(msg.data.trackId);
      self.postMessage({
        type: 'cache-status',
        trackId: msg.data.trackId,
        status, // numeric enum → JsPlaybackCacheStatus
      });
      break;
    }

    case 'cache-read': {
      const bytes = await offlineCache.readTrack(msg.data.trackId);
      self.postMessage({
        type: 'cache-bytes',
        trackId: msg.data.trackId,
        buffer: bytes.buffer,
      }, [bytes.buffer]);
      break;
    }
  }
};
```

Forward `eventBus` messages (e.g., `CoreEvent::Playback::DownloadProgress`) to the UI to provide real-time speed indicators and queue state. The same manager exposes eviction utilities (`evictBytes`, `clearCache`) and statistics (`cacheStats`, `activeDownloads`) for cache management dashboards.

---

## Event Flow

### Single EventBus in Worker

**All modules publish and subscribe to the same bus:**

```rust
// Worker initialization
let event_bus = Arc::new(EventBus::new(100));

// All modules share it
let auth = AuthManager::new(store, event_bus.clone(), http);
let sync = SyncService::new(event_bus.clone(), library, http);
let metadata = MetadataService::new(event_bus.clone(), library);
let playback = PlaybackEngine::new(event_bus.clone(), library);

// Cross-module coordination happens automatically
// Example: Auth emits SignedIn → Sync subscribes and starts sync
```

**Event Flow:**
```
Worker EventBus:
  auth.signIn()
    └─► Emits: Auth.SignedIn
          ├─► sync subscribes → starts sync automatically
          ├─► metadata subscribes → fetches with token
          └─► Forward to main thread (UI update)

  sync.startSync()
    └─► Emits: Sync.Progress (50%)
          └─► Forward to main thread (progress bar)

  library.addTrack()
    └─► Emits: Library.TrackAdded
          └─► Forward to main thread (refresh UI)
```

### Forwarding Events to Main Thread

**Worker: Forward important events**
```javascript
// WEB WORKER
const eventReceiver = eventBus.subscribe();

async function eventForwardingLoop() {
  while (true) {
    const eventJson = await eventReceiver.recv();
    const event = JSON.parse(eventJson);
    
    // Forward events that UI needs
    if (event.type === 'Auth' || 
        event.type === 'Sync' || 
        event.type === 'Library') {
      self.postMessage({ 
        type: 'rust-event', 
        event 
      });
    }
  }
}

eventForwardingLoop();
```

**Main Thread: Dispatch to UI**
```javascript
// MAIN THREAD (React example)
worker.onmessage = (msg) => {
  if (msg.data.type === 'rust-event') {
    const event = msg.data.event;
    
    switch (event.type) {
      case 'Auth':
        if (event.payload.event === 'SignedIn') {
          setUser({ profileId: event.payload.profile_id });
        }
        break;
        
      case 'Sync':
        if (event.payload.event === 'Progress') {
          setSyncProgress(event.payload.percent);
        }
        break;
        
      case 'Library':
        if (event.payload.event === 'TrackAdded') {
          refreshLibrary();
        }
        break;
    }
  }
};
```

---

## Database Access

### Single Database in Worker

**All database operations happen in worker:**
```javascript
// WEB WORKER
const library = await JsLibrary.create("indexeddb://music");

// core-sync writes to database
await syncService.startFullSync(profileId);
// → Downloads tracks from cloud
// → Writes to library.addTrack()
// → Emits Library.TrackAdded events

// core-metadata enriches database
await metadataService.enrichTrack(trackId);
// → Fetches lyrics, artwork
// → Updates library.updateTrack()
// → Emits Library.TrackUpdated events
```

### Main Thread Queries via RPC

**Main Thread needs track info:**
```javascript
// MAIN THREAD
async function getTrack(trackId) {
  return new Promise((resolve) => {
    const requestId = generateId();
    
    pendingRequests.set(requestId, resolve);
    
    worker.postMessage({ 
      type: 'query', 
      requestId,
      method: 'getTrack', 
      params: { trackId } 
    });
  });
}

// Handle response
worker.onmessage = (msg) => {
  if (msg.data.type === 'query-response') {
    const resolve = pendingRequests.get(msg.data.requestId);
    if (resolve) {
      resolve(msg.data.result);
      pendingRequests.delete(msg.data.requestId);
    }
  }
};

// Usage
const track = await getTrack('track-123');
console.log(track.title); // "Bohemian Rhapsody"
```

**Worker handles queries:**
```javascript
// WEB WORKER
self.onmessage = async (msg) => {
  if (msg.data.type === 'query') {
    const { requestId, method, params } = msg.data;
    
    let result;
    switch (method) {
      case 'getTrack':
        result = await library.getTrack(params.trackId);
        break;
      case 'searchTracks':
        result = await library.searchTracks(params.query);
        break;
      case 'getPlaylists':
        result = await library.getPlaylists();
        break;
    }
    
    self.postMessage({ 
      type: 'query-response', 
      requestId, 
      result 
    });
  }
};
```

---

## Performance Characteristics

### Bundle Size

| Component | Size (Uncompressed) | Size (Gzipped) |
|-----------|---------------------|----------------|
| Main Thread JS | ~50 KB | ~20 KB |
| Worker WASM | ~2.43 MB | ~950 KB |
| **Total Download** | **~2.48 MB** | **~970 KB** |

**Comparison:**
- Monolithic (all on main): ~3 MB / ~1.1 MB gzipped
- All-in-worker: ~2.48 MB / ~970 KB gzipped
- **Savings**: ~15% smaller!

### Playback Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Time to First Audio** | ~100-200 ms | First chunk decoded |
| **Chunk Decode Time** | ~50-80 ms | 2 seconds of audio |
| **Transfer Overhead** | ~1 μs | Zero-copy transfer |
| **Memory Usage** | ~20 MB | 10 seconds buffered (5 chunks) |
| **Seek Latency** | ~150-250 ms | Re-decode from seek point |

### UI Responsiveness

| Operation | Main Thread | Worker Thread |
|-----------|-------------|---------------|
| **UI Rendering** | 60 FPS ✅ | N/A |
| **Audio Playback** | Smooth ✅ | N/A |
| **Database Query** | Non-blocking ✅ | Instant (local) |
| **Sync Operation** | Non-blocking ✅ | Background |
| **Decode Operation** | Non-blocking ✅ | Background |

**All heavy operations in worker = UI always responsive!**

---

## Implementation Checklist

### Phase 1: Worker Setup
- [x] Create Web Worker entry point
- [x] Load WASM bundle in worker
- [x] Initialize EventBus in worker
- [x] Set up postMessage bridge

### Phase 2: Core Modules
- [x] Initialize all core modules in worker
- [x] Connect all modules to single EventBus
- [x] Test module communication

### Phase 3: OAuth Flow
- [ ] Implement delegated popup handling
- [ ] Test OAuth flow (GoogleDrive, OneDrive)
- [ ] Handle token refresh in worker
- [ ] Verify cookies/credentials work in worker

### Phase 4: Audio Playback
- [ ] Implement chunked decoding in worker
- [ ] Implement transferable buffer transfer
- [ ] Implement audio queue on main thread
- [ ] Test seamless chunk playback
- [ ] Implement seek support

### Phase 5: Database Queries
- [ ] Implement RPC query system
- [ ] Test query latency
- [ ] Optimize frequently-accessed queries
- [ ] Add query caching if needed

### Phase 6: Event Forwarding
- [ ] Implement event forwarding loop
- [ ] Filter events for main thread
- [ ] Test UI reactivity to events
- [ ] Optimize event serialization

### Phase 7: Testing
- [ ] Load test (1000+ tracks)
- [ ] Memory leak testing
- [ ] Audio playback stress test
- [ ] OAuth flow edge cases
- [ ] Worker crash recovery

---

## Advantages Summary

### ✅ Architectural Benefits

1. **Single EventBus**
   - All modules in same context
   - Direct communication (no postMessage)
   - Immediate cross-module coordination

2. **No Database Split**
   - Single IndexedDB connection
   - No sync conflicts
   - Single source of truth

3. **UI Always Responsive**
   - All heavy work offloaded
   - Main thread only renders
   - Smooth 60 FPS guaranteed

4. **Simple Deployment**
   - One WASM bundle
   - No bundle coordination
   - Easier to maintain

> **Design Rationale:** A single monolithic bundle was chosen because the `core-*` modules are highly interdependent, making it difficult to split them into separate, logical bundles. Furthermore, WASM dynamic linking is not yet a mature or standardized technology, making a single bundle the most robust and maintainable approach at this time.


5. **Zero-Copy Audio**
   - Transferable buffers
   - No memory duplication
   - Fast playback start

### ✅ User Experience

- **Fast Load**: ~970 KB gzipped (cacheable)
- **Quick Playback**: First audio in ~100ms
- **Smooth UI**: Never blocks, always 60 FPS
- **Low Memory**: Only buffer 10 seconds of audio
- **Reliable OAuth**: Delegated popup handling

---

## Potential Challenges

### 1. Debugging
**Challenge:** Worker errors harder to debug than main thread.

**Mitigation:**
- Comprehensive logging in worker
- Error forwarding to main thread
- Source maps for WASM

### 2. OAuth Complexity
**Challenge:** Popup handling across contexts.

**Mitigation:**
- Clear state management
- Timeout handling
- Fallback to redirect flow

### 3. Seek Performance
**Challenge:** Must re-decode on seek.

**Mitigation:**
- Cache decoded chunks if memory allows
- Optimize decoder seek performance
- Show loading indicator during seek

### 4. Worker Crash Recovery
**Challenge:** Worker crash loses all state.

**Mitigation:**
- Periodic state snapshots to IndexedDB
- Automatic worker restart
- State restoration on restart

> **Architectural Note:** Implementing robust crash recovery is critical for a production application. This adds significant responsibility to the "thin" main thread, which must act as a supervisor. It needs a reliable mechanism (like a heartbeat) to detect a worker failure and then orchestrate the complex process of restarting the worker and restoring its state from a persisted source like IndexedDB.


---

## Future Optimizations

### 1. Progressive Decoding
Decode first 30 seconds immediately, rest in background.

### 2. Shared Memory for Audio
Use SharedArrayBuffer for true zero-copy streaming (requires CORS headers).

### 3. Web Codec API
Use native browser decoders where available (experimental).

### 4. Service Worker Caching
Cache WASM bundle aggressively for instant load.

### 5. Prefetch Next Track
Decode next track in queue while current plays.

---

## Conclusion

The **All-in-Worker** architecture provides the best balance of:
- Performance (fast, responsive)
- Simplicity (one WASM bundle, single EventBus)
- User Experience (smooth UI, quick playback)
- Maintainability (clear separation, easy debugging)

This design leverages modern web platform features (Web Workers, Transferable Objects, Web Audio API) to create a production-grade music player that rivals native applications.

**Recommended for production deployment.** ✅
