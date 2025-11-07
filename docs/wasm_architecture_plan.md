# WASM Architecture Status & Plan

_Last updated: 2025-11-07_

## 1. Vision

The Music Platform Core runs the **same Rust business logic** on every target.  
Platform-specific details (filesystem, database, networking, secure storage, etc.) are provided by bridge crates that implement `bridge-traits`.

```
Native Host ───────┐       WASM Host ─────────┐
bridge-desktop     │       bridge-wasm        │
  * tokio HTTP     │       * fetch API HTTP   │
  * std::fs        │  ◀──▶  * IndexedDB FS    │
  * sqlx/sqlite    │ traits * sql.js/IndexedDB│
  * OS secure store│       * Web Crypto/etc.  │
───────────────┬───┘                         │
               │                             │
         bridge-traits (platform contracts)  │
───────────────┴─────────────────────────────┘
            UNIVERSAL CORE (core-*, provider-*)
```

## 2. Current Status

| Area | Native | WASM | Notes |
| --- | --- | --- | --- |
| `bridge-traits` | ✅ | ✅ | Conditional `Send/Sync` handled via `PlatformSend*` markers. |
| Async runtime (`core-async`) | ✅ | ✅ | No direct Tokio dependencies downstream. |
| Filesystem abstraction | ✅ | ✅ | `WasmFileSystem` (IndexedDB) ships via `bridge-wasm`; desktop hosts use `bridge-desktop`. |
| Database abstraction | ✅ | ✅ | All repositories and the query service rely on `DatabaseAdapter`; wasm uses `WasmDbAdapter`. |
| HTTP/S Transport | ✅ | ✅ | `bridge-wasm::WasmHttpClient` wraps `fetch`, native builds continue to use `reqwest`. |
| Secure storage | ✅ | ✅ | `WasmSecureStore` + settings store implemented; desktop secure storage lives in `bridge-desktop`. |
| Metadata extraction I/O | ✅ | ✅ | Extractor consumes byte buffers, and `core-sync` reads via `FileSystemAccess`. |
| Testing | ⚠️ | ⚠️ | Browser-based filesystem tests run under `wasm-bindgen-test`; broader wasm CI still pending. |

Legend: ✅ done * ⚙️ in progress * ❌ not started

## 3. Recent Progress

1. **Trait compatibility** – All bridge traits now use conditional bounds, so wasm builds no longer fail on `Send + Sync`.
2. **Filesystem** – IndexedDB-backed `WasmFileSystem` added with async trait implementation.
3. **Database**  
   - `DatabaseAdapter` trait finalized in `bridge-traits`.  
   - Native adapter (`SqliteAdapter`) refactored to match trait.  
   - WASM adapter skeleton (`WasmDbAdapter`) added, delegating to host JS (sql.js / IndexedDB).  
   - `SqliteTrackRepository` now consumes `dyn DatabaseAdapter`, enabling wasm injection.
4. **Build hygiene** - `cargo check` (native) passes; wasm targets build for bridge crates (`bridge-traits`, `bridge-wasm`, `core-async`). Workspace-wide wasm build still blocked by native crates pulling `tokio`/`mio`.
5. **Secure host wiring** - `bridge-wasm::build_wasm_bridges` bundles `WasmHttpClient`, `WasmFileSystem`, `WasmDbAdapter`, and secure storage, while `core-service::bootstrap_wasm` wires the bundle into the core behind the `wasm` feature.

## 4. Outstanding Work

1. **Tooling & CI**
   - [ ] Add wasm test targets/smoke tests for key `core-*` crates (e.g., `wasm-bindgen-test` in headless Chrome/Node).
   - [ ] Wire `cargo check --workspace --target wasm32-unknown-unknown` into CI, skipping native-only crates via features.

2. **Host Guidance**
   - [ ] Publish reference JS glue for `bridgeWasmDb.*` (sql.js + IndexedDB persistence) and document bundler expectations.
   - [ ] Expand docs with end-to-end wasm bootstrap examples showing how to call `core_service::bootstrap_wasm` and handle logging/progress callbacks.

## 5. Host Responsibilities
## 5. Host Responsibilities

### Desktop / Native
- Instantiate `SqliteAdapter` (or other native DB implementation).
- Provide implementations for filesystem, HTTP, secure storage, etc., via `bridge-desktop`.
- Inject adapters into core services during bootstrap.

### Web / WASM
- Provide JS glue:
  - `bridgeWasmDb.*` - wraps sql.js and persists to IndexedDB.
  - `bridgeWasmFs.*` - already in Rust (`WasmFileSystem`) but must be initialized per app.
  - HTTP (`fetch`) adapter that satisfies `HttpClient`.
  - Secure storage (localStorage/WebCrypto).
- Configure bundler to expose these APIs before initializing the Rust core.
- Call `core_service::bootstrap_wasm(WasmBridgeConfig::new("app-namespace"))` so the Rust core receives the assembled `HttpClient`, filesystem, database adapter, and storage bridges. The generated `CoreService` handle exposes `dependencies()` if the host needs to hold onto the adapters (e.g., to wire download streams or UI loggers).

## 6. Next Milestones

1. Land wasm CI (checks + tests) for the full workspace.
2. Ship host-facing documentation + JS glue samples covering `build_wasm_bridges` and `core_service::bootstrap_wasm`.
3. Track browser perf/quotas (IndexedDB limits, fetch constraints) and document recommended fallbacks.

With those pieces in place, every `core-*` crate can compile and run under `wasm32-unknown-unknown`, fulfilling the "universal core" promise.
