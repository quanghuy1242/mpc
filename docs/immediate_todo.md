# Immediate To-Do Tasks for Implemented Code (Phases 0-4)

This document lists critical architectural flaws and bugs found in the *already implemented* work that must be fixed to ensure the foundation of the project is sound, cross-platform, and correct. The tasks below are expanded with specific implementation details and sub-tasks.

---

### 1. Implement Runtime-Agnostic Async Abstraction (✅ Completed)

Resolved via the `core-async` crate and cross-crate refactors. All production + test code now depends on `core_async` re-exports (or the bridge `Clock`), so no `tokio` runtime leaks remain in the core workspace. No further action required unless new crates bypass the abstraction.

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

---

### 4. Ensure Wasm Compatibility for Core Components (✅ Completed)

Database, filesystem, HTTP, secure storage, and metadata I/O are now abstracted via bridge traits with wasm implementations (`bridge-wasm::WasmDbAdapter`, `WasmFileSystem`, `WasmHttpClient`, `WasmSecureStore`, etc.), and `core-service::bootstrap_wasm` wires them together. No further action needed for this task.

### 5. Fix WASM Trait Compatibility Issues (✅ Completed)

`bridge-traits` now uses `PlatformSend` / `PlatformSendSync`, allowing wasm bridge implementations to compile without `Send + Sync` bounds. This task is fully resolved.

---

### 6. Restore WASM Runtime Parity for `core_async::{task,runtime}` (Critical)

**Files:** `core-async/src/task.rs`, `core-async/src/runtime.rs`, `core-runtime/src/events.rs:1027`, `core-metadata/src/enrichment_job.rs:470`, `core-library/src/db.rs:407`, `core-runtime/src/logging.rs:456`

**Issue:** On wasm builds `core_async::task::spawn` drops the `JoinHandle`, `spawn_blocking` panics (`task.rs:135`), and `runtime::block_on` simply spawns a future and returns (`runtime.rs:31`). Multiple crates rely on awaiting `JoinHandle`s or on synchronous `block_on` semantics (tests, metadata enrichment, logging fan-out). Today these sites will either fail to compile on wasm (because `spawn` returns `()` ) or will silently skip work (logging sinks will never flush before returning).

**Required Task & Scope:** Provide feature-parity abstractions so the public API behaves identically on wasm. Introduce a `LocalJoinHandle` (or similar) that stores the output and can be `.await`ed, implement a `spawn_blocking` strategy (Web Workers or cooperative chunking), and make `runtime::block_on` actually wait for completion (e.g., `futures::executor::LocalPool` or a JS `Promise` bridge). Audit all call sites in `core-runtime`, `core-sync`, `core-metadata`, and tests to ensure they compile on wasm.

**Proposed Solution:** Wrap `spawn` in a tiny executor that pushes the future into a `LocalFutureHandle` storing completion via `futures::channel::oneshot`. Returning that handle keeps the same ergonomic surface as Tokio's `JoinHandle` for our codebase, but internally it remains single-thread friendly (no `Send` bounds, minimal ref-counting). For `block_on`, embed a `futures::executor::LocalPool` (or `wasm_bindgen_futures::future_to_promise`) that pumps the event loop until the provided future resolves, ensuring logging/tests can still rely on synchronous completion without pretending we have a multithreaded runtime. `spawn_blocking` should explicitly embrace wasm constraints—either provide a cooperative chunker that yields frequently or, when available, offload to Web Workers. The intent is parity for **our** APIs, not a 1:1 Tokio clone.

**Sub-tasks:**

1. Implement a wasm `JoinHandle` that polls the future to completion and exposes `await`, matching the tokio API signature to keep downstream code unchanged.
2. Replace the current wasm `block_on` stub with an executor-backed implementation so helpers like `runtime::block_on` in `core-runtime::logging` retain their semantics.
3. Decide on a wasm-safe replacement for `spawn_blocking` (web worker pool or documented `unavailable` guard) and gate any existing usages accordingly.

### 7. Implement WASM Synchronization Primitives Used by `core-sync` (Critical)

**Files:** `core-async/src/sync.rs:70-433`, `core-sync/src/scan_queue.rs:561-615`, `core-sync/src/coordinator.rs:75-1209`

**Issue:** The wasm versions of `Semaphore`, `Barrier`, and `watch` either panic or are stubs (`sync.rs:381-433`). `Notify::notified` simply spin-waits via `yield_now`. `CancellationToken` only offers `is_cancelled` and cannot be awaited. `core-sync::ScanQueue` calls `Semaphore::new`/`acquire().await` to enforce bounded concurrency (`scan_queue.rs:574`), so the wasm build will panic immediately. Any future use of `watch`/`Barrier` will also panic despite the API claiming platform agnosticism.

**Required Task & Scope:** Provide real single-threaded implementations backed by `Rc<RefCell<_>>` + `Waker` queues (or reuse crates such as `async-channel`/`async-broadcast`). At minimum, `Semaphore::new`, `acquire`, and `SemaphorePermit` must work on wasm, as must `Notify::notified` without busy loops, and `CancellationToken` must expose an async wait API that `core-sync` can hook into for cooperative cancellation.

**Proposed Solution:** Mirror the semantics we rely on, but implement them as first-class single-threaded primitives. `Semaphore::acquire` can live on top of an `Rc<RefCell<State>>` that tracks permits and a queue of `Waker`s; releasing a permit simply wakes the next waiter so we never spin. `Notify`/`CancellationToken` expose futures that resolve once triggered, pushing cooperative cancellation instead of threads. For `watch`, storing the latest value plus subscriber wakers keeps the API compatible while acknowledging wasm's non-`Send` world. Matching behaviors—not Tokio internals—keeps perf tight (no locks) and responsive UIs (bounded work per tick).

**Sub-tasks:**

1. Re-implement wasm `Semaphore` with an internal queue of waiters and unit tests that cover contention (mirroring `tokio::sync::Semaphore` behavior).
2. Replace the spin-loop `Notify` with an awaitable future (store `Waker`s) so high-frequency notifications do not peg the main thread.
3. Flesh out the wasm `watch` channel (send/recv, `borrow` semantics) or gate its export until parity exists; document and add tests that cover lag/back-pressure.

### 8. Harden WASM Broadcast/Event Bus Semantics (High)

**Files:** `core-async/src/sync.rs:452-676`, `core-runtime/src/events.rs`

**Issue:** The custom wasm `broadcast` channel stores messages in `Rc<RefCell<VecDeque<_>>>` and `Receiver::recv` busy-loops with `yield_now` (`sync.rs:606`). There are no wakers, no back-pressure notifications, and the implementation is not `Send + Sync`, yet `core-runtime::events` advertises thread safety and relies on `RecvError::Lagged` semantics. Under wasm this results in high CPU usage and unreliable delivery for the `EventBus` (core to runtime telemetry).

**Required Task & Scope:** Either port tokio's broadcast semantics to wasm (e.g., adapt `async-broadcast`) or wrap an existing single-threaded broadcast crate that supports awaitable receivers. Ensure `EventBus` invariants (lag detection, `receiver_count`, graceful shutdown) still hold, and add wasm-specific tests that cover concurrent publishers/subscribers without spinning.

**Proposed Solution:** Replace the hand-written queue with a wasm-appropriate broadcast primitive (either a lightweight port of our own or a dependency such as `async-broadcast`) so receivers await waker-driven notifications, not spin loops. Each receiver maintains an index into a ring buffer, enabling lag detection without heap churn, and senders wake lagging receivers explicitly. When the sender closes, all receivers resolve so shutdown paths stay deterministic. Native builds keep Tokio; wasm gets this single-thread-optimized variant under the same API surface, favoring predictable latency over perfect feature mimicry.

**Sub-tasks:**

1. Introduce wake-based notification for receivers instead of manual `yield_now` loops.
2. Guarantee `Receiver::recv` resolves when the channel closes so `EventBus` shutdown logic does not hang.
3. Update the `core-runtime::events` docs/tests to reflect the actual thread-safety guarantees on wasm once the new primitive lands.

### 9. Expose WASM Filesystem via `core_async::fs` (High)

**Files:** `core-async/src/fs.rs:15`, `bridge-wasm/src/filesystem.rs`, `bridge-wasm/src/bootstrap.rs`

**Issue:** `core_async::fs` is completely disabled on wasm (`compile_error!` guard), forcing wasm code to bypass the crate and talk to `bridge-wasm::WasmFileSystem` directly. This contradicts the stated goal of “core-async supports a shared codebase for native + web” and means modules like `core-runtime::config` or provider code must resort to `#[cfg]` branching with ad-hoc types.

**Required Task & Scope:** Add a wasm implementation for `core_async::fs` that delegates to the IndexedDB-backed `WasmFileSystem` (or another bridge trait). Wire this through `core-service::bootstrap_wasm` so downstream crates keep depending on `core_async::fs::*` regardless of target. Provide integration tests that exercise `read_dir`, `read_to_string`, `write`, and removal APIs in headless wasm tests.

**Proposed Solution:** Create a thin adapter inside `core_async::fs` that, when compiled for wasm, internally holds an `Arc<dyn FileSystemAccess>` (sourced from `bridge-wasm::WasmFileSystem`). The adapter should expose the same async signatures (`read`, `write`, `read_dir`, `remove_file`, etc.) but document IndexedDB-specific characteristics (chunked writes, quota errors, eventual consistency). During WASM bootstrap we can inject a singleton filesystem into `core_async` (or expose a setter) to avoid repeated initialization, and add wasm-bindgen tests to verify parity with the subset of operations we actually rely on. The goal is API compatibility for the workspace while embracing the browser-backed storage model instead of forcing POSIX semantics.

**Sub-tasks:**

1. Define a thin adapter around `bridge_traits::storage::FileSystemAccess` and expose it from `core_async::fs` when `target_arch = "wasm32"`.
2. Ensure async file handles (`File`, `OpenOptions`) expose the same API surface expected by the rest of the workspace (or document unsupported operations).
3. Update existing modules/tests that currently `cfg(not(wasm32))` their filesystem access so they can run unchanged on wasm.
