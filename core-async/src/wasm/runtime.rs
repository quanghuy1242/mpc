//! WASM-specific runtime implementation.
//!
//! Provides runtime utilities for WASM environments with careful consideration
//! of browser constraints.

use std::future::Future;

/// Attempts to run a future in WASM environment.
///
/// # Critical Limitation
///
/// **WASM cannot truly block**. In a browser environment, blocking the main thread
/// would freeze the entire UI and prevent the event loop from processing async
/// operations like timers, network requests, etc.
///
/// This function uses `futures::executor::LocalPool::run_until()` which works for
/// **immediate futures only** (futures that are already ready or can complete without
/// awaiting browser APIs). For futures that depend on timers, network, or any browser
/// API, this will **hang indefinitely**.
///
/// # When This Works
///
/// - Futures that are immediately ready (`future::ready()`)
/// - Pure computation without `.await` on external resources
/// - Already-completed channels/oneshots
///
/// # When This Hangs
///
/// - Any use of `time::sleep()` or `time::timeout()`
/// - Network requests via `fetch`
/// - `spawn().await` (waiting for spawned tasks)
/// - Any browser API that returns a Promise
///
/// # Recommended Alternatives
///
/// Instead of `block_on`, use:
/// - `spawn()` to run futures asynchronously
/// - Keep your functions `async` and use `.await`
/// - Use `wasm_bindgen_futures::spawn_local()` for fire-and-forget
///
/// # Examples
///
/// ```rust,no_run
/// use core_async::runtime;
/// use futures::future;
///
/// // This works - immediate future
/// let result = runtime::block_on(future::ready(42));
/// assert_eq!(result, 42);
///
/// // This HANGS - depends on browser event loop
/// // use core_async::time::{sleep, Duration};
/// // runtime::block_on(async {
/// //     sleep(Duration::from_millis(10)).await; // HANGS!
/// // });
/// ```
pub fn block_on<F>(future: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    use futures::executor::LocalPool;
    
    // WARNING: This will hang if the future depends on the browser event loop!
    // See function documentation for details.
    let mut pool = LocalPool::new();
    pool.run_until(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_block_on_immediate_futures() {
        use futures::future;
        
        // Only test with immediate futures that don't need event loop
        let result = block_on(future::ready(42));
        assert_eq!(result, 42);
        
        let result2 = block_on(async {
            let val = future::ready(10).await;
            val * 2
        });
        assert_eq!(result2, 20);
    }
}
