# Archived Architecture & Strategy Notes

This document contains a consolidation of various architectural and strategic notes that may be outdated but are preserved for historical context. For the current, definitive architecture, please refer to `WASM_ARCHITECTURE_FINAL.md`.

---
---

## From: `bundle_architecture.md`

# Core Service Architecture - Phase 6 Implementation Plan

**⚠️ DEPRECATED - WASM Section Outdated**

**Document Version**: 1.0  
**Date**: November 8, 2025  
**Status**: Planning Phase

**Note:** The WASM architecture section in this document is outdated. For the current WASM design, see:
- **[WASM_ARCHITECTURE_FINAL.md](./WASM_ARCHITECTURE_FINAL.md)** - All-in-Worker architecture (current)

The Desktop/Mobile sections remain valid.  

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Architecture Overview](#architecture-overview)
3. [Module Dependencies](#module-dependencies)
4. [Core Service API Design](#core-service-api-design)
5. [Initialization & Bootstrap](#initialization--bootstrap)
6. [Orchestration Patterns](#orchestration-patterns)
7. [State Management](#state-management)
8. [Event Flow & Communication](#event-flow--communication)
9. [Error Handling Strategy](#error-handling-strategy)
10. [Cross-Platform Considerations](#cross-platform-considerations)
11. [Implementation Roadmap](#implementation-roadmap)
12. [Testing Strategy](#testing-strategy)

---

## Executive Summary

The **core-service** module serves as the unified façade and orchestration layer for the Music Platform Core. It provides a single, ergonomic API that integrates all domain modules (auth, sync, library, metadata, playback) with platform bridge implementations (HTTP, filesystem, secure storage, etc.).

**Key Goals**:
- **Single Point of Entry**: Host applications interact exclusively with `CoreService`
- **Dependency Injection**: All platform bridges injected at initialization
- **Type Safety**: Compile-time verification of dependencies
- **Async-First**: Full async/await support with cancellation
- **Cross-Platform**: Works seamlessly on native (desktop/mobile) and WASM
- **Event-Driven**: Real-time state updates via event bus
- **Graceful Degradation**: Optional features fail gracefully when unavailable

**Platform-Specific Design**:
- **Desktop/Mobile**: CoreService is **optional** - modules can be used directly for maximum flexibility
- **WASM**: CoreService is **required** - split into 3 specialized bundles for multi-context coordination

---

## Platform Usage Overview

### Desktop/Mobile - Direct Module Usage (Recommended)

**Architecture**: Direct module composition without CoreService

```text
┌─────────────────────────────────────────────────────────────┐
│                  Desktop Application                         │
└────────────────────────────┬────────────────────────────────┘
                             │
        ┌────────────────────┼─────────────────────┐
        │                    │                     │
        ▼                    ▼                     ▼
  ┌──────────┐        ┌──────────┐         ┌──────────┐
  │AuthManager│        │SyncCoord │         │QuerySvc  │
  └────┬─────┘        └────┬─────┘         └────┬─────┘
       │                   │                     │
       └───────────────────┴─────────────────────┘
                           │
                    Tokio Runtime
              (Multi-threaded scheduler)
```

**Threading Model**:
- **Main Thread**: API calls, event handling, UI coordination
- **Tokio Worker Pool**: 4-16 threads (based on CPU cores)
  - Sync jobs (parallel file listing, metadata extraction)
  - Background metadata enrichment (concurrent API calls)
  - Audio decoding (producer thread)
  - Database operations (batched writes)
- **Platform Audio Thread**: PCM sample playback (consumer)

**Characteristics**:
- ✅ Zero orchestration overhead
- ✅ Full control over module wiring
- ✅ Direct function calls (no serialization)
- ✅ Shared memory via `Arc<T>` (zero-copy)
- ✅ True parallelism across CPU cores
- ✅ Simple single-binary deployment

**When to Use**:
- Advanced applications needing custom module composition
- Performance-critical use cases
- Applications already using Tokio runtime
- When maximum flexibility is needed

### Desktop/Mobile - CoreService (Optional)

**Architecture**: Unified API wrapper around modules

```text
┌─────────────────────────────────────────────────────────────┐
│                  Desktop Application                         │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │   CoreService   │ (Orchestration layer)
                    └────────┬────────┘
                             │
        ┌────────────────────┼─────────────────────┐
        │                    │                     │
        ▼                    ▼                     ▼
  ┌──────────┐        ┌──────────┐         ┌──────────┐
  │AuthManager│        │SyncCoord │         │QuerySvc  │
  └────┬─────┘        └────┬─────┘         └────┬─────┘
       │                   │                     │
       └───────────────────┴─────────────────────┘
                           │
                    Tokio Runtime
              (Multi-threaded scheduler)
```

**Threading Model**: Same as direct usage (Tokio multi-threaded)

**Characteristics**:
- ✅ Consistent API across platforms
- ✅ Simplified initialization
- ✅ Good for FFI boundaries (Python/C/Swift)
- ⚠️ Minor orchestration overhead
- ⚠️ Less flexibility than direct usage

**When to Use**:
- Cross-platform applications (shared API with WASM)
- Simple applications preferring convenience
- FFI/language bindings (PyO3, UniFFI)
- When API consistency matters more than flexibility

### WASM - CoreService (Required)

**Architecture**: 3-bundle split for multi-context coordination

```text
┌─────────────────────────────────────────────────────────────┐
│                    Web Application (UI)                      │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │   Main Thread (~1.8MB)       │
              │  • core-auth (OAuth)         │
              │  • core-playback (Web Audio) │
              │  • core-runtime (EventBus)   │
              │  • UI rendering              │
              │  • Worker coordination       │
              └──────┬───────────────────────┘
                     │
                     │ postMessage
                     │ (library queries,
                     │  sync commands)
                     │
                     ▼
              ┌──────────────────────────────┐
              │   Web Worker (~950KB)        │
              │  • core-library (Database)   │
              │  • core-sync (Cloud sync)    │
              │  • core-metadata (Enrich)    │
              │  • core-runtime (EventBus)   │
              └──────────────────────────────┘
```

**Threading Model**:
- **Main Thread**: Single-threaded, `!Send`
  - **core-auth**: OAuth flows (must be on main - popup/redirect)
  - **core-playback**: Audio decoding + playback (Web Audio API only on main)
  - **Library queries**: Via postMessage RPC to worker
  - Event subscription and UI rendering
  - Uses `Rc<RefCell<T>>` for state

- **Web Worker**: Single worker, separate JavaScript context
  - **core-library**: Database operations (IndexedDB per-context)
  - **core-sync**: Cloud sync, long-running operations
  - **core-metadata**: Metadata enrichment (CPU-intensive)
  - Responds to queries from main thread
  - Forwards events to main via postMessage

**Bundle Breakdown**:
- **Main Bundle** (~1.8MB / ~600KB gzipped): core-auth, core-playback, core-runtime, bridge-wasm
- **Worker Bundle** (~950KB / ~350KB gzipped): core-library, core-sync, core-metadata, core-runtime, bridge-wasm
- **Total**: ~2.75MB uncompressed / ~950KB gzipped

**Critical Design Decisions**:

1. **core-library ONLY in worker** because:
   - Database connections cannot be shared across JavaScript contexts
   - IndexedDB/OPFS access is per-context
   - Sync operations need direct database access
   - Main thread queries database via postMessage RPC

2. **core-playback MUST be on main** because:
   - Web Audio API only available on main thread
   - No separate audio worker (complexity not justified)
   - Audio decoding happens synchronously before playback

3. **core-auth MUST be on main** because:
   - OAuth popup/redirect flows require main thread
   - Token refresh during playback needs immediate access

4. **EventBus per context** because:
   - Cannot share WASM objects across JavaScript contexts
   - Each thread has its own EventBus instance
   - Events forwarded via postMessage when needed

**Characteristics**:
- ✅ Optimal bundle sizes (~950KB gzipped total)
- ✅ Main thread stays responsive (heavy work in worker)
- ✅ Simple 2-thread model (main + worker)
- ⚠️ Serialization overhead for postMessage (library queries)
- ⚠️ More complex than native (cross-context coordination)
- ❌ Cannot use modules directly (required orchestration)

**When to Use**: Always (only option for web deployment)

### Comparison Summary

| Aspect | Desktop Direct | Desktop CoreService | WASM CoreService |
|--------|---------------|--------------------|--------------------|
| **Architecture** | Direct modules | Wrapper + modules | 3-bundle split |
| **Threading** | Tokio multi-threaded | Tokio multi-threaded | Single-threaded + Workers |
| **Parallelism** | True (shared memory) | True (shared memory) | Simulated (postMessage) |
| **Bundle Size** | Single binary | Single binary | 9-11MB (3 bundles) |
| **Database Location** | Any thread | Any thread | Worker ONLY |
| **Initialization** | Manual wiring | Single call | Multi-context setup |
| **Performance** | Excellent | Excellent | Good |
| **Flexibility** | Maximum | Medium | Limited |
| **Complexity** | Medium | Low | High |
| **Required?** | No (optional) | No (optional) | Yes (mandatory) |
| **Best For** | Advanced apps | Simple/FFI apps | Web only |

---

## Key Takeaways

1. **Desktop/Mobile**: Use modules directly for performance and flexibility, or use CoreService for convenience and API consistency

---
---

## From: `EVENT_BUS_ARCHITECTURE.md`

# EventBus Architecture - Pub/Sub Model Explained

**✅ UPDATED for All-in-Worker Architecture**

See also: [WASM_ARCHITECTURE_FINAL.md](./WASM_ARCHITECTURE_FINAL.md) for complete WASM design.

---

## TL;DR - Single EventBus in Worker!

**All-in-Worker Architecture**: All business logic runs in a Web Worker with **ONE EventBus**:

```javascript
// WEB WORKER - ALL modules share single EventBus
const eventBus = new JsEventBus(100);

// All modules in worker
const authManager = new JsAuthManager(eventBus, httpClient, secureStore);
const syncService = new JsSyncService(eventBus, library, ...);
const libraryService = new JsLibrary(eventBus, ...);
const metadataService = new JsMetadataService(eventBus, ...);
const playbackEngine = new JsPlaybackEngine(eventBus, ...);

// Subscribe once for all events
const receiver = eventBus.subscribe();
while (true) {
  const event = await receiver.recv();
  
  // All modules communicate via this bus
  // Example: Auth.SignedIn → Sync automatically starts
  
  // Forward important events to main thread for UI
  if (event.type === 'Auth' || event.type === 'Sync' || event.type === 'Library') {
    self.postMessage({ type: 'rust-event', event });
  }
}

// ═══════════════════════════════════════════════════════════════════════

// MAIN THREAD - Thin UI layer only
worker.onmessage = (msg) => {
  if (msg.data.type === 'rust-event') {
    updateUI(msg.data.event); // React to worker events
  }
  
  if (msg.data.type === 'audio-chunk') {
    playAudio(msg.data.buffer); // Play decoded audio
  }
};
```

**Benefits of Single EventBus in Worker:**
- ✅ All modules communicate directly (no postMessage overhead)
- ✅ Cross-module coordination is instant (e.g., Auth → Sync)
- ✅ Single source of truth for events
- ✅ Simpler architecture (no event forwarding complexity)
- ✅ UI never blocks (all heavy work in worker)

---

## How the Pub/Sub Model Works

### Architecture: Broadcast Channel

The EventBus uses a **broadcast channel** pattern (not a queue!):

```
                    EventBus (Broadcast Channel)
                    ┌─────────────────────┐
                    │  Buffer: [E3, E2]   │ ← Circular buffer (capacity: 100)
                    │  Total sent: 5      │
                    └─────────────────────┘
                            ↓ ↓ ↓
              ┌─────────────┼─┼─────────────┐
              ↓             ↓ ↓             ↓
         Subscriber 1   Subscriber 2   Subscriber 3
         (UI Thread)    (Background)   (Analytics)
```

**Key Characteristics:**

1. **Broadcast** - Every subscriber gets a COPY of every event
2. **Buffered** - Keeps last N events in memory (prevents lagging subscribers from blocking)
3. **Async** - Non-blocking using Futures/Promises
4. **Multiple Producers** - Auth, Sync, Library, Playback all publish to same bus
5. **Multiple Consumers** - UI, logging, analytics all subscribe independently

---

## Inner Rust Implementation

### Under the Hood

```rust
// In core-async/src/sync.rs (WASM version)
pub mod broadcast {
    pub struct Sender<T> {
        shared: Rc<RefCell<Shared<T>>>,  // ← Shared state
    }
    
    pub struct Receiver<T> {
        shared: Rc<RefCell<Shared<T>>>,  // ← Same shared state
        next_index: u64,                  // ← Tracks which event to read next
    }
    
    struct Shared<T> {
        buffer: VecDeque<T>,        // ← Circular buffer (FIFO queue)
        capacity: usize,            // ← Max buffer size (100 in your case)
        total_sent: u64,            // ← Global event counter
        receiver_count: usize,      // ← How many subscribers
        waiters: Vec<Waker>,        // ← Async notification system
        closed: bool,               // ← Is channel closed?
    }
}
```

**Data Structure**: `VecDeque<T>` (double-ended queue)
- Not a simple queue - it's a **circular buffer**
- Old events get dropped when buffer is full
- Each receiver tracks its own position (`next_index`)

---

## The Receive Loop Pattern

### Yes, You Need a Loop!

```javascript
// Correct pattern
async function eventLoop(receiver) {
  while (true) {
    try {
      const event = await receiver.recv(); // ← Async wait (no busy-wait!)
      
      // Handle event
      switch (event.type) {
        case 'Auth':
          console.log('Auth event:', event);
          break;
        case 'Sync':
          console.log('Sync event:', event);
          break;
        case 'Library':
          console.log('Library event:', event);
          break;
        case 'Playback':
          console.log('Playback event:', event);
          break;
      }
    } catch (error) {
      if (error.message.includes('Lagged')) {
        // Subscriber fell behind, some events were missed
        console.warn('Missed some events!');
        continue; // Keep going
      } else if (error.message.includes('Closed')) {
        // EventBus was shut down
        console.log('EventBus closed');
        break;
      }
    }
  }
}
```

### How `await receiver.recv()` Works

**NOT a spin-loop!** It uses proper async/await:

1. **Call `recv()`** - Returns a Future/Promise
2. **No events available?** - Registers a Waker and suspends
3. **Event arrives?** - Waker is notified, async runtime resumes
4. **Returns event** - Your code continues

```rust
// Simplified Rust implementation
impl<'a, T: Clone> Future for RecvFuture<'a, T> {
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.receiver.try_recv() {
            Ok(value) => Poll::Ready(Ok(value)),      // ✅ Got event immediately
            Err(TryRecvError::Empty) => {
                // No event yet - register waker and suspend
                shared.waiters.push(cx.waker().clone()); // ← Save waker
                Poll::Pending                             // ← Suspend (NO CPU usage)
            }
        }
    }
}
```

**When event is published:**
```rust
pub fn send(&self, value: T) -> Result<usize, SendError<T>> {
    // 1. Add to buffer
    shared.buffer.push_back(value);
    
    // 2. Wake all waiting receivers
    for waker in waiters {
        waker.wake();  // ← Wakes up suspended recv() calls
    }
}
```

---

## Why Only One EventBus?

### Architecture Benefits

**Single Bus (Recommended):**
```javascript
// Create once
const eventBus = new JsEventBus(100);

// All modules use same bus
const authManager = new JsAuthManager(eventBus, ...);
const syncService = new JsSyncService(eventBus, ...);

// One subscriber sees ALL events
const receiver = eventBus.subscribe();
```

**Advantages:**
- ✅ Centralized event flow (easier debugging)
- ✅ Cross-module coordination (sync can react to auth events)
- ✅ Single event log for entire app
- ✅ Simpler architecture
- ✅ Less memory overhead

**Multiple Buses (Not Recommended):**
```javascript
// Bad: Separate buses
const authBus = new JsEventBus(100);
const syncBus = new JsEventBus(100);

const authManager = new JsAuthManager(authBus, ...);
const syncService = new JsSyncService(syncBus, ...);

// Need separate subscribers for each
const authReceiver = authBus.subscribe();
const syncReceiver = syncBus.subscribe();
```

**Problems:**
- ❌ Sync can't react to auth events
- ❌ No unified event timeline
- ❌ More complex subscriber management
- ❌ Duplicate memory buffers

---

## Real-World Usage Pattern

### Complete Example

```javascript
import { JsEventBus } from './core_runtime';
import { JsHttpClient, JsSecureStore } from './bridge_wasm';
import { JsAuthManager } from './core_auth';
import { JsSyncService } from './core_sync';

// 1. Create shared infrastructure (ONCE)
const eventBus = new JsEventBus(100);
const httpClient = new JsHttpClient();
const secureStore = new JsSecureStore("my-app");

// 2. Create all services with SAME event bus
const authManager = new JsAuthManager(eventBus, httpClient, secureStore);
const syncService = new JsSyncService(eventBus, library, httpClient);
const playback = new JsPlaybackEngine(eventBus, audioContext);

// 3. Subscribe ONCE to handle ALL events
const receiver = eventBus.subscribe();

async function startEventLoop() {
  while (true) {
    try {
      const eventJson = await receiver.recv();
      const event = JSON.parse(eventJson);
      
      // Route events to appropriate handlers
      switch (event.type) {
        case 'Auth': {
          if (event.kind === 'SignedIn') {
            console.log('User signed in:', event.profile_id);
            // Start sync automatically
            await syncService.startFullSync(event.profile_id);
          }
          break;
        }
        
        case 'Sync': {
          if (event.kind === 'Progress') {
            console.log(`Sync: ${event.percent}% complete`);
            updateProgressBar(event.percent);
          }
          break;
        }
        
        case 'Library': {
          if (event.kind === 'TrackAdded') {
            console.log('New track:', event.title);
            refreshLibraryUI();
          }
          break;
        }
        
        case 'Playback': {
          if (event.kind === 'Started') {
            console.log('Now playing:', event.title);
            updateNowPlaying(event);
          }
          break;
        }
      }
    } catch (error) {
      console.error('Event loop error:', error);
      // Handle lagged/closed errors
    }
  }
}

// Start the loop
startEventLoop();
```

---

## Multiple Subscribers

You CAN have multiple subscribers if needed:

```javascript
// Subscriber 1: UI updates
const uiReceiver = eventBus.subscribe();
async function uiEventLoop() {
  while (true) {
    const event = await uiReceiver.recv();
    updateUI(event);
  }
}

// Subscriber 2: Analytics
const analyticsReceiver = eventBus.subscribe();
async function analyticsEventLoop() {
  while (true) {
    const event = await analyticsReceiver.recv();
    sendToAnalytics(event);
  }
}

// Subscriber 3: Debug logging
const debugReceiver = eventBus.subscribe();
async function debugEventLoop() {
  while (true) {
    const event = await debugReceiver.recv();
    console.log('[DEBUG]', event);
  }
}

// Start all loops
uiEventLoop();
analyticsEventLoop();
debugEventLoop();
```

**All subscribers get ALL events independently!**

---

## Lagging and Buffer Management

### What Happens When Subscriber is Slow?

```
Time:     T1    T2    T3    T4    T5
Events:   E1    E2    E3    E4    E5

Buffer (capacity=3):
  T1: [E1]
  T2: [E1, E2]
  T3: [E1, E2, E3]
  T4: [E2, E3, E4]      ← E1 dropped (buffer full)
  T5: [E3, E4, E5]      ← E2 dropped
  
Slow subscriber still at E1:
  - Tries to read E1 → Lagged error!
  - Skips to E3 (oldest available)
```

**Handle lagging:**
```javascript
try {
  const event = await receiver.recv();
  process(event);
} catch (error) {
  if (error.message.includes('Lagged')) {
    // Subscriber fell behind, some events were lost
    console.warn('Missed events - consider increasing buffer size');
    // Continue processing from current position
  }
}
```

**Increase buffer if needed:**
```javascript
const eventBus = new JsEventBus(1000); // Bigger buffer for slow subscribers
```

---

## Summary

### Key Takeaways

1. **One EventBus for entire app** - All modules publish/subscribe to same bus
2. **Broadcast channel** - Every subscriber gets every event
3. **Async/await loop** - `while (true) { await receiver.recv(); }` (no CPU waste!)
4. **Inner type** - `VecDeque<T>` circular buffer with async Waker notifications
5. **Multiple subscribers OK** - Each gets independent copy of events
6. **Handle lagging** - Buffer can overflow for slow subscribers

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Main Thread                          │
│                                                         │
│  ┌──────────┐        ┌─────────────┐                  │
│  │ EventBus │◄───────┤ AuthManager │ (emits events)   │
│  │(capacity │        └─────────────┘                  │
│  │  = 100)  │                                          │
│  └────┬─────┘        ┌─────────────┐                  │
│       ├──────────────┤ SyncService │ (emits events)   │
│       │              └─────────────┘                  │
│       │                                                │
│       │              ┌─────────────┐                  │
│       └──────────────┤ Playback    │ (emits events)   │
│       │              └─────────────┘                  │
│       │                                                │
│       ├──► Subscriber 1 (UI updates)                  │
│       ├──► Subscriber 2 (Analytics)                   │
│       └──► Subscriber 3 (Logging)                     │
│                                                         │
│  All subscribers receive ALL events!                   │
└─────────────────────────────────────────────────────────┘
```

**The beauty**: Cross-module coordination without tight coupling! Auth emits SignedIn → Sync reacts automatically → Library gets updated → UI reflects changes. All through ONE event bus. 🎉

---
---

## From: `LAZY_LOADING_SHARED_DEPS.md`

# Lazy Loading WASM with Shared Dependencies

## Question 2: Does lazy loading duplicate shared dependencies?

**Short Answer:** With default builds, **YES** - shared code gets duplicated. But there are solutions!

---

## 🔍 The Problem

### **Scenario:**
```
core-runtime.wasm (403 KB)
  ├── serde (50 KB)
  ├── futures (30 KB)
  └── runtime code (323 KB)

core-library.wasm (545 KB)  
  ├── serde (50 KB)          ← DUPLICATE!
  ├── futures (30 KB)        ← DUPLICATE!
  ├── sqlx (200 KB)
  └── library code (265 KB)
```

**Total if loaded separately:** 948 KB  
**Actual unique code:** ~650 KB  
**Wasted (duplicated):** ~300 KB (30% overhead!)

### **Why This Happens:**

Each WASM module is **self-contained** - it includes ALL its dependencies. When you compile separately:

```rust
// core-library depends on core-runtime
[dependencies]
core-runtime = { path = "../core-runtime" }
```

The compiled `core-library.wasm` includes:
- ✅ core-library code
- ✅ **All of core-runtime** (embedded)
- ✅ All shared dependencies (serde, futures, etc.)

So loading both means:
```
Browser Memory:
├── core-runtime.wasm (403 KB)
│   └── Contains: runtime + serde + futures
│
└── core-library.wasm (545 KB)
    └── Contains: library + runtime + serde + futures
                           ^^^^^^^^^^^^^^^^^^^^^^^^
                           DUPLICATED FROM ABOVE!
```

---

## ✅ Solution 1: Dynamic Linking (Experimental)

**Status:** 🚧 Experimental, not recommended yet

WASM supports dynamic linking, but it's not mature:

```toml
[lib]
crate-type = ["cdylib"]  # Dynamic library

[dependencies]
core-runtime = { path = "../core-runtime", features = ["dynamic"] }
```

**Problems:**
- ❌ Limited browser support
- ❌ Complex setup
- ❌ Performance overhead
- ❌ Not production-ready

---

## ✅ Solution 2: Code Splitting with Webpack/Vite

Let the bundler dedupe shared code:

### **Setup with Vite:**

```javascript
// vite.config.js
export default {
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          // Extract shared runtime
          'wasm-runtime': ['./wasm/core-runtime/core_runtime.js'],
          'wasm-library': ['./wasm/core-library/core_library.js'],
        },
      },
    },
  },
  optimizeDeps: {
    exclude: ['*.wasm'],  // Don't pre-bundle WASM
  },
};
```

**Result:**
```
dist/
├── wasm-runtime.js       (50 KB - glue code)
├── core_runtime.wasm     (403 KB)
├── wasm-library.js       (50 KB - glue code)
└── core_library.wasm     (545 KB - still has duplication)
```

**Problem:** WASM binaries still duplicate code! Bundler only dedupes JS.

---

## ✅ Solution 3: Monolithic Build (Recommended)

**Best solution for production:** Combine all modules into ONE WASM file.

### **Architecture:**

```
mpc-wasm/
├── Cargo.toml
└── src/
    └── lib.rs
```

```toml
# mpc-wasm/Cargo.toml
[package]
name = "mpc-wasm"

[dependencies]
core-runtime = { path = "../core-runtime" }
core-library = { path = "../core-library" }
core-sync = { path = "../core-sync" }
```

```rust
// mpc-wasm/src/lib.rs

// Re-export everything
pub use core_runtime::wasm::*;
pub use core_library::wasm::*;
pub use core_sync::wasm::*;

// Optionally, create a unified init
#[wasm_bindgen(start)]
pub fn init() {
    // Initialize everything
}
```

**Build:**
```powershell
wasm-pack build mpc-wasm --target web --release
```

**Result:**
```
mpc-wasm/pkg/
├── mpc_wasm.wasm        (600 KB - all modules, zero duplication!)
└── mpc_wasm.js          (80 KB - single glue code)
```

### **Size Comparison:**

| Approach | Total Size | Duplication | HTTP Requests |
|----------|------------|-------------|---------------|
| Separate modules | 948 KB | ~300 KB (30%) | 2-4 |
| Monolithic | 600 KB | 0 KB | 1 |
| **Savings** | **-37%** | **-100%** | **-50%** |

---

## ✅ Solution 4: Hybrid Approach (Best of Both)

**For large apps:** Combine core modules + lazy load optional modules

### **Core Bundle (Always Load):**
```rust
// mpc-core-wasm/src/lib.rs
pub use core_runtime::wasm::*;
pub use core_library::wasm::*;
pub use core_auth::wasm::*;
// Essential modules
```

### **Optional Modules (Lazy Load):**
```rust
// Each stays separate
- core-metadata-wasm (artwork, lyrics)
- core-playback-wasm (audio processing)
- provider-gdrive-wasm (Google Drive)
```

### **TypeScript Loading:**

```typescript
// 1. Load core immediately (500 KB)
import initCore, { JsLibrary, JsEventBus } from './mpc-core.js';
await initCore();

const library = await JsLibrary.create("indexeddb://music");
const eventBus = new JsEventBus(100);

// 2. App is usable now!
showUI();

// 3. Lazy load features when needed
async function enableLyrics() {
  const { default: initMetadata, JsLyricsProvider } = 
    await import('./core-metadata.js');
  await initMetadata();
  return new JsLyricsProvider();
}

async function startPlayback() {
  const { default: initPlayback, JsAudioPlayer } = 
    await import('./core-playback.js');
  await initPlayback();
  return new JsAudioPlayer();
}
```

### **Benefits:**
- ✅ Fast initial load (core only)
- ✅ Features load on demand
- ✅ Zero duplication in core bundle
- ✅ Optional modules can duplicate core (acceptable if rarely loaded)

---

## 🎯 Recommendation by Use Case

### **Small App (< 3 modules)**
→ **Monolithic Build**
- Simplest
- Fastest
- No complexity

### **Medium App (3-6 modules)**
→ **Monolithic Build**
- Still recommended
- Trade ~100ms load time for simplicity
- Users won't notice

### **Large App (> 6 modules, optional features)**
→ **Hybrid Approach**
```
Core bundle:       runtime + library + auth (500 KB)
Optional modules:  metadata, playback, providers (200 KB each)
```

### **Progressive Web App**
→ **Hybrid + Service Worker**
```typescript
// Cache core bundle
navigator.serviceWorker.register('/sw.js');

// Lazy load and cache optional modules
const metadata = await caches.match('/metadata.wasm') 
  || await fetch('/metadata.wasm');
```

---

## 📊 Real Numbers for MPC Project

### **Current Separate Builds:**
```
core-runtime.wasm:   403 KB
core-library.wasm:   545 KB (includes runtime)
-----------------------------------------
Total:               948 KB
Duplication:         ~300 KB (runtime + serde + futures)
```

### **Proposed Monolithic:**
```
mpc-wasm.wasm:       600 KB (runtime + library combined)
-----------------------------------------
Total:               600 KB
Duplication:         0 KB
Savings:             -37%
```

### **Proposed Hybrid:**
```
Core (always):
  mpc-core.wasm:           500 KB (runtime + library + auth)
  
Optional (lazy):
  core-metadata.wasm:      180 KB (+ 50KB duplicate = 230 KB)
  core-playback.wasm:      150 KB (+ 50KB duplicate = 200 KB)
  provider-gdrive.wasm:    120 KB (+ 30KB duplicate = 150 KB)
-----------------------------------------
Initial load:              500 KB ✓
Full load (if all used):   1,080 KB
```

**Analysis:**
- Initial load: **48% smaller** (500 KB vs 948 KB)
- User enables metadata: +230 KB → 730 KB total
- User enables playback: +200 KB → 930 KB total
- Still ~2% larger than monolithic if ALL loaded
- But 48% faster initial load!

---

## 🔧 Implementation

### **Phase 1: Measure Current Usage**

```powershell
# Check actual sizes
Get-ChildItem core-runtime/pkg/*.wasm, core-library/pkg/*.wasm | 
  Select-Object Name, @{N='KB';E={[math]::Round($_.Length/1KB,0)}}
```

### **Phase 2: Create Monolithic Build**

```powershell
# Create new workspace member
New-Item -ItemType Directory mpc-wasm
```

```toml
# mpc-wasm/Cargo.toml
[package]
name = "mpc-wasm"
version = "0.1.0"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
core-runtime = { path = "../core-runtime" }
core-library = { path = "../core-library" }

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = { workspace = true }
```

```rust
// mpc-wasm/src/lib.rs
pub use core_runtime::wasm::*;
pub use core_library::wasm::*;
```

```powershell
# Build
wasm-pack build mpc-wasm --target web --release
```

### **Phase 3: Compare Sizes**

```powershell
# Compare
Write-Host "Separate:" -ForegroundColor Yellow
Get-ChildItem core-*/pkg/*.wasm | Measure-Object -Property Length -Sum

Write-Host "`nMonolithic:" -ForegroundColor Green
Get-ChildItem mpc-wasm/pkg/*.wasm | Measure-Object -Property Length -Sum
```

### **Phase 4: Benchmark Load Times**

```html
<script type="module">
  // Test separate
  console.time('separate');
  await Promise.all([
    import('./core-runtime/core_runtime.js'),
    import('./core-library/core_library.js'),
  ]);
  console.timeEnd('separate');

  // Test monolithic
  console.time('monolithic');
  await import('./mpc-wasm/mpc_wasm.js');
  console.timeEnd('monolithic');
</script>
```

---

## 💡 Key Insights

### **Why Separate Builds Duplicate:**

1. **Rust compilation model:** Each crate is a compilation unit
2. **WASM is self-contained:** No dynamic linking by default
3. **Dependencies are statically linked:** Embedded in each binary

### **Why Monolithic Works:**

1. **Single compilation unit:** Rust sees all code at once
2. **Link-time optimization:** Deduplicates at binary level
3. **One dependency tree:** serde, futures, etc. included once

### **When Duplication is OK:**

- Optional modules rarely loaded
- User-specific features (most users don't use all)
- Network is fast (mobile/slow networks = prefer monolithic)

---

## ✅ Decision Matrix

| Requirement | Recommended Approach |
|-------------|---------------------|
| Need smallest total size | Monolithic |
| Need fastest initial load | Hybrid (core + lazy) |
| Need simplest setup | Monolithic |
| Have optional features | Hybrid |
| Mobile-first | Monolithic |
| Desktop-first | Hybrid acceptable |
| PWA with caching | Hybrid + SW |

---

## 🚀 Quick Start: Monolithic Build

```powershell
# 1. Create workspace
mkdir mpc-wasm; cd mpc-wasm

# 2. Create Cargo.toml
@"
[package]
name = "mpc-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
core-runtime = { path = "../core-runtime" }
core-library = { path = "../core-library" }
wasm-bindgen = "0.2"
"@ | Out-File Cargo.toml

# 3. Create lib.rs
mkdir src
"pub use core_runtime::wasm::*;
pub use core_library::wasm::*;" | Out-File src/lib.rs

# 4. Build
wasm-pack build --target web --release

# 5. Check size
Get-ChildItem pkg/*.wasm
```

**Result:** Single WASM bundle with zero duplication!

---

## 📋 Summary

| Question | Answer |
|----------|--------|
| **Do separate modules duplicate code?** | Yes, ~30% duplication |
| **Does core-a including core-b mean bigger core-a?** | Yes, core-a contains all of core-b |
| **Can we avoid duplication?** | Yes, use monolithic build |
| **Should we always use monolithic?** | For production, yes. For lazy loading, use hybrid |
| **What about load time?** | Monolithic is actually faster (fewer HTTP requests) |

**Recommendation:** Monolithic build for MPC project (saves 37%, faster loading)

---
---

## From: `MULTI_WASM_LOADING.md`

# Loading Multiple WASM Modules - Strategy Guide

## Question 1: Loading 2+ WASM Modules in Browser

### **The Challenge**
When you have multiple WASM modules (`core-library.wasm`, `core-runtime.wasm`, etc.), you need to:
1. Load them efficiently
2. Share dependencies
3. Manage initialization order
4. Handle module size

### **Recommended Approach: Monolithic Build**

**For production, bundle everything into ONE WASM module.**

#### Why Monolithic is Better:
✅ **Single HTTP Request** - Faster loading
✅ **Shared Code** - No duplication of common dependencies  
✅ **Simpler Initialization** - One `init()` call
✅ **Better Optimization** - wasm-opt sees full picture
✅ **Smaller Total Size** - Shared code deduped

#### Example Structure:
```
mpc-wasm/
├── Cargo.toml           # Workspace member that combines all
├── src/
│   ├── lib.rs          # Re-exports from all modules
│   ├── library.rs      # Re-export core-library
│   ├── runtime.rs      # Re-export core-runtime
│   └── playback.rs     # Re-export core-playback
└── pkg/
    ├── mpc_wasm.wasm   # ONE bundle
    ├── mpc_wasm.js
    └── mpc_wasm.d.ts
```

```rust
// mpc-wasm/src/lib.rs
pub use core_library::wasm::*;
pub use core_runtime::wasm::*;
pub use core_playback::wasm::*;
```

### **Alternative: Separate Modules (Advanced)**

If you MUST load separately (e.g., lazy loading):

#### Option A: Sequential Loading
```typescript
import initRuntime from './core-runtime/core_runtime.js';
import initLibrary from './core-library/core_library.js';

// Load in dependency order
await initRuntime();
await initLibrary();
```

#### Option B: Parallel Loading
```typescript
const [runtimeWasm, libraryWasm] = await Promise.all([
  fetch('./core-runtime/core_runtime_bg.wasm'),
  fetch('./core-library/core_library_bg.wasm'),
]);

const [runtime, library] = await Promise.all([
  initRuntime(await runtimeWasm.arrayBuffer()),
  initLibrary(await libraryWasm.arrayBuffer()),
]);
```

#### Option C: Dynamic Import (Lazy Loading)
```typescript
// Load runtime immediately
import initRuntime, { JsEventBus } from './core-runtime/core_runtime.js';
await initRuntime();

// Load library only when needed
async function useLibrary() {
  const { default: init, JsLibrary } = await import('./core-library/core_library.js');
  await init();
  return JsLibrary.create("indexeddb://music");
}
```

### **Build Configuration**

#### Monolithic Build (Recommended)
```toml
# workspace-wasm/Cargo.toml
[package]
name = "mpc-wasm"

[dependencies]
core-library = { path = "../core-library" }
core-runtime = { path = "../core-runtime" }
core-playback = { path = "../core-playback" }
wasm-bindgen = "0.2"
```

```powershell
# Build single bundle
wasm-pack build workspace-wasm --target web --out-dir ../dist/wasm
```

#### Separate Builds (Current Approach)
```powershell
# Build each separately
wasm-pack build core-runtime --target web --out-dir pkg
wasm-pack build core-library --target web --out-dir pkg
wasm-pack build core-playback --target web --out-dir pkg
```

### **Distribution Strategy**

#### For NPM Package:
```json
{
  "name": "@mpc/wasm",
  "version": "0.1.0",
  "type": "module",
  "exports": {
    ".": "./mpc_wasm.js",
    "./runtime": "./core-runtime/core_runtime.js",
    "./library": "./core-library/core_library.js"
  },
  "files": [
    "*.wasm",
    "*.js",
    "*.d.ts"
  ]
}
```

#### For CDN:
```html
<!-- Single bundle -->
<script type="module">
  import init from 'https://cdn.example.com/mpc-wasm/mpc_wasm.js';
  await init();
</script>

<!-- Or separate -->
<script type="module">
  import initRuntime from 'https://cdn.example.com/mpc-wasm/runtime.js';
  import initLibrary from 'https://cdn.example.com/mpc-wasm/library.js';
  await Promise.all([initRuntime(), initLibrary()]);
</script>
```

### **Size Comparison**

| Approach | Total Size | HTTP Requests | Load Time |
|----------|------------|---------------|-----------|
| Separate (2 modules) | ~948 KB | 2 | ~500ms |
| Monolithic | ~600 KB | 1 | ~300ms |
| Lazy Load | ~600 KB | 1-2 | ~300-400ms |

*Monolithic is smaller because shared dependencies (serde, futures, etc.) aren't duplicated*

### **Recommendation**

✅ **For Production:** Use monolithic build
- Create `mpc-wasm` workspace member
- Re-export all public APIs
- Build single WASM bundle
- Simpler, faster, smaller

⚠️ **For Development:** Keep separate builds
- Faster incremental compilation
- Easier debugging
- Clear module boundaries
- Test modules independently

### **Implementation Plan**

1. **Create monolithic workspace:**
```powershell
New-Item -ItemType Directory mpc-wasm
```

2. **Add Cargo.toml:**
```toml
[package]
name = "mpc-wasm"
version = "0.1.0"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
core-library = { path = "../core-library" }
core-runtime = { path = "../core-runtime" }
wasm-bindgen = "0.2"
```

3. **Create lib.rs:**
```rust
// Re-export everything
pub use core_library::wasm::*;
pub use core_runtime::wasm::*;
```

4. **Build:**
```powershell
wasm-pack build mpc-wasm --target web --release
```

**Result:** One WASM file with all features!

---
---

## From: `WASM_BUNDLE_STRATEGY.md`

# WASM Bundle Strategy: 2-Bundle Architecture

**⚠️ DEPRECATED**: This document describes an outdated 2-bundle split architecture.

**✅ See [WASM_ARCHITECTURE_FINAL.md](./WASM_ARCHITECTURE_FINAL.md) for current All-in-Worker design.**

---

## Historical Context (Kept for Reference)

This document outlines the original 2-bundle split approach that separated main thread and worker logic. This approach was superseded by the simpler All-in-Worker architecture which provides better performance and maintainability.

---

## 📊 Dependency Graph

```
FOUNDATIONAL (no dependencies on other cores):
├── core-async (runtime abstraction)
├── bridge-traits (platform interfaces)
└── core-runtime (logging, events, config)

TIER 1 (depends on runtime only):
├── core-auth
│   └── → core-runtime, core-async
└── core-library
    └── → core-async, bridge-traits

TIER 2 (depends on tier 1):
├── core-metadata
│   └── → core-runtime, core-library, core-async
└── core-playback
    └── → core-runtime, core-auth, core-library, core-async

TIER 3 (depends on tier 2):
└── core-sync
    └── → core-runtime, core-auth, core-library, core-metadata, core-async
```

---

## 🧵 WASM Threading Constraints

### **Why Only 2 Threads?**

**JavaScript Context Limitations:**
- **Main Thread**: Required for DOM, Web Audio API, OAuth redirects
- **Web Worker**: Single worker for background tasks
- **No Audio Worker**: Audio decoding happens on main thread, playback uses Web Audio API
- **No Worker Pool**: Complexity and memory overhead outweigh benefits for this use case

### **Module Assignment Rules:**

```
Main Thread (MUST have):
├── core-playback → Web Audio API only on main thread
├── core-auth     → OAuth flows need main thread (popup/redirect)
└── UI queries    → Low-latency (via postMessage to worker)

Web Worker (MUST have):
├── core-library  → Database per context (cannot share)
├── core-sync     → Heavy I/O, long-running
└── core-metadata → CPU-intensive processing
```

### **EventBus Architecture:**

**Cannot share across threads!**
```javascript
// Main Thread
const mainBus = new JsEventBus(100);  // For auth, playback events

// Worker Thread (separate context)
const workerBus = new JsEventBus(100); // For sync, library events

// Bridge via postMessage
worker.onmessage = (msg) => {
  if (msg.data.type === 'event') {
    mainBus.emit(msg.data.event); // Forward to main thread subscribers
  }
};
```

---

## 🎯 2-WASM Split Strategy

### **Bundle 1: Main Thread (UI-focused)**
**File:** `mpc-wasm-main.wasm`  
**Target:** Main JavaScript thread (UI, auth, playback)

| Module | Size | Reason |
|--------|------|--------|
| core-runtime | ~100 KB | Events, logging, config |
| core-async | ~50 KB | Runtime abstraction |
| core-auth | ~80 KB | OAuth flows, token refresh |
| core-playback | **~1.4 MB** | ⚠️ **Audio decoders (heavy!)** |
| bridge-wasm | ~150 KB | HTTP, storage (for auth) |
| **Total** | **~1.78 MB** | **Compressed: ~550-650 KB** |

**Why these modules in main thread:**
- ✅ **core-playback MUST be here** - Web Audio API only available on main thread
- ✅ **core-auth MUST be here** - OAuth popup/redirect flows need main thread
- ✅ core-library queries via postMessage to worker (no DB on main!)
- ✅ Low-latency UI updates

---

### **Bundle 2: Worker Thread (Background tasks)**
**File:** `mpc-wasm-worker.wasm`  
**Target:** Web Worker (sync, metadata, database operations)

| Module | Size | Reason |
|--------|------|--------|
| core-runtime | ~100 KB | Events, logging (duplicated) |
| core-async | ~50 KB | Runtime abstraction (duplicated) |
| **core-library** | **~300 KB** | **⚠️ Database operations (MUST be here!)** |
| core-sync | ~200 KB | Cloud sync operations |
| core-metadata | ~150 KB | Metadata enrichment, artwork, lyrics |
| bridge-wasm | ~150 KB | HTTP, storage, database (duplicated) |
| **Total** | **~950 KB** | **Compressed: ~350-400 KB** |

**Why these modules in worker:**
- ✅ **core-library MUST be here** - IndexedDB/OPFS access per context
- ✅ core-sync: Heavy network I/O, long-running operations
- ✅ core-metadata: CPU-intensive (image processing, tag extraction)
- ✅ Prevents blocking UI
- ✅ Can run in parallel with playback
- ⚠️ **Main thread queries library via postMessage to worker**

---

## 📏 Duplication Analysis

| Module | Main Thread | Worker Thread | Duplication Cost |
|--------|-------------|---------------|------------------|
| **core-runtime** | ✅ | ✅ | ~100 KB |
| **core-async** | ✅ | ✅ | ~50 KB |
| **bridge-wasm** | ✅ | ✅ | ~150 KB |
| core-auth | ✅ | ❌ | 0 KB |
| core-playback | ✅ | ❌ | 0 KB |
| core-library | ❌ | ✅ | 0 KB |
| core-sync | ❌ | ✅ | 0 KB |
| core-metadata | ❌ | ✅ | 0 KB |
| **Total Duplication** | | | **~300 KB** |
| **Gzipped Duplication** | | | **~120 KB** |

**Note:** Duplicated modules are necessary because:
1. Each WASM module has isolated memory
2. Both threads need core functionality (events, logging, bridges)
3. **core-library is ONLY in worker** - main thread queries via postMessage
4. Browser caches the downloaded WASM files
5. Gzip reduces actual bandwidth cost significantly

**Critical**: core-library CANNOT be in both threads because:
- IndexedDB connections are per-context
- Database operations must happen in worker to avoid blocking UI
- Main thread makes queries via postMessage RPC

---

## ⚠️ core-playback Size Problem

### **The Challenge:**
- core-playback with all decoders: **~1.4 MB** (uncompressed)
- Includes: MP3, FLAC, Vorbis, Opus, AAC, WAV, ALAC decoders
- Makes main thread bundle very heavy

### **Why it MUST Stay in Main Thread:**

```typescript
// ❌ This WON'T WORK in Web Worker:
const audioContext = new AudioContext(); 
// ERROR: AudioContext is not defined in Worker scope!

// ✅ This ONLY works on main thread:
const audioContext = new AudioContext(); // ✅ Available
const source = audioContext.createBufferSource();
source.connect(audioContext.destination);
source.start(); // Play audio
```

**Web Audio API is ONLY available on main thread!**

---

## 💡 Solutions for core-playback Size

### **Option 1: Keep All Decoders in Main (Current)**
```
main-wasm: 1.93 MB → gzipped ~600-700 KB
```

**Pros:**
- ✅ Simple architecture
- ✅ All formats work immediately
- ✅ No complexity

**Cons:**
- ❌ Large initial download
- ❌ Slow initial load on slow networks

---

### **Option 2: Feature-Gated Decoders (Recommended)**

**Split into minimal + optional decoders:**

```toml
# mpc-wasm-main/Cargo.toml
[dependencies]
core-playback = { path = "../core-playback", default-features = false, features = [
  "decoder-mp3",    # Essential ~400 KB
  "decoder-aac",    # Essential ~300 KB
] }
```

**Create separate decoder bundles:**

```
Main bundle (essential):
├── mpc-wasm-main.wasm (mp3 + aac only)
│   Size: ~900 KB → gzipped ~350 KB

Optional bundles (lazy load):
├── mpc-wasm-flac.wasm (FLAC decoder)
│   Size: ~200 KB → gzipped ~80 KB
├── mpc-wasm-vorbis.wasm (Vorbis/Ogg)
│   Size: ~180 KB → gzipped ~70 KB
└── mpc-wasm-opus.wasm (Opus)
    Size: ~150 KB → gzipped ~60 KB
```

**Lazy loading:**

```typescript
// Load main bundle with essential decoders
import init from './mpc-wasm-main.js';
await init();

// Lazy load FLAC when user plays FLAC file
async function playFLAC(track: Track) {
  if (!flacDecoderLoaded) {
    const { default: initFlac } = await import('./mpc-wasm-flac.js');
    await initFlac();
    flacDecoderLoaded = true;
  }
  
  player.play(track); // Now FLAC decoder available
}
```

**Size comparison:**

| Approach | Initial Load | Full Load (all formats) |
|----------|--------------|-------------------------|
| All decoders | 1.93 MB (~700 KB gzip) | 1.93 MB |
| Essential only | 900 KB (~350 KB gzip) | 1.43 MB (~550 KB gzip) |
| **Savings** | **-53% initial** | **-25% total** |

---

### **Option 3: Decode in Worker + Transfer (Advanced)**

**Architecture:**
```
Worker Thread:
  ├── Decode audio to PCM buffer
  └── Transfer to main thread via SharedArrayBuffer

Main Thread:
  ├── Receive PCM buffer
  ├── Create AudioBuffer from PCM
  └── Play via Web Audio API
```

**Implementation:**

```typescript
// Worker: mpc-wasm-worker.wasm (includes core-playback)
self.onmessage = async (e) => {
  if (e.data.type === 'decode') {
    const pcmBuffer = await decoder.decode(e.data.audioData);
    
    // Transfer buffer to main thread
    self.postMessage({ 
      type: 'decoded', 
      pcmBuffer 
    }, [pcmBuffer.buffer]); // Transferable
  }
};

// Main: mpc-wasm-main.wasm (no decoders!)
worker.onmessage = (e) => {
  if (e.data.type === 'decoded') {
    const audioBuffer = audioContext.createBuffer(
      2, // stereo
      e.data.pcmBuffer.length / 2,
      44100
    );
    
    // Copy PCM data
    audioBuffer.copyToChannel(e.data.pcmBuffer, 0);
    
    // Play
    const source = audioContext.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(audioContext.destination);
    source.start();
  }
};
```

**Bundle sizes:**

| Bundle | Size | Contents |
|--------|------|----------|
| Main | ~500 KB | No decoders! |
| Worker | ~1.5 MB | All decoders |
| **Total** | **~2 MB** | More duplication |

**Pros:**
- ✅ Small main thread bundle
- ✅ Decoding doesn't block UI
- ✅ All decoders in one place

**Cons:**
- ❌ Higher total size (more duplication)
- ❌ Latency from worker communication
- ❌ Complex buffer management
- ❌ No streaming decode (must decode full file)

---

### **Option 4: AudioWorklet for Custom Decoders (Hybrid)**

**Architecture:**
```
Main Thread:
  └── mpc-wasm-main.wasm
      ├── Standard format decoding (MP3, AAC via Symphonia)
      └── AudioWorklet setup

AudioWorklet (runs in audio thread):
  └── custom-decoder-worklet.js
      └── WASM decoder module (loaded separately)
```

**For custom formats only:**

```typescript
// Main thread
await audioContext.audioWorklet.addModule('./custom-decoder-worklet.js');

const decoderNode = new AudioWorkletNode(audioContext, 'custom-decoder');
decoderNode.connect(audioContext.destination);

// Send compressed audio data
decoderNode.port.postMessage({ audioData: compressedBuffer });
```

**AudioWorklet still runs in main thread context but on audio thread!**

**Best for:**
- Custom/proprietary formats
- Real-time processing
- Low-latency requirements

---

## 🎯 Recommended Approach

### **Hybrid: Essential Decoders + Lazy Loading**

**Phase 1: Initial Load (~350 KB gzipped)**
```
mpc-wasm-main.wasm:
├── core-runtime
├── core-library
├── core-auth
└── core-playback (MP3 + AAC only) ← 80% of use cases
```

**Phase 2: Lazy Load on Demand**
```
When user plays FLAC:
  → Load mpc-wasm-flac.wasm (~80 KB gzipped)

When user plays Ogg/Vorbis:
  → Load mpc-wasm-vorbis.wasm (~70 KB gzipped)
```

**Implementation:**

```rust
// mpc-wasm-main/Cargo.toml
[dependencies]
core-playback = { 
  path = "../core-playback", 
  default-features = false,
  features = ["decoder-mp3", "decoder-aac"] # Essential formats
}

// mpc-wasm-flac/Cargo.toml
[dependencies]
core-playback = { 
  path = "../core-playback", 
  default-features = false,
  features = ["decoder-flac"]
}
```

```typescript
// TypeScript lazy loading
const decoderModules = {
  flac: () => import('./mpc-wasm-flac.js'),
  vorbis: () => import('./mpc-wasm-vorbis.js'),
  opus: () => import('./mpc-wasm-opus.js'),
};

async function ensureDecoder(format: string) {
  if (!loadedDecoders.has(format) && decoderModules[format]) {
    const { default: init } = await decoderModules[format]();
    await init();
    loadedDecoders.add(format);
  }
}

// Before playing
await ensureDecoder(track.format);
player.play(track);
```

**Benefits:**
- ✅ Fast initial load (350 KB vs 700 KB)
- ✅ Most users never need all formats
- ✅ Progressive enhancement
- ✅ Reasonable complexity

---

## 📦 Final Bundle Structure

### **Production Build:**

```
dist/
├── mpc-wasm-main.wasm          (900 KB → 350 KB gzipped)
│   └── Essential: runtime, library, auth, playback (MP3+AAC)
│
├── mpc-wasm-worker.wasm        (880 KB → 300 KB gzipped)
│   └── Background: runtime, library, auth, sync, metadata
│
└── optional-decoders/
    ├── mpc-wasm-flac.wasm      (200 KB → 80 KB gzipped)
    ├── mpc-wasm-vorbis.wasm    (180 KB → 70 KB gzipped)
    └── mpc-wasm-opus.wasm      (150 KB → 60 KB gzipped)
```

**Loading strategy:**
1. **Initial:** Load main + worker (650 KB gzipped total)
2. **App ready:** User can play MP3/AAC immediately
3. **On demand:** Load additional decoders as needed
4. **Max total:** 1,160 KB gzipped (if user uses all formats)

---

## 🏗️ Implementation Checklist

### **Phase 1: Create Main Bundle**
- [ ] Create `mpc-wasm-main/` directory
- [ ] Configure with essential decoders only
- [ ] Build and measure size
- [ ] Test MP3 and AAC playback

### **Phase 2: Create Worker Bundle**
- [ ] Create `mpc-wasm-worker/` directory
- [ ] Include sync + metadata
- [ ] Build and measure size
- [ ] Test sync operations

### **Phase 3: Optional Decoder Bundles**
- [ ] Create `mpc-wasm-flac/` (FLAC decoder only)
- [ ] Create `mpc-wasm-vorbis/` (Vorbis decoder only)
- [ ] Create `mpc-wasm-opus/` (Opus decoder only)
- [ ] Implement lazy loading logic

### **Phase 4: Communication**
- [ ] Implement postMessage bridge
- [ ] Forward events from worker to main
- [ ] Test cross-thread communication

### **Phase 5: Optimize**
- [ ] Measure actual gzipped sizes
- [ ] Profile loading performance
- [ ] Add service worker for caching

---

## 📊 Performance Targets

| Metric | Target | Current (Monolithic) | 2-Bundle Split |
|--------|--------|----------------------|----------------|
| Initial download | < 500 KB | ~600 KB | ✅ ~350 KB |
| Time to interactive | < 2s (3G) | ~3s | ✅ ~1.5s |
| Full load (all formats) | < 1.5 MB | ~600 KB | ~1,160 KB |
| Formats supported | All | All | All (lazy) |

---

## 🎯 Summary

**Recommended Architecture:**
1. **Main Thread:** Essential decoders (MP3 + AAC) + UI → 350 KB gzipped
2. **Worker Thread:** Sync + Metadata → 300 KB gzipped  
3. **Optional:** FLAC, Vorbis, Opus → Lazy load on demand

**Total Initial Load:** 650 KB gzipped (vs 700 KB monolithic)

**Key Decision:**
- ✅ **Keep core-playback in main thread** (Web Audio API requirement)
- ✅ **Use feature flags** to split decoders
- ✅ **Lazy load** uncommon formats
- ✅ **Accept duplication** of core modules (~200 KB gzipped)

**Result:**
- Fast initial load for most users
- Progressive enhancement for advanced formats
- Clean separation of concerns (UI vs background)
