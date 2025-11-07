//! Background Execution and Task Scheduling
//!
//! Provides platform-aware background task scheduling.

use std::time::Duration;

use crate::{
    error::Result,
    platform::{PlatformSend, PlatformSendSync},
};

/// Task execution constraints
#[derive(Debug, Clone)]
pub struct TaskConstraints {
    /// Require WiFi connection
    pub requires_wifi: bool,
    /// Require any network connection
    pub requires_network: bool,
    /// Require device to be charging
    pub requires_charging: bool,
    /// Require device to be idle
    pub requires_idle: bool,
}

impl Default for TaskConstraints {
    fn default() -> Self {
        Self {
            requires_wifi: false,
            requires_network: true,
            requires_charging: false,
            requires_idle: false,
        }
    }
}

/// Task scheduling priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
}

/// Scheduled task identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Task execution status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task is scheduled but not yet running
    Scheduled,
    /// Task is currently executing
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Background task executor trait
///
/// Abstracts platform-specific background task scheduling:
/// - **iOS**: BGTaskScheduler (requires user opt-in)
/// - **Android**: WorkManager (respects Doze mode)
/// - **Desktop**: System scheduler (cron, launchd, Task Scheduler) or daemon
/// - **Web**: Service Worker (limited, requires user interaction)
///
/// # Platform Constraints
///
/// Different platforms have different limitations:
/// - iOS: 30-second execution windows, must refresh periodically
/// - Android: Deferred under Doze/Idle mode based on constraints
/// - Web: No persistent background execution, relies on user-triggered actions
///
/// # Example
///
/// ```ignore
/// use bridge_traits::background::{BackgroundExecutor, TaskConstraints};
/// use std::time::Duration;
///
/// async fn schedule_sync(executor: &dyn BackgroundExecutor) -> Result<()> {
///     let constraints = TaskConstraints {
///         requires_wifi: true,
///         ..Default::default()
///     };
///     
///     executor.schedule_task(
///         "incremental_sync",
///         Duration::from_secs(3600),
///         constraints,
///     ).await?;
///     Ok(())
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait BackgroundExecutor: PlatformSendSync {
    /// Schedule a recurring task
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier for the task
    /// * `interval` - How often to run the task
    /// * `constraints` - Execution constraints (network, charging, etc.)
    ///
    /// # Platform Notes
    ///
    /// - iOS: Uses BGAppRefreshTask, actual execution timing is system-determined
    /// - Android: Uses WorkManager with specified constraints
    /// - Web: May fall back to requiring user-initiated sync
    async fn schedule_task(
        &self,
        task_id: &str,
        interval: Duration,
        constraints: TaskConstraints,
    ) -> Result<TaskId>;

    /// Schedule a one-time delayed task
    async fn schedule_once(
        &self,
        task_id: &str,
        delay: Duration,
        constraints: TaskConstraints,
    ) -> Result<TaskId>;

    /// Cancel a scheduled task
    async fn cancel_task(&self, task_id: &TaskId) -> Result<()>;

    /// Get status of a task
    async fn get_task_status(&self, task_id: &TaskId) -> Result<TaskStatus>;

    /// List all scheduled tasks
    async fn list_tasks(&self) -> Result<Vec<TaskId>>;

    /// Check if background execution is available
    ///
    /// Some platforms (especially web) may not support background execution.
    async fn is_available(&self) -> bool {
        true
    }

    /// Get estimated time until next execution window
    ///
    /// Returns `None` if the information is not available or if the task
    /// will execute immediately.
    async fn next_execution_time(&self, task_id: &TaskId) -> Result<Option<Duration>>;
}

/// Lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// Application is in the foreground and active
    Foreground,
    /// Application is in the background
    Background,
    /// Application is being suspended
    Suspended,
}

/// Lifecycle observer trait
///
/// Notifies the core about app lifecycle transitions so it can:
/// - Pause expensive operations when backgrounded
/// - Release resources before suspension
/// - Resume operations when foregrounded
///
/// # Platform Support
///
/// - **iOS**: UIApplication lifecycle notifications
/// - **Android**: Activity/Application lifecycle callbacks
/// - **Desktop**: Window focus/minimize events (less critical)
/// - **Web**: Page Visibility API
///
/// # Example
///
/// ```ignore
/// use bridge_traits::background::{LifecycleObserver, LifecycleState};
///
/// async fn setup_lifecycle(observer: &dyn LifecycleObserver) -> Result<()> {
///     let mut stream = observer.subscribe_changes().await?;
///     
///     while let Some(state) = stream.next().await {
///         match state {
///             LifecycleState::Background => pause_sync(),
///             LifecycleState::Foreground => resume_sync(),
///             _ => {}
///         }
///     }
///     Ok(())
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait LifecycleObserver: PlatformSendSync {
    /// Get current lifecycle state
    async fn get_state(&self) -> Result<LifecycleState>;

    /// Subscribe to lifecycle state changes
    async fn subscribe_changes(&self) -> Result<Box<dyn LifecycleChangeStream>>;
}

/// Stream of lifecycle state changes
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait LifecycleChangeStream: PlatformSend {
    /// Get the next lifecycle state update
    ///
    /// Returns `None` when the stream is closed.
    async fn next(&mut self) -> Option<LifecycleState>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_constraints() {
        let constraints = TaskConstraints {
            requires_wifi: true,
            ..Default::default()
        };

        assert!(constraints.requires_wifi);
        assert!(constraints.requires_network);
        assert!(!constraints.requires_charging);
    }

    #[test]
    fn test_task_id() {
        let id1 = TaskId::new("sync_job");
        let id2 = TaskId::new("sync_job");

        assert_eq!(id1, id2);
    }
}
