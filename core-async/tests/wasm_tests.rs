//! Integration tests for core-async on WASM platforms.
//!
//! These tests verify that the async abstraction works correctly in a WASM environment.

#![cfg(target_arch = "wasm32")]

use core_async::{sync, task, time};
use futures::stream::TryStreamExt;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_task_spawn() {
    // On WASM, spawn doesn't return a handle, so we test it differently
    task::spawn(async {
        // Task executes
    });
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
    let (mut tx, mut rx) = sync::mpsc::channel(10);

    task::spawn(async move {
        for i in 0..5 {
            tx.try_send(i).unwrap();
        }
    });

    time::sleep(time::Duration::from_millis(50)).await;

    let mut sum = 0;
    while let Ok(Some(value)) = rx.try_next() {
        sum += value;
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

#[wasm_bindgen_test]
async fn test_time_utilities() {
    let now_millis = time::now_millis();
    let now_secs = time::now_secs();

    assert!(now_millis > 0);
    assert!(now_secs > 0);
    assert!(now_millis / 1000 >= now_secs - 1);
}

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
