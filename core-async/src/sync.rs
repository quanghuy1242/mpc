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

// ============================================================================
// WASM Implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
pub use futures::channel::{mpsc, oneshot};

#[cfg(target_arch = "wasm32")]
/// An async mutex for protecting shared data.
///
/// On WASM, this is single-threaded and doesn't need actual locking,
/// but provides the same API as the native version.
pub struct Mutex<T> {
    inner: futures::lock::Mutex<T>,
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
}

#[cfg(target_arch = "wasm32")]
/// A permit from a semaphore.
pub struct SemaphorePermit<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

// ============================================================================
// Broadcast Channel (WASM Stub)
// ============================================================================

#[cfg(target_arch = "wasm32")]
/// Broadcast channel types.
///
/// On WASM, broadcast channels are not yet implemented.
pub mod broadcast {
    /// Error type for broadcast operations.
    #[derive(Debug)]
    pub enum RecvError {
        /// Channel was closed.
        Closed,
        /// Message was missed.
        Lagged(u64),
    }

    /// Error type for send operations.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SendError<T>(pub T);

    /// A broadcast sender.
    pub struct Sender<T> {
        _phantom: std::marker::PhantomData<T>,
    }

    /// A broadcast receiver.
    pub struct Receiver<T> {
        _phantom: std::marker::PhantomData<T>,
    }

    /// Creates a new broadcast channel.
    ///
    /// # Panics
    ///
    /// Panics on WASM as broadcast channels are not yet supported.
    pub fn channel<T>(_capacity: usize) -> (Sender<T>, Receiver<T>) {
        panic!("broadcast channels are not yet supported on WASM");
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
