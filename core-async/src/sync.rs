//! Synchronization primitives.
//!
//! This module provides platform-agnostic synchronization types:
//! - On native platforms: Uses `tokio::sync` (async-aware primitives)
//! - On WASM: Uses `futures::lock` (single-threaded async primitives)
//!
//! # Platform Differences
//!
//! ## Native (Tokio)
//! - All primitives are `Send + Sync` and can be shared across threads
//! - Mutexes and RwLocks are async-aware and won't block the executor
//! - Channels support multi-producer, multi-consumer patterns
//!
//! ## WASM
//! - Primitives are single-threaded (not `Send`)
//! - Simpler implementations since there's no thread contention
//! - Channels use `futures::channel` implementations
//!
//! # Examples
//!
//! ```rust
//! use core_async::sync::{Mutex, RwLock};
//!
//! async fn example() {
//!     let mutex = Mutex::new(42);
//!     let mut guard = mutex.lock().await;
//!     *guard += 1;
//!     drop(guard); // Release lock
//!     
//!     let rwlock = RwLock::new(vec![1, 2, 3]);
//!     let read_guard = rwlock.read().await;
//!     println!("Length: {}", read_guard.len());
//! }
//! ```

// ============================================================================
// Native Implementation (Tokio)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::{
    broadcast, mpsc, oneshot, watch, Barrier, Mutex, MutexGuard, Notify, RwLock, RwLockReadGuard,
    RwLockWriteGuard, Semaphore, SemaphorePermit,
};

#[cfg(not(target_arch = "wasm32"))]
pub use tokio_util::sync::CancellationToken;

// ============================================================================
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
pub use futures::channel::{mpsc, oneshot};

#[cfg(target_arch = "wasm32")]
use std::{cell::Cell, rc::Rc};

#[cfg(target_arch = "wasm32")]
/// An async mutex for protecting shared data.
///
/// On WASM, this is single-threaded and doesn't need actual locking,
/// but provides the same API as the native version.
pub struct Mutex<T> {
    inner: futures::lock::Mutex<T>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Rc<Cell<bool>>,
}

#[cfg(target_arch = "wasm32")]
impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Rc::new(Cell::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.set(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
}

#[cfg(target_arch = "wasm32")]
impl<T> Mutex<T> {
    /// Creates a new mutex with the given value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::Mutex;
    ///
    /// let mutex = Mutex::new(42);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            inner: futures::lock::Mutex::new(value),
        }
    }

    /// Acquires the mutex, blocking the current task until it is available.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::Mutex;
    ///
    /// # async fn example() {
    /// let mutex = Mutex::new(42);
    /// let mut guard = mutex.lock().await;
    /// *guard += 1;
    /// # }
    /// ```
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        MutexGuard {
            inner: self.inner.lock().await,
        }
    }

    /// Attempts to acquire the mutex without blocking.
    ///
    /// Returns `None` if the mutex is currently locked.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().map(|inner| MutexGuard { inner })
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: std::fmt::Debug> std::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mutex").finish_non_exhaustive()
    }
}

#[cfg(target_arch = "wasm32")]
/// A guard that releases the mutex when dropped.
pub struct MutexGuard<'a, T> {
    inner: futures::lock::MutexGuard<'a, T>,
}

#[cfg(target_arch = "wasm32")]
impl<'a, T> std::ops::Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T> std::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Debug> std::fmt::Debug for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Display> std::fmt::Display for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
/// A reader-writer lock for protecting shared data.
///
/// Multiple readers can hold the lock simultaneously, but only one writer
/// can hold it at a time.
pub struct RwLock<T> {
    inner: futures::lock::Mutex<T>, // WASM is single-threaded, so we use a mutex
}

#[cfg(target_arch = "wasm32")]
impl<T> RwLock<T> {
    /// Creates a new reader-writer lock.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::RwLock;
    ///
    /// let lock = RwLock::new(vec![1, 2, 3]);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            inner: futures::lock::Mutex::new(value),
        }
    }

    /// Acquires a read lock.
    ///
    /// On WASM, this behaves identically to a write lock since we're single-threaded.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::RwLock;
    ///
    /// # async fn example() {
    /// let lock = RwLock::new(42);
    /// let guard = lock.read().await;
    /// println!("Value: {}", *guard);
    /// # }
    /// ```
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        RwLockReadGuard {
            inner: self.inner.lock().await,
        }
    }

    /// Acquires a write lock.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::RwLock;
    ///
    /// # async fn example() {
    /// let lock = RwLock::new(42);
    /// let mut guard = lock.write().await;
    /// *guard += 1;
    /// # }
    /// ```
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        RwLockWriteGuard {
            inner: self.inner.lock().await,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: Default> Default for RwLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: std::fmt::Debug> std::fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RwLock").finish_non_exhaustive()
    }
}

#[cfg(target_arch = "wasm32")]
/// A read guard for `RwLock`.
pub struct RwLockReadGuard<'a, T> {
    inner: futures::lock::MutexGuard<'a, T>,
}

#[cfg(target_arch = "wasm32")]
impl<'a, T> std::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Debug> std::fmt::Debug for RwLockReadGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Display> std::fmt::Display for RwLockReadGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
/// A write guard for `RwLock`.
pub struct RwLockWriteGuard<'a, T> {
    inner: futures::lock::MutexGuard<'a, T>,
}

#[cfg(target_arch = "wasm32")]
impl<'a, T> std::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T> std::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Debug> std::fmt::Debug for RwLockWriteGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
impl<'a, T: std::fmt::Display> std::fmt::Display for RwLockWriteGuard<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&**self, f)
    }
}

#[cfg(target_arch = "wasm32")]
/// A synchronization primitive for notifying tasks.
///
/// This is a simplified version for WASM.
pub struct Notify {
    notified: std::rc::Rc<std::cell::Cell<bool>>,
}

#[cfg(target_arch = "wasm32")]
impl Notify {
    /// Creates a new `Notify`.
    pub fn new() -> Self {
        Self {
            notified: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
    }

    /// Notifies one waiting task.
    ///
    /// On WASM, this is a no-op for now since we don't have proper
    /// async notification support.
    pub fn notify_one(&self) {
        self.notified.set(true);
    }

    /// Notifies all waiting tasks.
    pub fn notify_waiters(&self) {
        self.notified.set(true);
    }

    /// Waits for a notification.
    ///
    /// On WASM, this checks if notified and resets the flag.
    pub async fn notified(&self) {
        // Simple spin-wait for WASM
        while !self.notified.get() {
            crate::task::yield_now().await;
        }
        self.notified.set(false);
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "wasm32")]
/// A barrier for synchronizing multiple tasks.
///
/// This is a simplified version for WASM. Not fully implemented.
pub struct Barrier {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(target_arch = "wasm32")]
impl Barrier {
    /// Creates a new barrier that waits for `n` tasks.
    ///
    /// # Panics
    ///
    /// Panics on WASM as barriers are not yet fully supported.
    pub fn new(_n: usize) -> Self {
        panic!("Barrier is not fully supported on WASM");
    }

    /// Waits for all tasks to reach the barrier.
    pub async fn wait(&self) {
        panic!("Barrier is not fully supported on WASM");
    }
}

#[cfg(target_arch = "wasm32")]
/// A counting semaphore.
///
/// This is a simplified version for WASM. Not fully implemented.
pub struct Semaphore {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(target_arch = "wasm32")]
impl Semaphore {
    /// Creates a new semaphore with `permits` permits.
    ///
    /// # Panics
    ///
    /// Panics on WASM as semaphores are not yet fully supported.
    pub fn new(_permits: usize) -> Self {
        panic!("Semaphore is not fully supported on WASM");
    }

    /// Acquires a permit.
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, ()> {
        panic!("Semaphore is not fully supported on WASM");
    }

    /// Returns the number of available permits.
    ///
    /// Note: On WASM this always returns 0 as semaphores are not fully implemented.
    pub fn available_permits(&self) -> usize {
        0
    }
}

#[cfg(target_arch = "wasm32")]
/// A permit from a semaphore.
pub struct SemaphorePermit<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

// ============================================================================
// Broadcast Channel (WASM Implementation)
// ============================================================================

#[cfg(target_arch = "wasm32")]
/// Broadcast channel types.
///
/// On WASM, broadcast channels use a single-threaded implementation
/// with `Rc` and `RefCell` for interior mutability.
pub mod broadcast {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Error type for broadcast operations.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecvError {
        /// Channel was closed.
        Closed,
        /// Message was missed due to buffer overflow.
        Lagged(u64),
    }

    impl std::fmt::Display for RecvError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                RecvError::Closed => write!(f, "broadcast channel closed"),
                RecvError::Lagged(n) => write!(f, "broadcast channel lagged by {} messages", n),
            }
        }
    }

    impl std::error::Error for RecvError {}

    /// Error type for send operations.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SendError<T>(pub T);

    impl<T> std::fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "broadcast channel closed")
        }
    }

    impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

    /// Shared state for a broadcast channel.
    struct Shared<T> {
        buffer: VecDeque<T>,
        capacity: usize,
        closed: bool,
        receiver_count: usize,
        total_sent: u64,
    }

    /// A broadcast sender.
    ///
    /// Messages sent through this sender will be received by all receivers.
    pub struct Sender<T> {
        shared: Rc<RefCell<Shared<T>>>,
    }

    impl<T: Clone> Sender<T> {
        /// Sends a value to all receivers.
        ///
        /// Returns the number of receivers that will receive the message.
        /// Returns an error if all receivers have been dropped.
        pub fn send(&self, value: T) -> Result<usize, SendError<T>> {
            let mut shared = self.shared.borrow_mut();

            if shared.closed || shared.receiver_count == 0 {
                return Err(SendError(value));
            }

            // Add to buffer
            shared.buffer.push_back(value);
            shared.total_sent += 1;

            // Truncate buffer if over capacity
            while shared.buffer.len() > shared.capacity {
                shared.buffer.pop_front();
            }

            Ok(shared.receiver_count)
        }

        /// Creates a new receiver for this broadcast channel.
        pub fn subscribe(&self) -> Receiver<T> {
            let mut shared = self.shared.borrow_mut();
            shared.receiver_count += 1;

            Receiver {
                shared: Rc::clone(&self.shared),
                next_index: shared.total_sent,
            }
        }

        /// Returns the number of active receivers.
        pub fn receiver_count(&self) -> usize {
            self.shared.borrow().receiver_count
        }
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self {
                shared: Rc::clone(&self.shared),
            }
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            // If this is the last sender, close the channel
            if Rc::strong_count(&self.shared) == self.shared.borrow().receiver_count + 1 {
                self.shared.borrow_mut().closed = true;
            }
        }
    }

    /// A broadcast receiver.
    ///
    /// Receives messages sent by the broadcast sender.
    pub struct Receiver<T> {
        shared: Rc<RefCell<Shared<T>>>,
        next_index: u64,
    }

    impl<T: Clone> Receiver<T> {
        /// Receives the next value from the channel.
        ///
        /// Waits asynchronously if no messages are available.
        pub async fn recv(&mut self) -> Result<T, RecvError> {
            loop {
                let result = self.try_recv();

                match result {
                    Ok(value) => return Ok(value),
                    Err(TryRecvError::Empty) => {
                        // Wait a bit before trying again
                        crate::task::yield_now().await;
                        continue;
                    }
                    Err(TryRecvError::Closed) => return Err(RecvError::Closed),
                    Err(TryRecvError::Lagged(n)) => return Err(RecvError::Lagged(n)),
                }
            }
        }

        /// Attempts to receive the next value without blocking.
        pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
            let shared = self.shared.borrow();

            if shared.closed && shared.buffer.is_empty() {
                return Err(TryRecvError::Closed);
            }

            // Check if we've lagged behind
            let oldest_index = shared.total_sent.saturating_sub(shared.buffer.len() as u64);
            if self.next_index < oldest_index {
                let lagged_by = oldest_index - self.next_index;
                self.next_index = oldest_index;
                return Err(TryRecvError::Lagged(lagged_by));
            }

            // Check if there are new messages for us
            let buffer_index = (self.next_index - oldest_index) as usize;
            if buffer_index >= shared.buffer.len() {
                return Err(TryRecvError::Empty);
            }

            // Get the message
            let value = shared.buffer[buffer_index].clone();
            self.next_index += 1;

            Ok(value)
        }
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            let mut shared = self.shared.borrow_mut();
            shared.receiver_count = shared.receiver_count.saturating_sub(1);
        }
    }

    /// Error type for try_recv operations.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TryRecvError {
        /// No messages are available.
        Empty,
        /// Channel was closed.
        Closed,
        /// Message was missed due to buffer overflow.
        Lagged(u64),
    }

    /// Creates a new broadcast channel.
    ///
    /// The channel will buffer up to `capacity` messages before old messages
    /// are dropped.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core_async::sync::broadcast;
    ///
    /// let (tx, mut rx) = broadcast::channel(16);
    /// ```
    pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let shared = Rc::new(RefCell::new(Shared {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            closed: false,
            receiver_count: 1, // One receiver created by default
            total_sent: 0,
        }));

        let sender = Sender {
            shared: Rc::clone(&shared),
        };

        let receiver = Receiver {
            shared,
            next_index: 0,
        };

        (sender, receiver)
    }
}

// ============================================================================
// Watch Channel (WASM Stub)
// ============================================================================

#[cfg(target_arch = "wasm32")]
/// Watch channel types.
///
/// On WASM, watch channels are not yet implemented.
pub mod watch {
    /// Error type for watch operations.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecvError;

    /// Error type for send operations.
    #[derive(Debug)]
    pub struct SendError<T>(pub T);

    /// A watch sender.
    pub struct Sender<T> {
        _phantom: std::marker::PhantomData<T>,
    }

    /// A watch receiver.
    pub struct Receiver<T> {
        _phantom: std::marker::PhantomData<T>,
    }

    /// Creates a new watch channel.
    ///
    /// # Panics
    ///
    /// Panics on WASM as watch channels are not yet supported.
    pub fn channel<T>(_initial: T) -> (Sender<T>, Receiver<T>) {
        panic!("watch channels are not yet supported on WASM");
    }
}
