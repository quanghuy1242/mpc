# WASM Support TODO

_Generated: 2025-11-07_

This list captures the remaining work required for full `wasm32-unknown-unknown` support across the Music Platform Core.

## 1. HTTP Transport

- **Status (2025-11-07):** ✅ Complete. `bridge-wasm::http::WasmHttpClient` wraps the browser `fetch` API (handles headers, binary bodies, optional timeouts, download streams) and `bridge-wasm::bootstrap::build_wasm_bridges` now returns it as part of a ready-to-use wasm bridge stack, so hosts can inject `Arc<WasmHttpClient>` without compiling the native `reqwest` shim.
- **Action Items:** None. Any remaining host-specific wiring is tracked under Task 5 (Secure Host Wiring).

## 2. Secure Storage

- **Status (2025-11-07):** ✅ Complete. `bridge-wasm::storage::{WasmSecureStore, WasmSettingsStore}` implement the secure/settings store traits (AES-256-GCM + transactional localStorage) and `build_wasm_bridges` wires them automatically alongside filesystem, database, and HTTP adapters.
- **Action Items:** None. Host bootstrap guidance lives in `bridge-wasm/src/bootstrap.rs` and Task 5 covers broader wiring/diagnostics.

## 3. Database Adapter Rollout (Task 4.1)

- **Status (2025-11-07):** ✅ Complete. `core-library/src/query.rs` now routes every query (`query_tracks`, `stream_tracks`, `query_albums`, `search`, `get_track_details`) through `dyn DatabaseAdapter` and shared repository helpers, so no `SqlitePool` or `#[cfg(not(target_arch = "wasm32"))]` guards remain. `core-service/src/lib.rs` exposes a real `CoreService` façade plus `bootstrap_wasm` that calls `bridge-wasm::build_wasm_bridges`, while `core-service/Cargo.toml` gates native-only crates (`bridge-desktop`, provider connectors) behind the `desktop-shims` feature so wasm builds inject `WasmDbAdapter`, filesystem, HTTP, and storage adapters without pulling in `tokio`/`libsqlite3-sys`.
- **Action Items:** None. Future wiring guidance now lives alongside the wasm bootstrap helper.

## 4. Metadata Extraction I/O

- **Status (2025-11-07):** ✅ Complete. `core-metadata::MetadataExtractor` now exposes `extract_from_bytes` and `extract_from_filesystem`, so the lofty pipeline always works on in-memory buffers regardless of platform. `core-sync::MetadataProcessor` downloads via `FileSystemAccess`, reads bytes through the same bridge, and feeds them straight into the extractor (no direct `core_async::fs` calls or platform-specific paths). Wasm builds therefore use `bridge-wasm::WasmFileSystem` for both temp storage and read-back, exactly matching native behavior.
- **Action Items:** None.

## 5. Secure Host Wiring

- **Status (2025-11-07):** ✅ Complete. `bridge-wasm::bootstrap` now exposes `build_wasm_bridges`/`WasmBridgeConfig`, bundling `WasmHttpClient`, `WasmFileSystem`, `WasmDbAdapter`, `WasmSecureStore`, and settings storage. `core-service` re-exports that config and ships `bootstrap_wasm`, so wasm hosts can initialize the full bridge stack with a single async call, while Cargo feature gating keeps native-only crates (`bridge-desktop`, provider connectors) behind `desktop-shims`.
- **Action Items:** None.

## 6. Bridge Usage Audit

- **Status (2025-11-07):** ✅ Complete. All previously native-only entry points now go through bridge traits (`core-library` queries, `core-sync` metadata processor, `core-service` bootstrap) and compile for `wasm32-unknown-unknown` without `#[cfg(not(...))]` exclusions. `bridge-wasm/tests/filesystem_tests.rs` has been re-enabled under `wasm-bindgen-test`, giving us a browser-side integration suite that exercises the IndexedDB filesystem trait end-to-end, and `cargo check --target wasm32-unknown-unknown` now succeeds for `core-library`, `core-metadata`, `core-sync`, and `core-service`.
- **Action Items:** None (future regressions should add wasm checks/tests to CI, but the audit work is finished).

## 6. Testing & CI

- [ ] Add wasm tests for core crates using `wasm-bindgen-test` (similar to `bridge-wasm` filesystem tests).
- [ ] Gate workspace members so `cargo check --workspace --target wasm32-unknown-unknown` excludes native-only crates (or gate native dependencies behind `cfg`).
- [ ] Document the headless WebDriver setup (Edge/Chrome) plus Node fallback for wasm tests.

Completing these tasks will bring the entire core (`core-sync`, `core-metadata`, `core-playback`, `core-library`, etc.) to true platform parity across native and WebAssembly targets.
