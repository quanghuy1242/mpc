//! WASM Notify Implementation
//!
//! A synchronization primitive that allows one task to notify another.
//! Uses proper `Waker`-based notification instead of spin-waiting.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Shared state for the Notify primitive
struct NotifyState {
    /// Waker for the waiting task (only one task can wait at a time in simple impl)
    waker: Option<Waker>,
    /// Whether a notification has been posted
    notified: bool,
}

/// A synchronization primitive for notifying tasks.
///
/// `Notify` provides a way for one task to signal another task that an event
/// has occurred. This is more efficient than using spin-loops with `yield_now`.
///
/// # Examples
///
/// ```
/// use core_async::sync::Notify;
/// use std::rc::Rc;
///
/// # async fn example() {
/// let notify = Rc::new(Notify::new());
/// let notify_clone = notify.clone();
///
/// // Spawn a task that waits for notification
/// crate::task::spawn(async move {
///     notify_clone.notified().await;
///     println!("Notified!");
/// });
///
/// // Notify the waiting task
/// notify.notify_one();
/// # }
/// ```
#[derive(Clone)]
pub struct Notify {
    state: Rc<RefCell<NotifyState>>,
}

impl Notify {
    /// Creates a new `Notify`.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Notify;
    ///
    /// let notify = Notify::new();
    /// ```
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(NotifyState {
                waker: None,
                notified: false,
            })),
        }
    }

    /// Notifies one waiting task.
    ///
    /// If a task is currently waiting on `notified()`, it will be woken up.
    /// If no task is waiting, the notification is stored and the next call
    /// to `notified()` will complete immediately.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Notify;
    ///
    /// let notify = Notify::new();
    /// notify.notify_one();
    /// ```
    pub fn notify_one(&self) {
        let mut state = self.state.borrow_mut();
        state.notified = true;

        // Wake the waiting task if any
        if let Some(waker) = state.waker.take() {
            drop(state); // Drop borrow before waking
            waker.wake();
        }
    }

    /// Notifies all waiting tasks.
    ///
    /// In this simple implementation, this behaves the same as `notify_one`
    /// since we only support one waiter at a time.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Notify;
    ///
    /// let notify = Notify::new();
    /// notify.notify_waiters();
    /// ```
    pub fn notify_waiters(&self) {
        self.notify_one();
    }

    /// Waits for a notification.
    ///
    /// This method completes when `notify_one()` or `notify_waiters()` is called.
    /// If a notification was already posted, this returns immediately.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Notify;
    /// use std::rc::Rc;
    ///
    /// # async fn example() {
    /// let notify = Rc::new(Notify::new());
    /// let notify_clone = notify.clone();
    ///
    /// crate::task::spawn(async move {
    ///     notify_clone.notified().await;
    /// });
    ///
    /// notify.notify_one();
    /// # }
    /// ```
    pub async fn notified(&self) {
        NotifiedFuture {
            notify: self.clone(),
            registered: false,
        }
        .await
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by `Notify::notified()`.
struct NotifiedFuture {
    notify: Notify,
    registered: bool,
}

impl Future for NotifiedFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.notify.state.borrow_mut();

        // Check if already notified
        if state.notified {
            state.notified = false;
            return Poll::Ready(());
        }

        // Register waker
        let should_register = !self.registered;
        if should_register {
            state.waker = Some(cx.waker().clone());
        } else {
            // Update waker if changed
            let needs_update = state.waker.as_ref()
                .map(|w| !w.will_wake(cx.waker()))
                .unwrap_or(true);
            
            if needs_update {
                state.waker = Some(cx.waker().clone());
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
    async fn test_notify_basic() {
        let notify = Notify::new();

        // Post notification before waiting
        notify.notify_one();

        // Should complete immediately since notification was posted
        notify.notified().await;
    }

    #[wasm_bindgen_test]
    async fn test_notify_immediate() {
        let notify = Notify::new();

        // Notify before waiting
        notify.notify_one();

        // Should complete immediately
        notify.notified().await;
    }

    #[wasm_bindgen_test]
    async fn test_notify_multiple() {
        let notify = Rc::new(Notify::new());

        let notify_clone = notify.clone();
        let handle1 = crate::task::spawn(async move {
            notify_clone.notified().await;
            1
        });

        let notify_clone = notify.clone();
        let handle2 = crate::task::spawn(async move {
            notify_clone.notified().await;
            2
        });

        // Give tasks time to register
        crate::task::yield_now().await;

        // Notify both (notify_waiters)
        notify.notify_waiters();
        notify.notify_waiters();

        // Both should complete
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();
        assert_eq!(result1, 1);
        assert_eq!(result2, 2);
    }
}
