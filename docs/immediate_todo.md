# Immediate To-Do Tasks for Implemented Code (Phases 0-4)

This document lists critical architectural flaws and bugs found in the *already implemented* work that must be fixed to ensure the foundation of the project is sound, cross-platform, and correct.

---

### 1. Implement Runtime-Agnostic Async Abstraction (Highest Priority)

**Issue:** The existing async code in `core-sync`, `core-auth`, and other modules has hard dependencies on the `tokio` runtime. This is a critical architectural flaw that prevents the library from compiling or running on WebAssembly.

**Task:**
- **Create an Abstraction Layer:** Create a new internal module that provides a unified, runtime-agnostic async API.
- **Use Conditional Compilation:** This module must use `#[cfg]` attributes to export the correct async primitives for the target platform (e.g., `tokio` for native, `wasm-bindgen-futures` for Wasm).
- **Refactor Core Crates:** Update all existing modules that perform async operations to use the new abstraction layer instead of calling `tokio` directly.

---

### 2. Implement Incremental Sync Logic (Critical)

**File:** `core-sync/src/coordinator.rs`

**Issue:** The `SyncCoordinator`, which was implemented as part of Phase 3, is critically flawed. It was marked as complete but lacks the logic to perform an incremental sync, forcing a full, slow, and expensive re-scan on every run.

**Task:**
- Modify the existing `execute_sync` function to correctly handle the `SyncType::Incremental` case.
- When the sync type is incremental, the function must call the `provider.get_changes()` method and correctly process the added, modified, and deleted files.

---

### 3. Refactor `execute_sync` for Clarity

**File:** `core-sync/src/coordinator.rs`

**Issue:** The existing `execute_sync` function is overly long and complex, making it difficult to maintain and to correctly implement the critical incremental sync logic.

**Task:**
- Break down the `execute_sync` function into smaller, more focused functions for each phase of the synchronization process (e.g., `discover_files`, `process_scan_queue`, `resolve_conflicts`).