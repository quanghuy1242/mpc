//! Integration tests for core-async on native platforms.
//!
//! These tests verify that the async abstraction works correctly with Tokio.

use core_async::{sync, task, time};
use std::sync::Arc;

#[core_async::test]
async fn test_task_spawn() {
    let handle = task::spawn(async { 42 });
    let result = handle.await.unwrap();
    assert_eq!(result, 42);
}

#[core_async::test]
async fn test_task_spawn_blocking() {
    let handle = task::spawn_blocking(|| {
        // Simulate CPU-intensive work
        std::thread::sleep(std::time::Duration::from_millis(10));
        100
    });
    let result = handle.await.unwrap();
    assert_eq!(result, 100);
}

#[core_async::test]
async fn test_sleep() {
    let start = time::Instant::now();
    time::sleep(time::Duration::from_millis(50)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= time::Duration::from_millis(50));
    assert!(elapsed < time::Duration::from_millis(150)); // Allow some slack
}

#[core_async::test]
async fn test_timeout_success() {
    let result = time::timeout(time::Duration::from_millis(100), async {
        time::sleep(time::Duration::from_millis(10)).await;
        42
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}

#[core_async::test]
async fn test_timeout_failure() {
    let result = time::timeout(time::Duration::from_millis(10), async {
        time::sleep(time::Duration::from_millis(100)).await;
        42
    })
    .await;

    assert!(result.is_err());
}

#[core_async::test]
async fn test_instant_elapsed() {
    let start = time::Instant::now();
    time::sleep(time::Duration::from_millis(50)).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= time::Duration::from_millis(50));
}

#[core_async::test]
async fn test_mutex() {
    let mutex = Arc::new(sync::Mutex::new(0));
    let mutex_clone = mutex.clone();

    let handle = task::spawn(async move {
        let mut guard = mutex_clone.lock().await;
        *guard += 1;
    });

    handle.await.unwrap();

    let guard = mutex.lock().await;
    assert_eq!(*guard, 1);
}

#[core_async::test]
async fn test_rwlock() {
    let rwlock = Arc::new(sync::RwLock::new(vec![1, 2, 3]));

    // Multiple readers can access simultaneously
    let rwlock_clone1 = rwlock.clone();
    let rwlock_clone2 = rwlock.clone();

    let handle1 = task::spawn(async move {
        let guard = rwlock_clone1.read().await;
        guard.len()
    });

    let handle2 = task::spawn(async move {
        let guard = rwlock_clone2.read().await;
        guard.len()
    });

    let len1 = handle1.await.unwrap();
    let len2 = handle2.await.unwrap();

    assert_eq!(len1, 3);
    assert_eq!(len2, 3);

    // Writer gets exclusive access
    let mut guard = rwlock.write().await;
    guard.push(4);
    assert_eq!(guard.len(), 4);
}

#[core_async::test]
async fn test_mpsc_channel() {
    let (tx, mut rx) = sync::mpsc::channel(10);

    task::spawn(async move {
        for i in 0..5 {
            tx.send(i).await.unwrap();
        }
    });

    let mut sum = 0;
    while let Some(value) = rx.recv().await {
        sum += value;
        if sum >= 10 {
            break;
        }
    }

    assert_eq!(sum, 10); // 0 + 1 + 2 + 3 + 4 = 10
}

#[core_async::test]
async fn test_oneshot_channel() {
    let (tx, rx) = sync::oneshot::channel();

    task::spawn(async move {
        time::sleep(time::Duration::from_millis(10)).await;
        tx.send(42).unwrap();
    });

    let result = rx.await.unwrap();
    assert_eq!(result, 42);
}

#[core_async::test]
async fn test_notify() {
    let notify = Arc::new(sync::Notify::new());
    let notify_clone = notify.clone();

    let handle = task::spawn(async move {
        notify_clone.notified().await;
        "notified"
    });

    time::sleep(time::Duration::from_millis(10)).await;
    notify.notify_one();

    let result = handle.await.unwrap();
    assert_eq!(result, "notified");
}

#[core_async::test]
async fn test_interval() {
    let mut interval = time::interval(time::Duration::from_millis(10));

    let start = time::Instant::now();

    // Skip the first tick (immediate)
    interval.tick().await;

    // Wait for 3 ticks
    for _ in 0..3 {
        interval.tick().await;
    }

    let elapsed = start.elapsed();
    // Should be at least 30ms (3 ticks * 10ms)
    assert!(elapsed >= time::Duration::from_millis(30));
}

#[core_async::test]
async fn test_yield_now() {
    // Just verify it compiles and runs
    task::yield_now().await;
}

#[core_async::test]
async fn test_concurrent_task_execution() {
    let counter = Arc::new(sync::Mutex::new(0));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter_clone = counter.clone();
        let handle = task::spawn(async move {
            let mut guard = counter_clone.lock().await;
            *guard += 1;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let final_count = *counter.lock().await;
    assert_eq!(final_count, 10);
}

#[core_async::test]
async fn test_time_utilities() {
    let now_millis = time::now_millis();
    let now_secs = time::now_secs();

    assert!(now_millis > 0);
    assert!(now_secs > 0);
    assert!(now_millis / 1000 >= now_secs - 1); // Allow 1 second of slack
}

#[core_async::test]
async fn test_broadcast_channel() {
    let (tx, mut rx1) = sync::broadcast::channel(10);
    let mut rx2 = tx.subscribe();

    task::spawn(async move {
        for i in 0..5 {
            tx.send(i).unwrap();
        }
    });

    let mut values1 = vec![];
    let mut values2 = vec![];

    for _ in 0..5 {
        values1.push(rx1.recv().await.unwrap());
        values2.push(rx2.recv().await.unwrap());
    }

    assert_eq!(values1, vec![0, 1, 2, 3, 4]);
    assert_eq!(values2, vec![0, 1, 2, 3, 4]);
}

#[core_async::test]
async fn test_watch_channel() {
    let (tx, mut rx) = sync::watch::channel(0);

    task::spawn(async move {
        for i in 1..=5 {
            time::sleep(time::Duration::from_millis(10)).await;
            tx.send(i).unwrap();
        }
    });

    let mut last_value = 0;
    while rx.changed().await.is_ok() {
        last_value = *rx.borrow();
        if last_value >= 5 {
            break;
        }
    }

    assert_eq!(last_value, 5);
}
