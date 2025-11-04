//! Background Task Execution Implementation

use async_trait::async_trait;
use bridge_traits::{
    background::{
        BackgroundExecutor, LifecycleChangeStream, LifecycleObserver, LifecycleState,
        TaskConstraints, TaskId, TaskStatus,
    },
    error::{BridgeError, Result},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Tokio-based background executor for desktop
///
/// Provides task scheduling using:
/// - Tokio runtime for async execution
/// - Simple in-memory task tracking
/// - No platform constraints (desktop always has resources)
pub struct TokioBackgroundExecutor {
    tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
}

struct TaskInfo {
    status: TaskStatus,
    _handle: Option<tokio::task::JoinHandle<()>>,
}

impl TokioBackgroundExecutor {
    /// Create a new background executor
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
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

        // Note: This is a simplified implementation
        // A production version would:
        // 1. Actually execute user-defined task functions
        // 2. Persist task state across restarts
        // 3. Respect constraints more thoroughly

        if constraints.requires_network || constraints.requires_wifi {
            warn!(
                task_id = task_id,
                "Network constraints not fully implemented for desktop"
            );
        }

        let mut tasks = self.tasks.write().await;
        tasks.insert(
            id.clone(),
            TaskInfo {
                status: TaskStatus::Scheduled,
                _handle: None,
            },
        );

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

        if constraints.requires_network || constraints.requires_wifi {
            warn!(
                task_id = task_id,
                "Network constraints not fully implemented for desktop"
            );
        }

        let mut tasks = self.tasks.write().await;
        tasks.insert(
            id.clone(),
            TaskInfo {
                status: TaskStatus::Scheduled,
                _handle: None,
            },
        );

        Ok(id)
    }

    async fn cancel_task(&self, task_id: &TaskId) -> Result<()> {
        debug!(task_id = ?task_id, "Cancelling task");

        let mut tasks = self.tasks.write().await;

        if let Some(mut task_info) = tasks.remove(task_id) {
            task_info.status = TaskStatus::Cancelled;

            // Abort the task if it has a handle
            if let Some(handle) = task_info._handle {
                handle.abort();
            }

            Ok(())
        } else {
            Err(BridgeError::OperationFailed(format!(
                "Task not found: {:?}",
                task_id
            )))
        }
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
        // Always available on desktop
        true
    }

    async fn next_execution_time(&self, task_id: &TaskId) -> Result<Option<Duration>> {
        let tasks = self.tasks.read().await;

        if tasks.contains_key(task_id) {
            // For desktop, tasks typically execute immediately or on schedule
            // Return None to indicate immediate execution
            Ok(None)
        } else {
            Err(BridgeError::OperationFailed(format!(
                "Task not found: {:?}",
                task_id
            )))
        }
    }
}

/// Desktop lifecycle observer (no-op implementation)
///
/// Desktop applications don't have the same lifecycle constraints as mobile apps.
/// They're essentially always in the foreground state from the core's perspective.
pub struct DesktopLifecycleObserver;

impl DesktopLifecycleObserver {
    /// Create a new lifecycle observer
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
        // Desktop apps are always in foreground from core's perspective
        Ok(LifecycleState::Foreground)
    }

    async fn subscribe_changes(&self) -> Result<Box<dyn LifecycleChangeStream>> {
        // Return a stream that never emits changes
        Ok(Box::new(DesktopLifecycleChangeStream))
    }
}

/// Desktop lifecycle change stream (never emits)
struct DesktopLifecycleChangeStream;

#[async_trait]
impl LifecycleChangeStream for DesktopLifecycleChangeStream {
    async fn next(&mut self) -> Option<LifecycleState> {
        // Desktop apps don't typically transition between states
        // This would block indefinitely, which is correct behavior
        // In practice, you'd use tokio::select! with other operations
        std::future::pending::<()>().await;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_background_executor_creation() {
        let executor = TokioBackgroundExecutor::new();
        assert!(executor.is_available().await);
    }

    #[tokio::test]
    async fn test_schedule_task() {
        let executor = TokioBackgroundExecutor::new();

        let task_id = executor
            .schedule_task(
                "test-task",
                Duration::from_secs(60),
                TaskConstraints::default(),
            )
            .await
            .unwrap();

        let status = executor.get_task_status(&task_id).await.unwrap();
        assert_eq!(status, TaskStatus::Scheduled);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let executor = TokioBackgroundExecutor::new();

        let task_id = executor
            .schedule_task(
                "test-task",
                Duration::from_secs(60),
                TaskConstraints::default(),
            )
            .await
            .unwrap();

        executor.cancel_task(&task_id).await.unwrap();

        // Task should be removed
        assert!(executor.get_task_status(&task_id).await.is_err());
    }

    #[tokio::test]
    async fn test_lifecycle_observer() {
        let observer = DesktopLifecycleObserver::new();
        let state = observer.get_state().await.unwrap();
        assert_eq!(state, LifecycleState::Foreground);
    }
}
