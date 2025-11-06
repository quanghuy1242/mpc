# Immediate To-Do Tasks for Implemented Code (Phases 0-4)

This document lists critical architectural flaws and bugs found in the *already implemented* work that must be fixed to ensure the foundation of the project is sound, cross-platform, and correct. The tasks below are expanded with specific implementation details and sub-tasks.

---

### 1. Implement Runtime-Agnostic Async Abstraction (Highest Priority)

**Issue:** The existing async code in `core-sync`, `core-auth`, and other modules has hard dependencies on the `tokio` runtime. This is a critical architectural flaw that prevents the library from compiling or running on WebAssembly (Wasm) and violates the cross-platform principles laid out in `core_architecture.md`.

**Required Task & Scope:** Introduce a runtime-agnostic async abstraction layer to decouple the core library from `tokio`. This will involve creating a new internal crate and refactoring all `core-*` crates to use it.

**Sub-tasks:**

1.  **Create a new `core-async` Crate:**
    *   This crate will be a private, internal dependency for other `core-*` and `provider-*` crates.
    *   Its purpose is to abstract away runtime-specific APIs. It will only re-export async primitives.

2.  **Implement Conditional Exports using `#[cfg]`:**
    *   **For native targets (`#[cfg(not(target_arch = "wasm32"))]`):**
        *   Re-export `tokio` types and functions.
        *   Example `core-async/src/lib.rs`:
            ```rust
            pub use tokio::{main, test}; // For application entry-points and tests
            pub mod task { pub use tokio::task::{spawn, spawn_blocking, JoinHandle}; }
            pub mod time { pub use tokio::time::{sleep, Duration, Instant}; }
            pub mod sync { pub use tokio::sync::{Mutex, MutexGuard, RwLock, broadcast, mpsc}; }
            ```
    *   **For Wasm (`#[cfg(target_arch = "wasm32")]`):**
        *   Export Wasm-compatible equivalents.
        *   Example `core-async/src/lib.rs`:
            ```rust
            // No `main` macro needed for wasm libraries
            pub use wasm_bindgen_test::wasm_bindgen_test as test;
            pub mod task {
                // `JoinHandle` might need a custom wrapper for wasm
                pub fn spawn<F>(future: F) where F: std::future::Future + 'static, F::Output: 'static {
                    wasm_bindgen_futures::spawn_local(future);
                }
            }
            pub mod time {
                pub use std::time::Duration;
                pub async fn sleep(duration: Duration) { gloo_timers::future::sleep(duration).await }
            }
            pub mod sync {
                // Use non-tokio versions, e.g., futures::lock::Mutex
                pub use futures::lock::Mutex;
            }
            ```

3.  **Refactor All Core and Provider Crates:**
    *   Audit every `core-*` and `provider-*` crate for direct `tokio` dependencies in their `Cargo.toml` and usages in the code.
    *   Replace the `tokio` dependency with the new `core-async` crate.
    *   Replace all direct calls (e.g., `tokio::spawn`, `tokio::sync::Mutex`) with their `core_async` counterparts (e.g., `core_async::task::spawn`, `core_async::sync::Mutex`).

4.  **Use the `Clock` Abstraction for Time:**
    *   The `bridge-traits` crate already defines a `Clock` trait. Enforce its usage for obtaining the current time system-wide to improve testability, instead of calling `Instant::now()` directly. The `SystemClock` implementation can be provided by `bridge-traits`.

---

### 2. Implement Incremental Sync Logic (Critical)

**File:** `core-sync/src/coordinator.rs`

**Issue:** The `SyncCoordinator`, marked as complete in `TASK-304`, is critically flawed. It lacks the logic to perform an incremental sync, forcing a full, slow, and expensive re-scan on every run instead of efficiently fetching only what has changed.

**Required Task & Scope:** Fully implement the incremental sync logic within the `execute_sync` function (or its refactored equivalent) to correctly process added, modified, and deleted files provided by the `StorageProvider`.

**Sub-tasks:**

1.  **Differentiate Sync Types in `execute_sync`:**
    *   The function must check the `sync_type` of the `SyncJob`.
    *   The `SyncType::Incremental` path will execute a different discovery logic than `SyncType::Full`.

2.  **Fetch the Change Cursor:**
    *   For an incremental sync, query the `SyncJobRepository` to find the `cursor` from the last successfully completed sync job for the specific provider.
    *   If no cursor exists, the coordinator should log a warning and automatically escalate to a `Full` sync for this job.

3.  **Call `StorageProvider::get_changes`:**
    *   With the retrieved cursor, call `provider.get_changes(Some(cursor))`. This will return a stream of change events from the cloud API.

4.  **Process the Stream of Changes:**
    *   Iterate through the change events from the provider.
    *   **For Added/Modified Files:**
        *   Create a `WorkItem` from the file metadata.
        *   Enqueue the `WorkItem` into the `ScanQueue` for processing (metadata extraction, artwork, etc.). Set a `High` priority to process changes first.
    *   **For Deleted Files:**
        *   Use the `ConflictResolver` to process the deletion. Call `conflict_resolver.handle_deletion(remote_id)`. This will mark the corresponding track and related data as deleted in the local library database.

5.  **Update Progress and Persist the New Cursor:**
    *   Continuously update the `SyncProgress` as changes are processed.
    *   After the provider's change stream is fully consumed, retrieve the `new_cursor` from the final response.
    *   Persist this `new_cursor` to the *current* `SyncJob` record in the database. This is essential for the *next* incremental sync to work.

---

### 3. Refactor `execute_sync` for Clarity and Maintainability

**File:** `core-sync/src/coordinator.rs`

**Issue:** The `execute_sync` function is overly long and complex, mixing concerns of discovery, processing, and conflict resolution. This monolithic structure makes it difficult to read, test, and was a contributing factor to the failed implementation of incremental sync.

**Required Task & Scope:** Break down `execute_sync` into a set of smaller, cohesive, private `async` functions, each responsible for a distinct phase of the sync process. The main `execute_sync` function will become a high-level orchestrator.

**Sub-tasks:**

1.  **Orchestrator Role for `execute_sync`:**
    *   The refactored `execute_sync` will manage the `SyncJob` lifecycle (start, progress updates, completion, or failure) and orchestrate calls to the phase-specific helper functions. It should contain high-level error handling.

2.  **Create a `discovery_phase` Helper:**
    *   Signature: `async fn discovery_phase(&self, job: &SyncJob, provider: &Arc<dyn StorageProvider>) -> Result<String, SyncError>`
    *   This function will contain the `match job.sync_type` logic.
    *   For a **full sync**, it lists all media from the provider and enqueues them.
    *   For an **incremental sync**, it gets and processes the change set (as defined in Task 2).
    *   It returns the `new_cursor` to be persisted.

3.  **Create a `processing_phase` Helper:**
    *   Signature: `async fn processing_phase(&self, job: &mut SyncJob, job_control: &CancellationToken) -> Result<SyncJobStats, SyncError>`
    *   This function contains the main loop that dequeues items from the `ScanQueue`.
    *   It spawns concurrent tasks (respecting `max_concurrent` limits) to process each `WorkItem`. Processing involves calling the `MetadataProcessor` to handle file download, tag extraction, and persistence.
    *   It listens for cancellation signals via `job_control`.
    *   It collects and returns the final `SyncJobStats` (items added, updated, failed).

4.  **Create a `conflict_resolution_phase` Helper:**
    *   Signature: `async fn conflict_resolution_phase(&self, job: &SyncJob) -> Result<(), SyncError>`
    *   This function runs after the main processing is complete.
    *   It uses the `ConflictResolutionOrchestrator` to perform tasks like identifying duplicates created during the sync and handling any other post-sync cleanup.

5.  **Proposed `execute_sync` Structure:**
    *   The final structure should resemble this clear, top-down workflow:
        ```rust
        // Pseudo-code for the refactored function
        async fn execute_sync(&self, job_id: SyncJobId, cancellation_token: CancellationToken) {
            // 1. Fetch and start the job
            let mut job = self.job_repo.find_by_id(job_id).await.unwrap();
            job.start();
            self.job_repo.update(&job).await;
            self.events.send(SyncEvent::Started(job.summary()));

            // 2. Execute phases
            let result = async {
                let provider = self.providers.get(&job.provider_id).unwrap();

                // Phase 1: Discover files
                let new_cursor = self.discovery_phase(&job, provider).await?;
                job.update_cursor(new_cursor);
                self.job_repo.update(&job).await?;

                // Phase 2: Process work queue
                let stats = self.processing_phase(&mut job, &cancellation_token).await?;

                // Phase 3: Resolve conflicts (if any)
                self.conflict_resolution_phase(&job).await?;

                Ok(stats)
            }.await;

            // 3. Finalize the job
            match result {
                Ok(stats) => job.complete(stats),
                Err(e) => job.fail(e.to_string()),
            }
            if cancellation_token.is_cancelled() {
                job.cancel();
            }
            self.job_repo.update(&job).await;
            self.events.send(SyncEvent::Completed(job.summary()));
        }
        ```
