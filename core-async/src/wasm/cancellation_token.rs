//! WASM CancellationToken Implementation
//!
//! A token that can be used to signal cancellation across tasks.
//! Provides async wait functionality for cooperative cancellation.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Shared state for the cancellation token
struct TokenState {
    /// Whether cancellation has been requested
    cancelled: bool,
    /// Wakers waiting for cancellation
    waiters: Vec<Waker>,
}

/// A token that can be used to signal cancellation across tasks.
///
/// `CancellationToken` allows cooperative cancellation of tasks. When `cancel()`
/// is called, all tasks waiting on `cancelled()` will be woken up.
///
/// # Examples
///
/// ```
/// use core_async::sync::CancellationToken;
/// use std::rc::Rc;
///
/// # async fn example() {
/// let token = Rc::new(CancellationToken::new());
/// let token_clone = token.clone();
///
/// // Spawn a task that can be cancelled
/// let handle = crate::task::spawn(async move {
///     loop {
///         if token_clone.is_cancelled() {
///             break;
///         }
///         // Do work...
///         # break;
///     }
/// });
///
/// // Cancel the operation
/// token.cancel();
/// # }
/// ```
#[derive(Clone)]
pub struct CancellationToken {
    state: Rc<RefCell<TokenState>>,
}

impl CancellationToken {
    /// Creates a new cancellation token.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::CancellationToken;
    ///
    /// let token = CancellationToken::new();
    /// ```
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(TokenState {
                cancelled: false,
                waiters: Vec::new(),
            })),
        }
    }

    /// Cancels the token and wakes all waiting tasks.
    ///
    /// After calling this method, `is_cancelled()` will return `true` and
    /// all tasks waiting on `cancelled()` will be woken up.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::CancellationToken;
    ///
    /// let token = CancellationToken::new();
    /// token.cancel();
    /// assert!(token.is_cancelled());
    /// ```
    pub fn cancel(&self) {
        let mut state = self.state.borrow_mut();
        state.cancelled = true;

        // Wake all waiting tasks
        let waiters = std::mem::take(&mut state.waiters);
        drop(state); // Drop borrow before waking

        for waker in waiters {
            waker.wake();
        }
    }

    /// Returns `true` if cancellation has been requested.
    ///
    /// This is a non-async check that can be used in tight loops.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::CancellationToken;
    ///
    /// let token = CancellationToken::new();
    /// assert!(!token.is_cancelled());
    ///
    /// token.cancel();
    /// assert!(token.is_cancelled());
    /// ```
    pub fn is_cancelled(&self) -> bool {
        self.state.borrow().cancelled
    }

    /// Waits for the token to be cancelled.
    ///
    /// This is an async method that completes when `cancel()` is called.
    /// If the token is already cancelled, this returns immediately.
    ///
    /// This enables cooperative cancellation patterns where tasks can await
    /// cancellation instead of polling `is_cancelled()` in a loop.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::CancellationToken;
    /// use std::rc::Rc;
    ///
    /// # async fn example() {
    /// let token = Rc::new(CancellationToken::new());
    /// let token_clone = token.clone();
    ///
    /// // Spawn a task that waits for cancellation
    /// let handle = crate::task::spawn(async move {
    ///     token_clone.cancelled().await;
    ///     println!("Cancelled!");
    /// });
    ///
    /// // Cancel the token
    /// token.cancel();
    ///
    /// // Task will complete
    /// handle.await.unwrap();
    /// # }
    /// ```
    pub async fn cancelled(&self) {
        CancelledFuture {
            token: self.clone(),
            registered: false,
        }
        .await
    }

    /// Creates a child token that is cancelled when either this token or
    /// the parent is cancelled.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::CancellationToken;
    ///
    /// let parent = CancellationToken::new();
    /// let child = parent.child_token();
    ///
    /// parent.cancel();
    /// assert!(child.is_cancelled());
    /// ```
    pub fn child_token(&self) -> CancellationToken {
        // For simplicity, just return a clone on WASM
        // A full implementation would track parent-child relationships
        self.clone()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by `CancellationToken::cancelled()`.
struct CancelledFuture {
    token: CancellationToken,
    registered: bool,
}

impl Future for CancelledFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.token.state.borrow_mut();

        // Check if already cancelled
        if state.cancelled {
            return Poll::Ready(());
        }

        // Register waker
        let should_register = !self.registered;
        if should_register {
            state.waiters.push(cx.waker().clone());
        } else {
            // Update waker if changed
            if let Some(last) = state.waiters.last_mut() {
                if !last.will_wake(cx.waker()) {
                    *last = cx.waker().clone();
                }
            }
        }
        
        drop(state);
        if should_register {
            self.registered = true;
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_cancellation_token_basic() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
    }

    #[wasm_bindgen_test]
    async fn test_cancellation_token_wait() {
        let token = Rc::new(CancellationToken::new());
        let token_clone = token.clone();

        let handle = crate::task::spawn(async move {
            token_clone.cancelled().await;
            42
        });

        // Give task time to register
        crate::task::yield_now().await;

        // Cancel the token
        token.cancel();

        // Task should complete
        let result = handle.await.unwrap();
        assert_eq!(result, 42);
    }

    #[wasm_bindgen_test]
    async fn test_cancellation_token_immediate() {
        let token = CancellationToken::new();

        // Cancel before waiting
        token.cancel();

        // Should complete immediately
        token.cancelled().await;
    }

    #[wasm_bindgen_test]
    async fn test_cancellation_token_multiple_waiters() {
        let token = Rc::new(CancellationToken::new());

        // Spawn multiple waiting tasks
        let mut handles = Vec::new();
        for i in 0..5 {
            let token_clone = token.clone();
            let handle = crate::task::spawn(async move {
                token_clone.cancelled().await;
                i
            });
            handles.push(handle);
        }

        // Give tasks time to register
        crate::task::yield_now().await;

        // Cancel the token
        token.cancel();

        // All tasks should complete
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.unwrap();
            assert_eq!(result, i);
        }
    }

    #[wasm_bindgen_test]
    async fn test_cancellation_token_child() {
        let parent = Rc::new(CancellationToken::new());
        let child = parent.child_token();

        assert!(!parent.is_cancelled());
        assert!(!child.is_cancelled());

        parent.cancel();

        assert!(parent.is_cancelled());
        assert!(child.is_cancelled());
    }
}
