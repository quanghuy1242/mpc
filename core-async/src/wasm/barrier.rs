//! WASM Barrier Implementation
//!
//! A synchronization primitive that allows multiple tasks to wait until all
//! have reached a synchronization point.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Shared state for the barrier
struct BarrierState {
    /// Number of tasks that must wait before all are released
    count: usize,
    /// Number of tasks currently waiting
    waiting: usize,
    /// Generation counter to distinguish different "rounds" of waiting
    generation: usize,
    /// Wakers for tasks waiting at the barrier
    waiters: Vec<Waker>,
}

/// A barrier that blocks tasks until all participants have reached it.
///
/// A barrier is a synchronization primitive that allows multiple tasks to
/// wait until all of them have reached a synchronization point.
///
/// # Examples
///
/// ```
/// use core_async::sync::Barrier;
/// use std::rc::Rc;
///
/// # async fn example() {
/// let barrier = Rc::new(Barrier::new(3));
///
/// let mut handles = Vec::new();
/// for i in 0..3 {
///     let barrier_clone = barrier.clone();
///     let handle = crate::task::spawn(async move {
///         println!("Task {} waiting", i);
///         barrier_clone.wait().await;
///         println!("Task {} released", i);
///     });
///     handles.push(handle);
/// }
///
/// for handle in handles {
///     handle.await.unwrap();
/// }
/// # }
/// ```
#[derive(Clone)]
pub struct Barrier {
    state: Rc<RefCell<BarrierState>>,
}

impl Barrier {
    /// Creates a new barrier that blocks `n` tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Barrier;
    ///
    /// let barrier = Barrier::new(5);
    /// ```
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "barrier count must be greater than 0");

        Self {
            state: Rc::new(RefCell::new(BarrierState {
                count: n,
                waiting: 0,
                generation: 0,
                waiters: Vec::new(),
            })),
        }
    }

    /// Waits for all tasks to reach the barrier.
    ///
    /// Returns a `BarrierWaitResult` indicating whether this task was the
    /// last to reach the barrier.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Barrier;
    /// use std::rc::Rc;
    ///
    /// # async fn example() {
    /// let barrier = Rc::new(Barrier::new(2));
    /// let barrier_clone = barrier.clone();
    ///
    /// crate::task::spawn(async move {
    ///     barrier_clone.wait().await;
    /// });
    ///
    /// barrier.wait().await;
    /// # }
    /// ```
    pub async fn wait(&self) -> BarrierWaitResult {
        WaitFuture {
            barrier: self.clone(),
            generation: None,
            registered: false,
        }
        .await
    }
}

/// Result returned from `Barrier::wait()`.
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// Returns `true` if this task was the last to reach the barrier.
    ///
    /// The "leader" task can be used to perform cleanup or coordination
    /// after all tasks have synchronized.
    ///
    /// # Examples
    ///
    /// ```
    /// use core_async::sync::Barrier;
    ///
    /// # async fn example() {
    /// let barrier = Barrier::new(3);
    /// let result = barrier.wait().await;
    /// if result.is_leader() {
    ///     println!("I'm the leader!");
    /// }
    /// # }
    /// ```
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }
}

/// Future returned by `Barrier::wait()`.
struct WaitFuture {
    barrier: Barrier,
    generation: Option<usize>,
    registered: bool,
}

impl Future for WaitFuture {
    type Output = BarrierWaitResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let current_generation = {
            let state = self.barrier.state.borrow();
            state.generation
        };
        let generation = *self.generation.get_or_insert(current_generation);

        let mut state = self.barrier.state.borrow_mut();

        // If generation has changed, we've been released
        if state.generation != generation {
            return Poll::Ready(BarrierWaitResult { is_leader: false });
        }

        // Increment waiting count on first registration
        let should_register = !self.registered;
        if should_register {
            state.waiting += 1;

            // Check if we're the last to arrive
            if state.waiting == state.count {
                // We're the leader - release all waiters
                state.waiting = 0;
                state.generation = state.generation.wrapping_add(1);

                let waiters = std::mem::take(&mut state.waiters);
                drop(state); // Drop borrow before waking

                // Wake all other waiters
                for waker in waiters {
                    waker.wake();
                }

                self.registered = true;
                return Poll::Ready(BarrierWaitResult { is_leader: true });
            }
        }

        // Register waker
        if should_register {
            state.waiters.push(cx.waker().clone());
        } else {
            // Update waker if changed
            if let Some(last) = state.waiters.last_mut() {
                if !last.will_wake(cx.waker()) {
                    *last = cx.waker().clone();
                }
            } else {
                state.waiters.push(cx.waker().clone());
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
    async fn test_barrier_basic() {
        let barrier = Rc::new(Barrier::new(3));
        let mut handles = Vec::new();
        let mut leader_count = 0;

        for _ in 0..3 {
            let barrier_clone = barrier.clone();
            let handle = crate::task::spawn(async move {
                let result = barrier_clone.wait().await;
                result.is_leader()
            });
            handles.push(handle);
        }

        // All tasks should complete
        for handle in handles {
            let is_leader = handle.await.unwrap();
            if is_leader {
                leader_count += 1;
            }
        }

        // Exactly one should be the leader
        assert_eq!(leader_count, 1);
    }

    #[wasm_bindgen_test]
    async fn test_barrier_reuse() {
        let barrier = Rc::new(Barrier::new(2));

        // First round
        let barrier_clone = barrier.clone();
        let handle1 = crate::task::spawn(async move {
            barrier_clone.wait().await;
            1
        });

        barrier.wait().await;
        let result1 = handle1.await.unwrap();
        assert_eq!(result1, 1);

        // Second round - barrier should be reusable
        let barrier_clone = barrier.clone();
        let handle2 = crate::task::spawn(async move {
            barrier_clone.wait().await;
            2
        });

        barrier.wait().await;
        let result2 = handle2.await.unwrap();
        assert_eq!(result2, 2);
    }

    #[wasm_bindgen_test]
    async fn test_barrier_single() {
        let barrier = Barrier::new(1);
        let result = barrier.wait().await;
        assert!(result.is_leader());
    }

    #[wasm_bindgen_test]
    #[should_panic(expected = "barrier count must be greater than 0")]
    fn test_barrier_zero() {
        Barrier::new(0);
    }
}
