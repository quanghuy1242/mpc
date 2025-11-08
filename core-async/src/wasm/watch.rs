//! WASM Watch Channel Implementation
//!
//! A single-producer, multi-consumer channel where the producer can update a value
//! and consumers can observe the latest value. Uses proper Waker-based notifications.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Shared state for the watch channel
struct WatchState<T> {
    /// The current value
    value: T,
    /// Version number, incremented on each send
    version: u64,
    /// Whether the sender has been dropped
    closed: bool,
    /// Wakers for receivers waiting for changes
    waiters: Vec<Waker>,
}

/// Error returned when the watch channel is closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecvError;

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "watch channel closed")
    }
}

impl std::error::Error for RecvError {}

/// Error returned when sending fails (all receivers dropped).
#[derive(Debug)]
pub struct SendError<T>(pub T);

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "watch channel closed - no receivers")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

/// The sending side of a watch channel.
///
/// Values sent will be observed by all receivers.
pub struct Sender<T> {
    state: Rc<RefCell<WatchState<T>>>,
}

impl<T> Sender<T> {
    /// Sends a new value to all receivers.
    ///
    /// All receivers will be able to observe this new value.
    /// Returns an error if all receivers have been dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::watch;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (tx, mut rx) = watch::channel(0);
    /// tx.send(42)?;
    /// assert_eq!(*rx.borrow(), 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut state = self.state.borrow_mut();

        // Check if there are any receivers
        if state.closed && state.waiters.is_empty() {
            return Err(SendError(value));
        }

        state.value = value;
        state.version += 1;

        // Wake all waiting receivers
        let waiters = std::mem::take(&mut state.waiters);
        drop(state); // Drop borrow before waking

        for waker in waiters {
            waker.wake();
        }

        Ok(())
    }

    /// Returns a reference to the most recently sent value.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::watch;
    ///
    /// let (tx, _rx) = watch::channel(42);
    /// assert_eq!(*tx.borrow(), 42);
    /// ```
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            guard: self.state.borrow(),
        }
    }

    /// Checks if any receivers exist.
    pub fn is_closed(&self) -> bool {
        let state = self.state.borrow();
        state.closed && state.waiters.is_empty()
    }

    /// Creates a new receiver for this channel.
    pub fn subscribe(&self) -> Receiver<T> {
        Receiver {
            state: Rc::clone(&self.state),
            version: self.state.borrow().version,
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;

        // Wake all waiting receivers so they can see the channel is closed
        let waiters = std::mem::take(&mut state.waiters);
        drop(state);

        for waker in waiters {
            waker.wake();
        }
    }
}

/// The receiving side of a watch channel.
///
/// Can observe the latest value sent by the sender.
pub struct Receiver<T> {
    state: Rc<RefCell<WatchState<T>>>,
    /// Last seen version
    version: u64,
}

impl<T: Clone> Receiver<T> {
    /// Receives the next value, waiting if the current value has already been seen.
    ///
    /// Returns an error if the sender has been dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::watch;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (tx, mut rx) = watch::channel(0);
    ///
    /// tx.send(42)?;
    /// let value = rx.changed().await?;
    /// assert_eq!(value, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn changed(&mut self) -> Result<(), RecvError> {
        ChangedFuture {
            receiver: self,
            registered: false,
        }
        .await
    }

    /// Returns a reference to the most recently sent value.
    ///
    /// This method does not mark the value as seen, so future calls to
    /// `changed()` may still return immediately if the value has changed.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::watch;
    ///
    /// let (tx, rx) = watch::channel(42);
    /// assert_eq!(*rx.borrow(), 42);
    /// ```
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            guard: self.state.borrow(),
        }
    }

    /// Returns a clone of the most recently sent value and marks it as seen.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::watch;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (tx, mut rx) = watch::channel(42);
    /// let value = rx.borrow_and_update();
    /// assert_eq!(*value, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn borrow_and_update(&mut self) -> Ref<'_, T> {
        let state = self.state.borrow();
        self.version = state.version;
        Ref { guard: state }
    }

    /// Returns `true` if the value has changed since the last time it was seen.
    pub fn has_changed(&self) -> Result<bool, RecvError> {
        let state = self.state.borrow();

        if state.closed && self.version == state.version {
            return Err(RecvError);
        }

        Ok(self.version != state.version)
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
            version: self.version,
        }
    }
}

/// A reference to the current value in the watch channel.
pub struct Ref<'a, T> {
    guard: std::cell::Ref<'a, WatchState<T>>,
}

impl<'a, T> std::ops::Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard.value
    }
}

/// Future returned by `Receiver::changed()`.
struct ChangedFuture<'a, T> {
    receiver: &'a mut Receiver<T>,
    registered: bool,
}

impl<'a, T> Future for ChangedFuture<'a, T> {
    type Output = Result<(), RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.receiver.state.borrow_mut();

        // Check if value has changed
        if self.receiver.version != state.version {
            let new_version = state.version;
            drop(state);
            self.receiver.version = new_version;
            return Poll::Ready(Ok(()));
        }

        // Check if channel is closed
        if state.closed {
            return Poll::Ready(Err(RecvError));
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

/// Creates a new watch channel.
///
/// The channel starts with the given initial value.
///
/// # Examples
///
/// ```
/// use core_async::sync::watch;
///
/// let (tx, rx) = watch::channel(42);
/// assert_eq!(*rx.borrow(), 42);
/// ```
pub fn channel<T>(initial: T) -> (Sender<T>, Receiver<T>) {
    let state = Rc::new(RefCell::new(WatchState {
        value: initial,
        version: 0,
        closed: false,
        waiters: Vec::new(),
    }));

    let sender = Sender {
        state: Rc::clone(&state),
    };

    let receiver = Receiver {
        state,
        version: 0,
    };

    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_watch_basic() {
        let (tx, rx) = channel(42);
        assert_eq!(*rx.borrow(), 42);

        tx.send(100).unwrap();
        assert_eq!(*rx.borrow(), 100);
    }

    #[wasm_bindgen_test]
    async fn test_watch_changed() {
        let (tx, mut rx) = channel(0);

        // Spawn a task that waits for change
        let handle = crate::task::spawn(async move {
            rx.changed().await.unwrap();
            *rx.borrow()
        });

        // Give task time to register
        crate::task::yield_now().await;

        // Send a new value
        tx.send(42).unwrap();

        // Task should complete with new value
        let result = handle.await.unwrap();
        assert_eq!(result, 42);
    }

    #[wasm_bindgen_test]
    async fn test_watch_multiple_receivers() {
        let (tx, rx) = channel(0);

        let mut rx1 = rx.clone();
        let mut rx2 = rx.clone();

        let handle1 = crate::task::spawn(async move {
            rx1.changed().await.unwrap();
            *rx1.borrow()
        });

        let handle2 = crate::task::spawn(async move {
            rx2.changed().await.unwrap();
            *rx2.borrow()
        });

        // Give tasks time to register
        crate::task::yield_now().await;

        // Send a value
        tx.send(42).unwrap();

        // Both should see it
        assert_eq!(handle1.await.unwrap(), 42);
        assert_eq!(handle2.await.unwrap(), 42);
    }

    #[wasm_bindgen_test]
    async fn test_watch_close() {
        let (tx, mut rx) = channel(0);

        let handle = crate::task::spawn(async move {
            rx.changed().await
        });

        // Give task time to register
        crate::task::yield_now().await;

        // Drop sender to close channel
        drop(tx);

        // Should get an error
        let result = handle.await.unwrap();
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn test_watch_borrow_and_update() {
        let (tx, mut rx) = channel(0);

        tx.send(42).unwrap();

        // borrow_and_update marks value as seen
        assert_eq!(*rx.borrow_and_update(), 42);
        assert!(!rx.has_changed().unwrap());

        // Send another value
        tx.send(100).unwrap();
        assert!(rx.has_changed().unwrap());
    }
}
