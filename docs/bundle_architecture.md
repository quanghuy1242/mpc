# Core Service Architecture - Phase 6 Implementation Plan

**Document Version**: 1.0  
**Date**: November 8, 2025  
**Status**: Planning Phase  

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
              │  CoreServiceMain (2-3MB)     │
              │  • Auth UI flows             │
              │  • Event subscription        │
              │  • Worker coordination       │
              │  • UI state management       │
              └──────┬──────────────┬────────┘
                     │              │
          postMessage│              │postMessage
          (queries)  │              │(audio data)
                     │              │
      ┌──────────────▼─┐        ┌──▼─────────────┐
      │ Web Worker 1-N │        │  Audio Worker  │
      │ CoreServiceWork│        │ CoreServiceAud │
      │ (4-5MB)        │        │ (2-3MB)        │
      │ • core-library │        │ • Decode       │
      │ • core-sync    │        │ • Streaming    │
      │ • core-metadata│        │ • Ring buffer  │
      │ • Database     │        │                │
      └────────────────┘        └────────┬───────┘
                                         │
                                         ▼
                              ┌──────────────────┐
                              │  AudioWorklet    │
                              │  (Playback)      │
                              └──────────────────┘
```

**Threading Model**:
- **Main Thread (UI)**: Single-threaded, `!Send`
  - CoreService API calls
  - Event subscription and rendering
  - User interaction
  - Worker pool management
  - Uses `Rc<RefCell<T>>` for state

- **Web Worker Pool** (2-4 workers): Separate JavaScript contexts
  - Each loads CoreServiceWorker WASM (3-4MB)
  - File listing, metadata extraction, DB writes
  - Task queue with load balancing
  - Communication via `postMessage`

- **Audio Worker**: Dedicated context
  - Loads CoreServiceAudio WASM (2-3MB)
  - Audio decoding (Symphonia WASM)
  - Ring buffer management
  - SharedArrayBuffer for zero-copy samples

- **AudioWorklet**: High-priority audio thread
  - Reads from SharedArrayBuffer
  - PCM output to Web Audio API
  - Real-time guarantees

**Bundle Breakdown**:
- **Main Bundle** (2-3MB): core-auth (OAuth flows), event bus, UI coordination
- **Worker Bundle** (4-5MB): core-sync, core-metadata, **core-library** (complete database)
- **Audio Bundle** (2-3MB): core-playback, Symphonia decoder

**Critical Design Decision**: core-library MUST be entirely in the worker bundle because:
- Database connections cannot be shared across JavaScript contexts
- IndexedDB/OPFS access is per-context
- Sync operations need to write to database
- Main thread queries database via postMessage to worker

**Characteristics**:
- ✅ Optimal bundle sizes (8-10MB total vs 24MB monolithic)
- ✅ Main thread stays responsive
- ✅ Parallel processing via workers
- ✅ Zero-copy audio via SharedArrayBuffer
- ⚠️ Serialization overhead for postMessage
- ⚠️ More complex than native (multi-context coordination)
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
