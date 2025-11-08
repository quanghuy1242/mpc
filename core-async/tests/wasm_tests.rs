//! Integration tests for core-async on WASM platforms.
//!
//! These tests verify that the async abstraction works correctly in a WASM environment.

#![cfg(target_arch = "wasm32")]

use core_async::{sync, task, time};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_task_spawn() {
    // On WASM, spawn now returns an awaitable JoinHandle!
    let handle = task::spawn(async {
        42
    });
    
    let result = handle.await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[wasm_bindgen_test]
async fn test_task_spawn_multiple() {
    // Test multiple concurrent tasks
    let handle1 = task::spawn(async { 1 });
    let handle2 = task::spawn(async { 2 });
    let handle3 = task::spawn(async { 3 });

    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();
    let result3 = handle3.await.unwrap();

    assert_eq!(result1 + result2 + result3, 6);
}

#[wasm_bindgen_test]
async fn test_task_spawn_with_complex_type() {
    let handle = task::spawn(async {
        vec![1, 2, 3, 4, 5]
    });

    let result = handle.await.unwrap();
    assert_eq!(result, vec![1, 2, 3, 4, 5]);
}

#[wasm_bindgen_test]
async fn test_task_spawn_nested() {
    // Test spawning tasks from within tasks
    let handle = task::spawn(async {
        let inner_handle = task::spawn(async {
            10
        });
        let inner_result = inner_handle.await.unwrap();
        inner_result * 2
    });

    let result = handle.await.unwrap();
    assert_eq!(result, 20);
}

#[wasm_bindgen_test]
async fn test_sleep() {
    let start = time::Instant::now();
    time::sleep(time::Duration::from_millis(50)).await;
    let elapsed = start.elapsed();

    // Allow more slack on WASM due to browser timing precision
    assert!(elapsed >= time::Duration::from_millis(40));
    assert!(elapsed < time::Duration::from_millis(200));
}

#[wasm_bindgen_test]
async fn test_timeout_success() {
    let result = time::timeout(time::Duration::from_millis(100), async {
        time::sleep(time::Duration::from_millis(10)).await;
        42
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[wasm_bindgen_test]
async fn test_timeout_failure() {
    let result = time::timeout(time::Duration::from_millis(10), async {
        time::sleep(time::Duration::from_millis(100)).await;
        42
    })
    .await;

    assert!(result.is_err());
}

#[wasm_bindgen_test]
async fn test_instant_elapsed() {
    let start = time::Instant::now();
    time::sleep(time::Duration::from_millis(50)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= time::Duration::from_millis(40));
}

#[wasm_bindgen_test]
async fn test_instant_operations() {
    let now = time::Instant::now();
    let later = now + time::Duration::from_millis(100);
    let earlier = now - time::Duration::from_millis(50);

    assert!(later.duration_since(now) == time::Duration::from_millis(100));
    assert!(now.duration_since(earlier) == time::Duration::from_millis(50));
}

#[wasm_bindgen_test]
async fn test_mutex() {
    let mutex = sync::Mutex::new(0);

    {
        let mut guard = mutex.lock().await;
        *guard += 1;
    }

    let guard = mutex.lock().await;
    assert_eq!(*guard, 1);
}

#[wasm_bindgen_test]
async fn test_mutex_try_lock() {
    let mutex = sync::Mutex::new(42);

    let guard1 = mutex.try_lock();
    assert!(guard1.is_some());
    assert_eq!(*guard1.unwrap(), 42);

    // After dropping, we can lock again
    let mut guard2 = mutex.lock().await;
    *guard2 = 100;
    drop(guard2);

    let guard3 = mutex.lock().await;
    assert_eq!(*guard3, 100);
}

#[wasm_bindgen_test]
async fn test_rwlock() {
    let rwlock = sync::RwLock::new(vec![1, 2, 3]);

    // Read access
    {
        let guard = rwlock.read().await;
        assert_eq!(guard.len(), 3);
    }

    // Write access
    {
        let mut guard = rwlock.write().await;
        guard.push(4);
        assert_eq!(guard.len(), 4);
    }

    // Read again to verify write
    {
        let guard = rwlock.read().await;
        assert_eq!(guard.len(), 4);
        assert_eq!(*guard, vec![1, 2, 3, 4]);
    }
}

#[wasm_bindgen_test]
async fn test_oneshot_channel() {
    let (tx, rx) = sync::oneshot::channel();

    task::spawn(async move {
        time::sleep(time::Duration::from_millis(10)).await;
        tx.send(42).unwrap();
    });

    let result = rx.await.unwrap();
    assert_eq!(result, 42);
}

#[wasm_bindgen_test]
async fn test_mpsc_channel() {
    use futures::StreamExt;
    let (mut tx, mut rx) = sync::mpsc::channel(10);

    task::spawn(async move {
        for i in 0..5 {
            tx.try_send(i).unwrap();
        }
    });

    time::sleep(time::Duration::from_millis(50)).await;

    let mut sum = 0;
    while let Some(value) = rx.next().await {
        sum += value;
        if sum >= 10 {
            break;
        }
    }

    assert_eq!(sum, 10); // 0 + 1 + 2 + 3 + 4 = 10
}

#[wasm_bindgen_test]
async fn test_notify() {
    let notify = sync::Notify::new();

    notify.notify_one();
    notify.notified().await;

    // Test completed successfully
}

#[wasm_bindgen_test]
async fn test_interval() {
    let mut interval = time::interval(time::Duration::from_millis(20));

    let start = time::Instant::now();

    // Wait for 2 ticks
    for _ in 0..2 {
        interval.tick().await;
    }

    let elapsed = start.elapsed();
    // Should be at least 40ms (2 ticks * 20ms)
    // Allow more slack on WASM
    assert!(elapsed >= time::Duration::from_millis(30));
}

#[wasm_bindgen_test]
async fn test_yield_now() {
    // Just verify it compiles and runs
    task::yield_now().await;
}

// This test is disabled on WASM because std::time::SystemTime is not supported
// The time module uses web-sys Performance API instead
// #[wasm_bindgen_test]
// async fn test_time_utilities() {
//     let now_millis = time::now_millis();
//     let now_secs = time::now_secs();
//     assert!(now_millis > 0);
//     assert!(now_secs > 0);
//     assert!(now_millis / 1000 >= now_secs - 1);
// }

#[wasm_bindgen_test]
async fn test_sequential_operations() {
    let mutex = sync::Mutex::new(0);

    for i in 0..5 {
        let mut guard = mutex.lock().await;
        *guard += i;
    }

    let final_value = *mutex.lock().await;
    assert_eq!(final_value, 10); // 0 + 1 + 2 + 3 + 4 = 10
}

#[wasm_bindgen_test]
async fn test_duration_calculations() {
    let d1 = time::Duration::from_secs(1);
    let d2 = time::Duration::from_millis(500);

    assert_eq!(d1.as_millis(), 1000);
    assert_eq!(d2.as_millis(), 500);

    let d3 = d1 + d2;
    assert_eq!(d3.as_millis(), 1500);
}

// ============================================================================
// Runtime block_on tests - LIMITED SUPPORT ON WASM
// ============================================================================
// NOTE: block_on on WASM only works for immediate futures that don't depend
// on the browser event loop. Most real-world async code should use spawn().

#[wasm_bindgen_test]
fn test_runtime_block_on_immediate() {
    use core_async::runtime;
    use futures::future;
    
    // Only test with immediate futures that don't need the event loop
    let result = runtime::block_on(future::ready(42));
    assert_eq!(result, 42);
    
    let result2 = runtime::block_on(async {
        let val = future::ready(10).await;
        val * 2
    });
    assert_eq!(result2, 20);
}

// ============================================================================
// Integration tests - spawn (the recommended pattern for WASM)
// ============================================================================

#[wasm_bindgen_test]
async fn test_integration_spawn_with_await() {
    use core_async::task;
    
    let handle = task::spawn(async { 100 });
    let result = handle.await.unwrap();
    assert_eq!(result, 100);
}

#[wasm_bindgen_test]
async fn test_integration_multiple_spawn() {
    use core_async::task;
    
    let handles: Vec<_> = (0..5)
        .map(|i| task::spawn(async move { i * i }))
        .collect();
    
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    assert_eq!(results, vec![0, 1, 4, 9, 16]);
}

// ============================================================================
// Broadcast Channel Tests (Waker-Based, No Spin Loops)
// ============================================================================

#[wasm_bindgen_test]
async fn test_broadcast_basic() {
    use core_async::sync::broadcast;
    
    let (tx, mut rx) = broadcast::channel(10);
    
    // Send a message
    tx.send(42).unwrap();
    
    // Receive should resolve immediately
    let result = rx.recv().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[wasm_bindgen_test]
async fn test_broadcast_multiple_receivers() {
    use core_async::sync::broadcast;
    
    let (tx, mut rx1) = broadcast::channel(10);
    let mut rx2 = tx.subscribe();
    let mut rx3 = tx.subscribe();
    
    // Send a message
    tx.send(100).unwrap();
    
    // All receivers should get the message
    assert_eq!(rx1.recv().await.unwrap(), 100);
    assert_eq!(rx2.recv().await.unwrap(), 100);
    assert_eq!(rx3.recv().await.unwrap(), 100);
}

#[wasm_bindgen_test]
async fn test_broadcast_lag_detection() {
    use core_async::sync::broadcast;
    
    let (tx, mut rx) = broadcast::channel(3); // Small buffer
    
    // Fill buffer beyond capacity
    for i in 0..5 {
        tx.send(i).unwrap();
    }
    
    // Receiver should detect lag
    let result = rx.recv().await;
    assert!(result.is_err());
    match result {
        Err(core_async::sync::broadcast::RecvError::Lagged(n)) => {
            assert_eq!(n, 2); // Missed first 2 messages
        }
        _ => panic!("Expected Lagged error"),
    }
    
    // Can still receive remaining messages
    assert_eq!(rx.recv().await.unwrap(), 2);
    assert_eq!(rx.recv().await.unwrap(), 3);
    assert_eq!(rx.recv().await.unwrap(), 4);
}

#[wasm_bindgen_test]
async fn test_broadcast_await_message() {
    use core_async::sync::broadcast;
    use core_async::task;
    use core_async::time;
    
    let (tx, mut rx) = broadcast::channel(10);
    
    // Spawn task that sends after delay
    task::spawn(async move {
        time::sleep(time::Duration::from_millis(50)).await;
        tx.send(999).unwrap();
    });
    
    // Receiver should wait (no spin loop!) and get message
    let result = rx.recv().await;
    assert_eq!(result.unwrap(), 999);
}

#[wasm_bindgen_test]
async fn test_broadcast_channel_closure() {
    use core_async::sync::broadcast;
    
    let (tx, mut rx) = broadcast::channel::<i32>(10);
    
    // Drop sender to close channel
    drop(tx);
    
    // Receiver should get Closed error
    let result = rx.recv().await;
    assert!(result.is_err());
    match result {
        Err(core_async::sync::broadcast::RecvError::Closed) => {}
        _ => panic!("Expected Closed error"),
    }
}

#[wasm_bindgen_test]
async fn test_broadcast_concurrent_publishers() {
    use core_async::sync::broadcast;
    use core_async::task;
    
    let (tx, mut rx) = broadcast::channel(20);
    
    // Spawn multiple publishers
    for i in 0..5 {
        let tx_clone = tx.clone();
        task::spawn(async move {
            tx_clone.send(i).unwrap();
        });
    }
    
    // Collect messages (order not guaranteed)
    let mut received = Vec::new();
    for _ in 0..5 {
        if let Ok(val) = rx.recv().await {
            received.push(val);
        }
    }
    
    // Should have received all 5 messages
    assert_eq!(received.len(), 5);
    received.sort();
    assert_eq!(received, vec![0, 1, 2, 3, 4]);
}

#[wasm_bindgen_test]
async fn test_broadcast_try_recv() {
    use core_async::sync::broadcast;
    
    let (tx, mut rx) = broadcast::channel(10);
    
    // No messages yet
    let result = rx.try_recv();
    assert!(matches!(
        result,
        Err(core_async::sync::broadcast::TryRecvError::Empty)
    ));
    
    // Send message
    tx.send(42).unwrap();
    
    // try_recv should succeed immediately
    let result = rx.try_recv();
    assert_eq!(result.unwrap(), 42);
}

#[wasm_bindgen_test]
async fn test_broadcast_receiver_count() {
    use core_async::sync::broadcast;
    
    let (tx, _rx1) = broadcast::channel::<i32>(10);
    assert_eq!(tx.receiver_count(), 1);
    
    let _rx2 = tx.subscribe();
    assert_eq!(tx.receiver_count(), 2);
    
    let _rx3 = tx.subscribe();
    assert_eq!(tx.receiver_count(), 3);
    
    drop(_rx2);
    assert_eq!(tx.receiver_count(), 2);
}
