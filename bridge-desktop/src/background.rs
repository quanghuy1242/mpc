//! Background Task Execution Implementation

use async_trait::async_trait;
use bridge_traits::{
    background::{
        BackgroundExecutor, LifecycleChangeStream, LifecycleObserver, LifecycleState,
        TaskConstraints, TaskId, TaskStatus,
    },
    error::{BridgeError, Result},
    network::{NetworkInfo, NetworkMonitor, NetworkStatus, NetworkType},
    time::{Clock, SystemClock},
};
use core_async::sync::{oneshot, RwLock};
use core_async::task::JoinHandle;
use core_async::time::sleep;
use futures_util::{future::BoxFuture, FutureExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

type TaskHandler = Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// Tokio-based background executor for desktop.
pub struct TokioBackgroundExecutor {
    tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
    handlers: Arc<RwLock<HashMap<String, TaskHandler>>>,
    network_monitor: Option<Arc<dyn NetworkMonitor>>,
    clock: Arc<dyn Clock>,
}

struct TaskInfo {
    status: TaskStatus,
    handle: Option<JoinHandle<()>>,
    cancel: Option<oneshot::Sender<()>>,
    last_run: Option<i64>,
    next_run: Option<i64>,
}

impl TokioBackgroundExecutor {
    /// Create a new background executor with no network monitoring.
    pub fn new() -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Self::with_network_monitor_and_clock(None, clock)
    }

    /// Create a background executor with an optional network monitor.
    pub fn with_network_monitor(monitor: Option<Arc<dyn NetworkMonitor>>) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Self::with_network_monitor_and_clock(monitor, clock)
    }

    /// Create a background executor with an optional network monitor and custom clock.
    pub fn with_network_monitor_and_clock(
        monitor: Option<Arc<dyn NetworkMonitor>>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            network_monitor: monitor,
            clock,
        }
    }

    fn now_millis(clock: &dyn Clock) -> i64 {
        clock.unix_timestamp_millis()
    }

    fn duration_to_millis(duration: Duration) -> i64 {
        duration.as_millis().min(i64::MAX as u128) as i64
    }

    fn schedule_after(clock: &dyn Clock, delay: Duration) -> i64 {
        let now = Self::now_millis(clock);
        now.saturating_add(Self::duration_to_millis(delay))
    }

    fn millis_to_duration(millis: i64) -> Duration {
        if millis <= 0 {
            Duration::from_secs(0)
        } else {
            Duration::from_millis(millis as u64)
        }
    }

    /// Register a handler that will be invoked when the task executes.
    pub async fn register_task_handler<F, Fut>(&self, task_id: &str, handler: F) -> Result<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let mut handlers = self.handlers.write().await;
        handlers.insert(task_id.to_string(), Arc::new(move || handler().boxed()));
        Ok(())
    }

    async fn handler_for(&self, task_id: &str) -> Option<TaskHandler> {
        let handlers = self.handlers.read().await;
        handlers.get(task_id).cloned()
    }

    async fn insert_task(&self, id: TaskId, info: TaskInfo) {
        let mut tasks = self.tasks.write().await;
        tasks.insert(id, info);
    }

    async fn update_task<F>(&self, id: &TaskId, update: F)
    where
        F: FnOnce(&mut TaskInfo),
    {
        let mut tasks = self.tasks.write().await;
        if let Some(info) = tasks.get_mut(id) {
            update(info);
        }
    }

    async fn remove_task(&self, id: &TaskId) -> Option<TaskInfo> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(id)
    }

    async fn constraints_satisfied(
        monitor: Option<Arc<dyn NetworkMonitor>>,
        constraints: &TaskConstraints,
    ) -> bool {
        if !(constraints.requires_network || constraints.requires_wifi) {
            return true;
        }

        if let Some(monitor) = monitor {
            match monitor.get_network_info().await {
                Ok(NetworkInfo {
                    status: NetworkStatus::Connected,
                    network_type,
                    ..
                }) => {
                    if constraints.requires_wifi {
                        matches!(network_type, Some(NetworkType::WiFi))
                    } else {
                        true
                    }
                }
                Ok(_) => false,
                Err(err) => {
                    warn!("Network monitor error: {}", err);
                    false
                }
            }
        } else {
            warn!(
                "Network constraints requested but no monitor provided; assuming constraint satisfied"
            );
            true
        }
    }

    async fn run_recurring_task(
        tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
        id: TaskId,
        handler: TaskHandler,
        period: Duration,
        constraints: TaskConstraints,
        mut cancel_rx: oneshot::Receiver<()>,
        monitor: Option<Arc<dyn NetworkMonitor>>,
        clock: Arc<dyn Clock>,
    ) {
        let mut ticker = tokio::time::interval(period);
        let period_millis = Self::duration_to_millis(period);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    let mut tasks = tasks.write().await;
                    if let Some(info) = tasks.get_mut(&id) {
                        info.status = TaskStatus::Cancelled;
                        info.next_run = None;
                    }
                    break;
                }
                _ = ticker.tick() => {
                    if !Self::constraints_satisfied(monitor.clone(), &constraints).await {
                        debug!(task_id = %id.0, "Constraints not satisfied; skipping run");
                        let mut tasks = tasks.write().await;
                        if let Some(info) = tasks.get_mut(&id) {
                            info.next_run = Some(Self::now_millis(clock.as_ref()).saturating_add(period_millis));
                        }
                        continue;
                    }

                    {
                        let mut tasks = tasks.write().await;
                        if let Some(info) = tasks.get_mut(&id) {
                            info.status = TaskStatus::Running;
                        }
                    }

                    let result = handler().await;

                    let mut tasks = tasks.write().await;
                    if let Some(info) = tasks.get_mut(&id) {
                        let now = Self::now_millis(clock.as_ref());
                        info.last_run = Some(now);
                        info.next_run = Some(now.saturating_add(period_millis));
                        info.status = match result {
                            Ok(()) => TaskStatus::Completed,
                            Err(err) => {
                                warn!(task_id = %id.0, error = %err, "Recurring task failed");
                                TaskStatus::Failed
                            }
                        };
                    }
                }
            }
        }
    }

    async fn run_one_time_task(
        tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
        id: TaskId,
        handler: TaskHandler,
        delay: Duration,
        constraints: TaskConstraints,
        mut cancel_rx: oneshot::Receiver<()>,
        monitor: Option<Arc<dyn NetworkMonitor>>,
        clock: Arc<dyn Clock>,
    ) {
        let delay_sleep = sleep(delay);
        tokio::pin!(delay_sleep);
        tokio::select! {
            _ = &mut cancel_rx => {
                let mut tasks = tasks.write().await;
                if let Some(info) = tasks.get_mut(&id) {
                    info.status = TaskStatus::Cancelled;
                    info.next_run = None;
                }
                return;
            }
            _ = delay_sleep.as_mut() => {}
        }

        loop {
            if Self::constraints_satisfied(monitor.clone(), &constraints).await {
                break;
            }

            let retry_sleep = sleep(Duration::from_secs(5));
            tokio::pin!(retry_sleep);
            tokio::select! {
                _ = &mut cancel_rx => {
                    let mut tasks = tasks.write().await;
                    if let Some(info) = tasks.get_mut(&id) {
                        info.status = TaskStatus::Cancelled;
                        info.next_run = None;
                    }
                    return;
                }
                _ = retry_sleep.as_mut() => {}
            }
        }

        {
            let mut tasks = tasks.write().await;
            if let Some(info) = tasks.get_mut(&id) {
                info.status = TaskStatus::Running;
            }
        }

        let result = handler().await;

        let mut tasks = tasks.write().await;
        if let Some(info) = tasks.get_mut(&id) {
            info.last_run = Some(Self::now_millis(clock.as_ref()));
            info.next_run = None;
            info.status = match result {
                Ok(()) => TaskStatus::Completed,
                Err(err) => {
                    warn!(task_id = %id.0, error = %err, "One-time task failed");
                    TaskStatus::Failed
                }
            };
        }
    }
}

impl Default for TokioBackgroundExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BackgroundExecutor for TokioBackgroundExecutor {
    async fn schedule_task(
        &self,
        task_id: &str,
        interval: Duration,
        constraints: TaskConstraints,
    ) -> Result<TaskId> {
        let id = TaskId::new(task_id);

        debug!(
            task_id = task_id,
            interval_secs = interval.as_secs(),
            "Scheduling recurring task"
        );

        let handler = self.handler_for(task_id).await.ok_or_else(|| {
            BridgeError::OperationFailed(format!("No handler registered for task: {}", task_id))
        })?;
        let (cancel_tx, cancel_rx) = oneshot::channel();

        self.insert_task(
            id.clone(),
            TaskInfo {
                status: TaskStatus::Scheduled,
                handle: None,
                cancel: Some(cancel_tx),
                last_run: None,
                next_run: Some(Self::now_millis(self.clock.as_ref())),
            },
        )
        .await;

        let tasks = Arc::clone(&self.tasks);
        let handler_clone = handler.clone();
        let monitor = self.network_monitor.clone();
        let task_id_clone = TaskId::new(task_id);
        let constraints_clone = constraints.clone();
        let clock = Arc::clone(&self.clock);

        let handle = tokio::spawn(async move {
            TokioBackgroundExecutor::run_recurring_task(
                tasks,
                task_id_clone,
                handler_clone,
                interval,
                constraints_clone,
                cancel_rx,
                monitor,
                clock,
            )
            .await;
        });

        self.update_task(&id, |info| {
            info.handle = Some(handle);
        })
        .await;

        Ok(id)
    }

    async fn schedule_once(
        &self,
        task_id: &str,
        delay: Duration,
        constraints: TaskConstraints,
    ) -> Result<TaskId> {
        let id = TaskId::new(task_id);

        debug!(
            task_id = task_id,
            delay_secs = delay.as_secs(),
            "Scheduling one-time task"
        );

        let handler = self.handler_for(task_id).await.ok_or_else(|| {
            BridgeError::OperationFailed(format!("No handler registered for task: {}", task_id))
        })?;
        let (cancel_tx, cancel_rx) = oneshot::channel();

        self.insert_task(
            id.clone(),
            TaskInfo {
                status: TaskStatus::Scheduled,
                handle: None,
                cancel: Some(cancel_tx),
                last_run: None,
                next_run: Some(Self::schedule_after(self.clock.as_ref(), delay)),
            },
        )
        .await;

        let tasks = Arc::clone(&self.tasks);
        let handler_clone = handler.clone();
        let monitor = self.network_monitor.clone();
        let task_id_clone = TaskId::new(task_id);
        let constraints_clone = constraints.clone();
        let clock = Arc::clone(&self.clock);

        let handle = tokio::spawn(async move {
            TokioBackgroundExecutor::run_one_time_task(
                tasks,
                task_id_clone,
                handler_clone,
                delay,
                constraints_clone,
                cancel_rx,
                monitor,
                clock,
            )
            .await;
        });

        self.update_task(&id, |info| {
            info.handle = Some(handle);
        })
        .await;

        Ok(id)
    }

    async fn cancel_task(&self, task_id: &TaskId) -> Result<()> {
        debug!(task_id = ?task_id, "Cancelling task");

        if let Some(mut info) = self.remove_task(task_id).await {
            if let Some(cancel) = info.cancel.take() {
                let _ = cancel.send(());
            }
            if let Some(handle) = info.handle.take() {
                handle.abort();
            }
            return Ok(());
        }

        Err(BridgeError::OperationFailed(format!(
            "Task not found: {:?}",
            task_id
        )))
    }

    async fn get_task_status(&self, task_id: &TaskId) -> Result<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks
            .get(task_id)
            .map(|info| info.status.clone())
            .ok_or_else(|| BridgeError::OperationFailed(format!("Task not found: {:?}", task_id)))
    }

    async fn list_tasks(&self) -> Result<Vec<TaskId>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.keys().cloned().collect())
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn next_execution_time(&self, task_id: &TaskId) -> Result<Option<Duration>> {
        let tasks = self.tasks.read().await;
        if let Some(info) = tasks.get(task_id) {
            if let Some(next) = info.next_run {
                let now = Self::now_millis(self.clock.as_ref());
                let remaining = next - now;
                Ok(Some(Self::millis_to_duration(remaining)))
            } else {
                Ok(None)
            }
        } else {
            Err(BridgeError::OperationFailed(format!(
                "Task not found: {:?}",
                task_id
            )))
        }
    }
}

/// Desktop lifecycle observer (no-op implementation).
pub struct DesktopLifecycleObserver;

impl DesktopLifecycleObserver {
    /// Create a new lifecycle observer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DesktopLifecycleObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LifecycleObserver for DesktopLifecycleObserver {
    async fn get_state(&self) -> Result<LifecycleState> {
        Ok(LifecycleState::Foreground)
    }

    async fn subscribe_changes(&self) -> Result<Box<dyn LifecycleChangeStream>> {
        Ok(Box::new(DesktopLifecycleChangeStream))
    }
}

/// Desktop lifecycle change stream (never emits).
struct DesktopLifecycleChangeStream;

#[async_trait]
impl LifecycleChangeStream for DesktopLifecycleChangeStream {
    async fn next(&mut self) -> Option<LifecycleState> {
        std::future::pending::<()>().await;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_traits::error::BridgeError;
    use bridge_traits::network::NetworkChangeStream;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[core_async::test]
    async fn test_background_executor_creation() {
        let executor = TokioBackgroundExecutor::new();
        assert!(executor.is_available().await);
    }

    #[core_async::test]
    async fn test_schedule_task_runs_handler() {
        let executor = TokioBackgroundExecutor::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        executor
            .register_task_handler("test", move || {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await
            .unwrap();

        let task_id = executor
            .schedule_task(
                "test",
                Duration::from_millis(30),
                TaskConstraints::default(),
            )
            .await
            .unwrap();

        sleep(Duration::from_millis(120)).await;

        assert!(executor.get_task_status(&task_id).await.unwrap() != TaskStatus::Cancelled);
        assert!(counter.load(Ordering::SeqCst) >= 2);

        executor.cancel_task(&task_id).await.unwrap();
    }

    #[core_async::test]
    async fn test_schedule_once_executes() {
        let executor = TokioBackgroundExecutor::new();
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);

        executor
            .register_task_handler("once", move || {
                let flag = Arc::clone(&flag_clone);
                async move {
                    flag.store(true, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await
            .unwrap();

        let task_id = executor
            .schedule_once(
                "once",
                Duration::from_millis(25),
                TaskConstraints::default(),
            )
            .await
            .unwrap();

        sleep(Duration::from_millis(100)).await;

        assert_eq!(
            executor.get_task_status(&task_id).await.unwrap(),
            TaskStatus::Completed
        );
        assert!(flag.load(Ordering::SeqCst));
    }

    #[core_async::test]
    async fn test_cancel_task() {
        let executor = TokioBackgroundExecutor::new();
        executor
            .register_task_handler("cancel", || async { Ok(()) })
            .await
            .unwrap();

        let task_id = executor
            .schedule_task("cancel", Duration::from_secs(1), TaskConstraints::default())
            .await
            .unwrap();

        executor.cancel_task(&task_id).await.unwrap();
        assert!(executor.get_task_status(&task_id).await.is_err());
    }

    #[core_async::test]
    async fn test_network_constraints() {
        let connected = Arc::new(AtomicBool::new(false));
        let monitor =
            Arc::new(TestNetworkMonitor::new(Arc::clone(&connected))) as Arc<dyn NetworkMonitor>;
        let executor = TokioBackgroundExecutor::with_network_monitor(Some(monitor));

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        executor
            .register_task_handler("network", move || {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await
            .unwrap();

        let task_id = executor
            .schedule_task(
                "network",
                Duration::from_millis(40),
                TaskConstraints {
                    requires_wifi: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        sleep(Duration::from_millis(150)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        connected.store(true, Ordering::SeqCst);
        sleep(Duration::from_millis(150)).await;
        assert!(counter.load(Ordering::SeqCst) >= 1);

        executor.cancel_task(&task_id).await.unwrap();
    }

    #[core_async::test]
    async fn test_lifecycle_observer() {
        let observer = DesktopLifecycleObserver::new();
        assert_eq!(
            observer.get_state().await.unwrap(),
            LifecycleState::Foreground
        );
    }

    #[derive(Clone)]
    struct TestNetworkMonitor {
        connected: Arc<AtomicBool>,
    }

    impl TestNetworkMonitor {
        fn new(connected: Arc<AtomicBool>) -> Self {
            Self { connected }
        }
    }

    #[async_trait]
    impl NetworkMonitor for TestNetworkMonitor {
        async fn get_network_info(&self) -> Result<NetworkInfo> {
            if self.connected.load(Ordering::SeqCst) {
                Ok(NetworkInfo {
                    status: NetworkStatus::Connected,
                    network_type: Some(NetworkType::WiFi),
                    is_metered: false,
                    is_expensive: false,
                })
            } else {
                Ok(NetworkInfo {
                    status: NetworkStatus::Disconnected,
                    network_type: None,
                    is_metered: false,
                    is_expensive: false,
                })
            }
        }

        async fn subscribe_changes(&self) -> Result<Box<dyn NetworkChangeStream>> {
            Err(BridgeError::NotAvailable(
                "Change stream not supported in test monitor".into(),
            ))
        }
    }
}
