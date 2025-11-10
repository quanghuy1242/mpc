# WASM Bundle Architecture - Database Location Clarification

## Critical Design Constraint

**Database must reside in ONE JavaScript context only.**

### Why?

1. **IndexedDB is context-bound**: Each Web Worker has its own separate IndexedDB instance
2. **No shared memory for DB**: Unlike native (Arc<Database>), WASM cannot share database connections
3. **Sync writes to DB**: core-sync needs direct database write access
4. **Metadata writes to DB**: core-metadata enrichment needs direct database write access

## ✅ Correct Architecture: Database in Worker

```text
┌─────────────────────────────────────────────────────────────┐
│                    Main Thread (UI)                          │
│  ┌────────────────────────────────────────────────┐         │
│  │         CoreServiceMain (2-3MB)                │         │
│  │  • Auth flows (OAuth, token management)        │         │
│  │  • Event bus (subscribe to library changes)    │         │
│  │  • Worker coordination (task dispatch)         │         │
│  │  • UI state cache (read-only projection)       │         │
│  └────────────────┬───────────────────────────────┘         │
└───────────────────┼──────────────────────────────────────────┘
                    │
         ┌──────────┴─────────────┐
         │ postMessage            │
         │ (Query requests)       │
         │ (Sync commands)        │
         │ (Event subscriptions)  │
         └──────────┬─────────────┘
                    │
┌───────────────────▼──────────────────────────────────────────┐
│               Web Worker Pool (2-4 workers)                   │
│  ┌────────────────────────────────────────────────┐          │
│  │      CoreServiceWorker (4-5MB)                 │          │
│  │  ┌──────────────────────────────────────────┐ │          │
│  │  │       core-library (COMPLETE)            │ │          │
│  │  │  • Database (IndexedDB)                  │ │          │
│  │  │  • All repositories (Track/Album/etc)    │ │          │
│  │  │  • Query service                         │ │          │
│  │  │  • Cache metadata                        │ │          │
│  │  └──────────────────────────────────────────┘ │          │
│  │  ┌──────────────────────────────────────────┐ │          │
│  │  │       core-sync (Background sync)        │ │          │
│  │  │  • Provider scanning (Drive/OneDrive)    │ │          │
│  │  │  • Change detection                      │ │          │
│  │  │  • Conflict resolution                   │ │          │
│  │  │  • Database writes (via core-library)    │ │          │
│  │  └──────────────────────────────────────────┘ │          │
│  │  ┌──────────────────────────────────────────┐ │          │
│  │  │     core-metadata (Enrichment)           │ │          │
│  │  │  • MusicBrainz lookups                   │ │          │
│  │  │  • Lyrics fetching                       │ │          │
│  │  │  • Artwork downloads                     │ │          │
│  │  │  • Database updates (via core-library)   │ │          │
│  │  └──────────────────────────────────────────┘ │          │
│  └────────────────────────────────────────────────┘          │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│                  Dedicated Audio Worker                        │
│  ┌────────────────────────────────────────────────┐          │
│  │       CoreServiceAudio (2-3MB)                 │          │
│  │  • core-playback (decoder + streaming)         │          │
│  │  • Symphonia WASM (MP3/AAC/FLAC)               │          │
│  │  • Ring buffer management                      │          │
│  │  • SharedArrayBuffer for zero-copy             │          │
│  └────────────────────────────────────────────────┘          │
└───────────────────────────────────────────────────────────────┘
```

## Data Flow Examples

### Example 1: User Queries Library

```text
1. UI (Main Thread)
   └─> CoreServiceMain.queryTracks({artist: "Queen"})
   
2. Main Thread → Worker (postMessage)
   └─> { type: "query", method: "queryTracks", filter: {...} }
   
3. Worker (Database Access)
   └─> core-library.TrackRepository.query(filter)
   └─> IndexedDB read
   └─> Return results
   
4. Worker → Main Thread (postMessage)
   └─> { type: "query_result", tracks: [...] }
   
5. UI (Main Thread)
   └─> Render track list
```

### Example 2: Sync Job Writes to Database

```text
1. User triggers sync
   └─> UI: CoreServiceMain.startSync()
   
2. Main Thread → Worker (postMessage)
   └─> { type: "sync_start", provider: "google-drive" }
   
3. Worker (Sync Process)
   ├─> core-sync.SyncCoordinator.start()
   ├─> Scan Google Drive (via bridge-wasm HTTP)
   ├─> Detect new/changed files
   │
   ├─> For each new track:
   │   ├─> core-library.TrackRepository.create(track)
   │   └─> IndexedDB write (SAME CONTEXT - works!)
   │
   ├─> Emit progress events
   └─> Emit completion event
   
4. Worker → Main Thread (events via postMessage)
   ├─> { type: "event", event: "Sync.Progress", ... }
   └─> { type: "event", event: "Sync.Complete", ... }
   
5. UI (Main Thread)
   └─> Update progress bar
   └─> Refresh track list (send new query to worker)
```

### Example 3: Metadata Enrichment

```text
1. Worker (Background enrichment)
   ├─> core-metadata.EnrichmentService.enrichTrack(track_id)
   │
   ├─> Fetch from MusicBrainz API
   ├─> Download artwork
   │
   ├─> Update database (SAME CONTEXT)
   │   └─> core-library.TrackRepository.update(track_id, metadata)
   │   └─> IndexedDB write (works!)
   │
   └─> Emit event
   
2. Worker → Main Thread (event)
   └─> { type: "event", event: "Metadata.Updated", track_id }
   
3. UI (Main Thread)
   └─> Invalidate cache for that track
   └─> Re-query if visible
```

## ❌ Why Main Thread Cannot Have Database

### Broken Architecture (DON'T DO THIS)

```text
Main Thread: core-library (IndexedDB)
Worker Thread: core-sync (needs to write to DB)

Problem:
1. Sync worker scans files
2. Tries to write to database
3. ❌ Cannot access main thread's IndexedDB
4. Must postMessage to main thread
5. Main thread blocked during large batch inserts
6. UI freezes = BAD UX
```

### What Would Happen?

```typescript
// ❌ BROKEN: Database in main thread
// Worker needs to insert 1000 tracks

for (let i = 0; i < 1000; i++) {
  // Worker → Main thread (postMessage)
  await postMessage({ type: "db_insert", track: tracks[i] });
  
  // Wait for main thread response
  const result = await waitForResponse(); // Slow!
  
  // Main thread blocked processing inserts = UI frozen
}

// Result: 
// - 1000 postMessage round-trips
// - Main thread does 1000 IndexedDB inserts (blocking)
// - UI frozen for 10-30 seconds
// - User rage-quits
```

## ✅ Correct: Database in Worker

```typescript
// ✅ CORRECT: Database in worker
// Worker inserts directly

const batch = [];
for (let i = 0; i < 1000; i++) {
  batch.push(tracks[i]);
}

// Single batch insert in worker
await core_library.bulkInsert(batch); // Fast!

// Notify main thread when done
postMessage({ type: "event", event: "Sync.TracksAdded", count: 1000 });

// Result:
// - Single batch insert (fast)
// - Main thread never blocked
// - UI stays responsive
// - User happy
```

## Communication Patterns

### Pattern 1: Request-Response (Queries)

```typescript
// Main Thread
async function queryTracks(filter) {
  const requestId = crypto.randomUUID();
  
  worker.postMessage({
    id: requestId,
    type: "query",
    method: "queryTracks",
    params: { filter }
  });
  
  return new Promise((resolve) => {
    pendingRequests.set(requestId, resolve);
  });
}

// Worker
self.onmessage = async (e) => {
  const { id, type, method, params } = e.data;
  
  if (type === "query") {
    const result = await library[method](params);
    self.postMessage({ id, type: "result", data: result });
  }
};
```

### Pattern 2: Fire-and-Forget (Commands)

```typescript
// Main Thread
function startSync(provider) {
  worker.postMessage({
    type: "command",
    method: "startSync",
    params: { provider }
  });
  
  // Don't wait - listen for events instead
}

// Worker
self.onmessage = async (e) => {
  if (e.data.type === "command") {
    // Start long-running task
    syncCoordinator.start(e.data.params);
    
    // Emit events as it progresses
    // No response needed
  }
};
```

### Pattern 3: Event Stream (Updates)

```typescript
// Worker
syncCoordinator.on("progress", (event) => {
  self.postMessage({
    type: "event",
    event: "Sync.Progress",
    data: event
  });
});

// Main Thread
worker.onmessage = (e) => {
  if (e.data.type === "event") {
    eventBus.emit(e.data.event, e.data.data);
  }
};
```

## Bundle Sizing Impact

### Before (Incorrect Split)

- Main Bundle: 2-3MB (auth + library queries)
- Worker Bundle: 3-4MB (sync + metadata + library writes)
- Audio Bundle: 2-3MB (playback)
- **Problem**: core-library duplicated in both main + worker

### After (Correct Split)

- Main Bundle: **1.5-2MB** (auth only, smaller!)
- Worker Bundle: **4-5MB** (sync + metadata + library complete)
- Audio Bundle: 2-3MB (playback)
- **Total**: 8-10MB (no duplication)

## Implementation Notes

### Worker Initialization

```typescript
// worker.ts
import init, { JsLibrary, JsSyncCoordinator } from './core-service-worker.js';

let library: JsLibrary;
let syncCoordinator: JsSyncCoordinator;

self.onmessage = async (e) => {
  if (e.data.type === "init") {
    await init(); // Initialize WASM
    
    // Create database connection
    library = await JsLibrary.create("indexeddb://music-library");
    
    // Initialize sync with database access
    syncCoordinator = new JsSyncCoordinator(library);
    
    self.postMessage({ type: "init_complete" });
  }
  
  // Handle queries, commands, etc.
};
```

### Main Thread Cache (Optional Optimization)

```typescript
// Main thread can cache read-only data for instant UI updates
class LibraryCache {
  private cache = new Map<string, Track>();
  
  async getTrack(id: string): Promise<Track> {
    // Check cache first
    if (this.cache.has(id)) {
      return this.cache.get(id)!;
    }
    
    // Query worker if not cached
    const track = await queryWorker("getTrack", { id });
    this.cache.set(id, track);
    return track;
  }
  
  // Invalidate cache when worker emits update events
  onTrackUpdated(trackId: string) {
    this.cache.delete(trackId);
  }
}
```

## Summary

✅ **Correct Architecture**:
- Main Thread: UI, events, auth, worker coordination (NO DATABASE)
- Worker Thread: core-library (complete), core-sync, core-metadata (WITH DATABASE)
- Audio Worker: core-playback only

✅ **Benefits**:
- Database writes don't block UI
- Batch operations are fast
- Single source of truth
- No context synchronization issues

❌ **Never Do**:
- Split core-library across contexts
- Put database in main thread
- Try to share database connections

---

**Key Takeaway**: In WASM, the database must live in the worker where the write operations happen. The main thread is for UI only and queries via postMessage.
