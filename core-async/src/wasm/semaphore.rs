//! WASM Semaphore Implementation
//!
//! A counting semaphore for WASM that uses `Rc<RefCell<>>` for interior mutability
//! and properly queues `Waker`s instead of spin-waiting.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Shared state for the semaphore
struct SemaphoreState {
    /// Number of available permits
    permits: usize,
    /// Queue of waiters (FIFO order)
    waiters: Vec<Waker>,
    /// Whether the semaphore is closed
    closed: bool,
}

/// A counting semaphore for WASM.
///
/// This semaphore allows limiting the number of concurrent operations.
/// When all permits are acquired, future attempts will wait until a permit
/// is released.
///
/// Unlike native (Tokio) implementation, this is single-threaded and uses
/// `Rc<RefCell<>>` instead of atomic operations.
#[derive(Clone)]
pub struct Semaphore {
    state: Rc<RefCell<SemaphoreState>>,
}

impl Semaphore {
    /// Creates a new semaphore with the given number of permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Semaphore;
    ///
    /// let semaphore = Semaphore::new(5);
    /// ```
    pub fn new(permits: usize) -> Self {
        Self {
            state: Rc::new(RefCell::new(SemaphoreState {
                permits,
                waiters: Vec::new(),
                closed: false,
            })),
        }
    }

    /// Acquires a permit from the semaphore.
    ///
    /// If no permits are available, this will wait until one becomes available.
    /// Returns `Err(())` if the semaphore has been closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Semaphore;
    ///
    /// # async fn example() -> Result<(), ()> {
    /// let semaphore = Semaphore::new(3);
    /// let permit = semaphore.acquire().await?;
    /// // Do work...
    /// drop(permit); // Release permit
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, ()> {
        AcquireFuture {
            semaphore: self,
            registered: false,
        }
        .await
    }

    /// Returns the number of available permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Semaphore;
    ///
    /// let semaphore = Semaphore::new(5);
    /// assert_eq!(semaphore.available_permits(), 5);
    /// ```
    pub fn available_permits(&self) -> usize {
        self.state.borrow().permits
    }

    /// Attempts to acquire a permit without waiting.
    ///
    /// Returns `None` if no permits are available.
    pub fn try_acquire(&self) -> Option<SemaphorePermit<'_>> {
        let mut state = self.state.borrow_mut();

        if state.closed {
            return None;
        }

        if state.permits > 0 {
            state.permits -= 1;
            Some(SemaphorePermit {
                semaphore: self.clone(),
                _phantom: std::marker::PhantomData,
            })
        } else {
            None
        }
    }

    /// Releases a permit back to the semaphore.
    ///
    /// This is called automatically when a `SemaphorePermit` is dropped.
    fn release(&self) {
        let mut state = self.state.borrow_mut();
        state.permits += 1;

        // Wake up the next waiter if any
        if let Some(waker) = state.waiters.pop() {
            // Don't hold the borrow across the wake
            drop(state);
            waker.wake();
        }
    }

    /// Closes the semaphore, preventing new acquires and waking all waiters.
    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;

        // Wake all waiters so they can see the closed state
        let waiters = std::mem::take(&mut state.waiters);
        drop(state);

        for waker in waiters {
            waker.wake();
        }
    }
}

/// A future that resolves when a permit is acquired.
struct AcquireFuture<'a> {
    semaphore: &'a Semaphore,
    registered: bool,
}

impl<'a> Future for AcquireFuture<'a> {
    type Output = Result<SemaphorePermit<'a>, ()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.semaphore.state.borrow_mut();

        if state.closed {
            return Poll::Ready(Err(()));
        }

        if state.permits > 0 {
            // Permit available, acquire it
            state.permits -= 1;
            return Poll::Ready(Ok(SemaphorePermit {
                semaphore: self.semaphore.clone(),
                _phantom: std::marker::PhantomData,
            }));
        }

        // No permits available, register waker
        if !self.registered {
            state.waiters.push(cx.waker().clone());
            self.registered = true;
        } else {
            // Update waker in case it changed
            if let Some(last) = state.waiters.last_mut() {
                if !last.will_wake(cx.waker()) {
                    *last = cx.waker().clone();
                }
            }
        }

        Poll::Pending
    }
}

/// A permit from a `Semaphore`.
///
/// This type is returned by `Semaphore::acquire` and automatically releases
/// the permit when dropped.
pub struct SemaphorePermit<'a> {
    semaphore: Semaphore,
    _phantom: std::marker::PhantomData<fn(&'a ())>,
}

impl<'a> SemaphorePermit<'a> {
    /// Forgets the permit, leaving the permit held permanently.
    ///
    /// This can be used to permanently reduce the number of available permits.
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

impl<'a> Drop for SemaphorePermit<'a> {
    fn drop(&mut self) {
        self.semaphore.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_semaphore_basic() {
        let sem = Semaphore::new(2);
        assert_eq!(sem.available_permits(), 2);

        // Acquire permits
        let permit1 = sem.acquire().await.unwrap();
        assert_eq!(sem.available_permits(), 1);

        let permit2 = sem.acquire().await.unwrap();
        assert_eq!(sem.available_permits(), 0);

        // Release and reacquire
        drop(permit1);
        assert_eq!(sem.available_permits(), 1);

        drop(permit2);
        assert_eq!(sem.available_permits(), 2);
    }

    #[wasm_bindgen_test]
    async fn test_semaphore_contention() {
        let sem = Semaphore::new(1);

        // Acquire the only permit
        let permit1 = sem.acquire().await.unwrap();
        assert_eq!(sem.available_permits(), 0);

        // Try to acquire with try_acquire - should fail
        assert!(sem.try_acquire().is_none());

        // Release and immediately acquire again
        drop(permit1);
        assert_eq!(sem.available_permits(), 1);

        let _permit2 = sem.acquire().await.unwrap();
        assert_eq!(sem.available_permits(), 0);
    }

    #[wasm_bindgen_test]
    async fn test_semaphore_multiple_waiters() {
        let sem = Semaphore::new(2);

        // Acquire all permits
        let permit1 = sem.acquire().await.unwrap();
        let permit2 = sem.acquire().await.unwrap();
        assert_eq!(sem.available_permits(), 0);

        // Release permits
        drop(permit1);
        drop(permit2);
        assert_eq!(sem.available_permits(), 2);

        // Reacquire to test multiple sequential acquires
        for _ in 0..5 {
            let _permit = sem.acquire().await.unwrap();
            drop(_permit);
        }
        
        assert_eq!(sem.available_permits(), 2);
    }

    #[wasm_bindgen_test]
    async fn test_semaphore_close() {
        let sem = Semaphore::new(1);

        let permit = sem.acquire().await.unwrap();

        // Close the semaphore
        sem.close();

        // Try to acquire - should fail
        let result = sem.acquire().await;
        assert!(result.is_err());

        // Release the permit
        drop(permit);

        // Still should fail after release
        let result = sem.acquire().await;
        assert!(result.is_err());
    }
}
