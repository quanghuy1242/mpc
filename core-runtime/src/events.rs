//! # Event Bus System
//!
//! Provides an event-driven architecture for the Music Platform Core using `tokio::sync::broadcast`.
//! This module enables decoupled communication between core modules through typed events.
//!
//! ## Overview
//!
//! The event bus system consists of:
//! - **Event Types**: Strongly-typed enum hierarchies for different domains
//! - **EventBus**: Central broadcast channel for publishing events
//! - **EventStream**: Wrapper for consuming events with filtering
//! - **Subscription Management**: Multiple subscribers can listen independently
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     emit      ┌───────────┐
//! │ Auth Module ├──────────────>│           │
//! └─────────────┘               │           │
//!                               │ EventBus  │
//! ┌─────────────┐     emit      │ (broadcast│     subscribe    ┌────────────┐
//! │ Sync Module ├──────────────>│  channel) ├─────────────────>│ Subscriber │
//! └─────────────┘               │           │                  └────────────┘
//!                               │           │
//! ┌─────────────┐     emit      │           │     subscribe    ┌────────────┐
//! │Library Mod  ├──────────────>│           ├─────────────────>│ Subscriber │
//! └─────────────┘               └───────────┘                  └────────────┘
//! ```
//!
//! ## Usage
//!
//! ### Creating an Event Bus
//!
//! ```rust
//! use core_runtime::events::EventBus;
//!
//! let event_bus = EventBus::new(100); // Buffer size of 100 events
//! ```
//!
//! ### Publishing Events
//!
//! ```rust
//! use core_runtime::events::{EventBus, CoreEvent, AuthEvent};
//!
//! # let event_bus = EventBus::new(100);
//! let event = CoreEvent::Auth(AuthEvent::SignedIn {
//!     profile_id: "user-123".to_string(),
//!     provider: "GoogleDrive".to_string(),
//! });
//!
//! event_bus.emit(event).ok();
//! ```
//!
//! ### Subscribing to Events
//!
//! ```rust
//! use core_runtime::events::{EventBus, CoreEvent};
//! use tokio::sync::broadcast::error::RecvError;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let event_bus = EventBus::new(100);
//! let mut stream = event_bus.subscribe();
//!
//! tokio::spawn(async move {
//!     loop {
//!         match stream.recv().await {
//!             Ok(event) => println!("Received: {:?}", event),
//!             Err(RecvError::Lagged(n)) => {
//!                 eprintln!("Missed {} events", n);
//!             }
//!             Err(RecvError::Closed) => break,
//!         }
//!     }
//! });
//! # }
//! ```
//!
//! ### Filtering Events
//!
//! ```rust
//! use core_runtime::events::{EventBus, CoreEvent, AuthEvent};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let event_bus = EventBus::new(100);
//! let mut stream = event_bus.subscribe();
//!
//! tokio::spawn(async move {
//!     loop {
//!         if let Ok(event) = stream.recv().await {
//!             // Filter for auth events only
//!             if matches!(event, CoreEvent::Auth(_)) {
//!                 println!("Auth event: {:?}", event);
//!             }
//!         }
//!     }
//! });
//! # }
//! ```
//!
//! ## Event Types
//!
//! ### Authentication Events
//! - `SignedOut`: User signed out from a profile
//! - `SigningIn`: Authentication flow in progress
//! - `SignedIn`: User successfully authenticated
//! - `TokenRefreshing`: Access token being refreshed
//! - `TokenRefreshed`: Token refresh completed
//! - `AuthError`: Authentication error occurred
//!
//! ### Sync Events
//! - `Started`: Sync job initiated
//! - `Progress`: Incremental progress update
//! - `Completed`: Sync finished successfully
//! - `Failed`: Sync encountered an error
//! - `Cancelled`: Sync was cancelled by user
//!
//! ### Library Events
//! - `TrackAdded`: New track added to library
//! - `TrackUpdated`: Track metadata updated
//! - `TrackDeleted`: Track removed from library
//! - `AlbumAdded`: New album created
//! - `PlaylistCreated`: New playlist created
//! - `PlaylistUpdated`: Playlist modified
//!
//! ### Playback Events
//! - `Started`: Playback started
//! - `Paused`: Playback paused
//! - `Resumed`: Playback resumed
//! - `Stopped`: Playback stopped
//! - `Completed`: Track finished playing
//! - `PositionChanged`: Playback position updated
//! - `Error`: Playback error occurred
//!
//! ## Error Handling
//!
//! The event bus uses `tokio::sync::broadcast`, which can produce two types of errors:
//!
//! - **`RecvError::Lagged(n)`**: Subscriber was too slow and missed `n` events.
//!   This is non-fatal; the subscriber can continue receiving new events.
//! - **`RecvError::Closed`**: All senders have been dropped. This indicates shutdown.
//!
//! Subscribers should handle `Lagged` gracefully and treat `Closed` as a signal to exit.
//!
//! ## Performance Considerations
//!
//! - **Buffer Size**: Choose an appropriate buffer size based on expected event volume.
//!   Too small causes lagging; too large wastes memory.
//! - **Slow Subscribers**: Slow subscribers receive `Lagged` errors but don't block fast ones.
//! - **Cloning**: Events are cloned for each subscriber. Keep event payloads lightweight.
//! - **Async Overhead**: Event delivery is async but very fast (microseconds).
//!
//! ## Thread Safety
//!
//! The event bus is fully thread-safe (`Send + Sync`). It can be safely shared across
//! async tasks using `Arc`:
//!
//! ```no_run
//! use std::sync::Arc;
//! use core_runtime::events::EventBus;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let event_bus = Arc::new(EventBus::new(100));
//! let bus_clone = Arc::clone(&event_bus);
//!
//! tokio::spawn(async move {
//!     // Use bus_clone in spawned task
//! });
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::broadcast;

// Re-export commonly used types
pub use tokio::sync::broadcast::error::{RecvError, SendError};
pub use tokio::sync::broadcast::Receiver;

/// Default buffer size for the event bus channel.
///
/// This value balances memory usage with the ability to handle bursts of events.
/// Subscribers that can't keep up will receive `RecvError::Lagged`.
pub const DEFAULT_EVENT_BUFFER_SIZE: usize = 100;

// ============================================================================
// Core Event Types
// ============================================================================

/// Top-level event enum encompassing all event categories.
///
/// This is the main event type published and received through the event bus.
/// It wraps domain-specific event types for different modules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum CoreEvent {
    /// Authentication-related events
    Auth(AuthEvent),
    /// Sync-related events
    Sync(SyncEvent),
    /// Library-related events
    Library(LibraryEvent),
    /// Playback-related events
    Playback(PlaybackEvent),
}

impl CoreEvent {
    /// Returns a human-readable description of the event.
    pub fn description(&self) -> &str {
        match self {
            CoreEvent::Auth(e) => e.description(),
            CoreEvent::Sync(e) => e.description(),
            CoreEvent::Library(e) => e.description(),
            CoreEvent::Playback(e) => e.description(),
        }
    }

    /// Returns the severity level of the event.
    pub fn severity(&self) -> EventSeverity {
        match self {
            CoreEvent::Auth(AuthEvent::AuthError { .. }) => EventSeverity::Error,
            CoreEvent::Sync(SyncEvent::Failed { .. }) => EventSeverity::Error,
            CoreEvent::Playback(PlaybackEvent::Error { .. }) => EventSeverity::Error,
            CoreEvent::Auth(AuthEvent::SignedIn { .. }) => EventSeverity::Info,
            CoreEvent::Sync(SyncEvent::Completed { .. }) => EventSeverity::Info,
            _ => EventSeverity::Debug,
        }
    }
}

/// Event severity levels for filtering and logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventSeverity {
    /// Debug-level events (verbose)
    Debug,
    /// Informational events
    Info,
    /// Warning events
    Warning,
    /// Error events
    Error,
}

// ============================================================================
// Authentication Events
// ============================================================================

/// Events related to authentication and profile management.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum AuthEvent {
    /// User signed out from a profile.
    SignedOut {
        /// The profile ID that was signed out.
        profile_id: String,
    },
    /// Authentication flow in progress.
    SigningIn {
        /// The provider being authenticated with (e.g., "GoogleDrive", "OneDrive").
        provider: String,
    },
    /// User successfully authenticated.
    SignedIn {
        /// The newly created or existing profile ID.
        profile_id: String,
        /// The provider used for authentication.
        provider: String,
    },
    /// Access token is being refreshed.
    TokenRefreshing {
        /// The profile whose token is being refreshed.
        profile_id: String,
    },
    /// Token refresh completed successfully.
    TokenRefreshed {
        /// The profile whose token was refreshed.
        profile_id: String,
        /// Timestamp when the new token expires (Unix epoch seconds).
        expires_at: u64,
    },
    /// Authentication error occurred.
    AuthError {
        /// The profile ID if available.
        profile_id: Option<String>,
        /// Human-readable error message.
        message: String,
        /// Whether the error is recoverable (e.g., retry possible).
        recoverable: bool,
    },
}

impl AuthEvent {
    fn description(&self) -> &str {
        match self {
            AuthEvent::SignedOut { .. } => "User signed out",
            AuthEvent::SigningIn { .. } => "Authentication in progress",
            AuthEvent::SignedIn { .. } => "User signed in successfully",
            AuthEvent::TokenRefreshing { .. } => "Refreshing access token",
            AuthEvent::TokenRefreshed { .. } => "Token refreshed successfully",
            AuthEvent::AuthError { .. } => "Authentication error",
        }
    }
}

// ============================================================================
// Sync Events
// ============================================================================

/// Events related to synchronization with cloud storage providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum SyncEvent {
    /// Sync job initiated.
    Started {
        /// Unique identifier for this sync job.
        job_id: String,
        /// The profile being synced.
        profile_id: String,
        /// The provider being synced.
        provider: String,
        /// Whether this is a full or incremental sync.
        is_full_sync: bool,
    },
    /// Incremental progress update during sync.
    Progress {
        /// The sync job ID.
        job_id: String,
        /// Number of items processed so far.
        items_processed: u64,
        /// Total items to process (may be estimated).
        total_items: Option<u64>,
        /// Progress percentage (0-100).
        percent: u8,
        /// Current phase (e.g., "Listing files", "Extracting metadata").
        phase: String,
    },
    /// Sync finished successfully.
    Completed {
        /// The sync job ID.
        job_id: String,
        /// Total items processed.
        items_processed: u64,
        /// Number of new items added.
        items_added: u64,
        /// Number of items updated.
        items_updated: u64,
        /// Number of items deleted.
        items_deleted: u64,
        /// Duration of sync in seconds.
        duration_secs: u64,
    },
    /// Sync encountered an error and stopped.
    Failed {
        /// The sync job ID.
        job_id: String,
        /// Human-readable error message.
        message: String,
        /// Number of items processed before failure.
        items_processed: u64,
        /// Whether the sync can be retried.
        recoverable: bool,
    },
    /// Sync was cancelled by user.
    Cancelled {
        /// The sync job ID.
        job_id: String,
        /// Number of items processed before cancellation.
        items_processed: u64,
    },
}

impl SyncEvent {
    fn description(&self) -> &str {
        match self {
            SyncEvent::Started { .. } => "Sync started",
            SyncEvent::Progress { .. } => "Sync in progress",
            SyncEvent::Completed { .. } => "Sync completed successfully",
            SyncEvent::Failed { .. } => "Sync failed",
            SyncEvent::Cancelled { .. } => "Sync cancelled",
        }
    }
}

// ============================================================================
// Library Events
// ============================================================================

/// Events related to library content changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum LibraryEvent {
    /// New track added to library.
    TrackAdded {
        /// The track ID.
        track_id: String,
        /// Track title.
        title: String,
        /// Artist name.
        artist: Option<String>,
        /// Album name.
        album: Option<String>,
    },
    /// Track metadata updated.
    TrackUpdated {
        /// The track ID.
        track_id: String,
        /// Fields that were updated.
        updated_fields: Vec<String>,
    },
    /// Track removed from library.
    TrackDeleted {
        /// The track ID that was deleted.
        track_id: String,
    },
    /// New album created.
    AlbumAdded {
        /// The album ID.
        album_id: String,
        /// Album name.
        name: String,
        /// Artist name.
        artist: Option<String>,
        /// Number of tracks in album.
        track_count: u32,
    },
    /// New playlist created.
    PlaylistCreated {
        /// The playlist ID.
        playlist_id: String,
        /// Playlist name.
        name: String,
    },
    /// Playlist modified (renamed, tracks added/removed).
    PlaylistUpdated {
        /// The playlist ID.
        playlist_id: String,
        /// What changed (e.g., "renamed", "tracks_added", "tracks_removed").
        change_type: String,
    },
}

impl LibraryEvent {
    fn description(&self) -> &str {
        match self {
            LibraryEvent::TrackAdded { .. } => "Track added to library",
            LibraryEvent::TrackUpdated { .. } => "Track metadata updated",
            LibraryEvent::TrackDeleted { .. } => "Track removed from library",
            LibraryEvent::AlbumAdded { .. } => "Album added to library",
            LibraryEvent::PlaylistCreated { .. } => "Playlist created",
            LibraryEvent::PlaylistUpdated { .. } => "Playlist updated",
        }
    }
}

// ============================================================================
// Playback Events
// ============================================================================

/// Events related to audio playback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum PlaybackEvent {
    /// Playback started.
    Started {
        /// The track ID being played.
        track_id: String,
        /// Track title.
        title: String,
    },
    /// Playback paused.
    Paused {
        /// The track ID.
        track_id: String,
        /// Position when paused (milliseconds).
        position_ms: u64,
    },
    /// Playback resumed after pause.
    Resumed {
        /// The track ID.
        track_id: String,
        /// Position when resumed (milliseconds).
        position_ms: u64,
    },
    /// Playback stopped.
    Stopped {
        /// The track ID.
        track_id: String,
    },
    /// Track finished playing naturally.
    Completed {
        /// The track ID that completed.
        track_id: String,
    },
    /// Playback position changed (seek or natural progression).
    PositionChanged {
        /// The track ID.
        track_id: String,
        /// New position (milliseconds).
        position_ms: u64,
        /// Track duration (milliseconds).
        duration_ms: u64,
    },
    /// Playback error occurred.
    Error {
        /// The track ID if available.
        track_id: Option<String>,
        /// Human-readable error message.
        message: String,
        /// Whether playback can be retried.
        recoverable: bool,
    },
}

impl PlaybackEvent {
    fn description(&self) -> &str {
        match self {
            PlaybackEvent::Started { .. } => "Playback started",
            PlaybackEvent::Paused { .. } => "Playback paused",
            PlaybackEvent::Resumed { .. } => "Playback resumed",
            PlaybackEvent::Stopped { .. } => "Playback stopped",
            PlaybackEvent::Completed { .. } => "Track completed",
            PlaybackEvent::PositionChanged { .. } => "Playback position changed",
            PlaybackEvent::Error { .. } => "Playback error",
        }
    }
}

// ============================================================================
// Event Bus
// ============================================================================

/// Central event bus for publishing and subscribing to events.
///
/// Uses `tokio::sync::broadcast` internally, which provides:
/// - Multiple producers (clone the `EventBus`)
/// - Multiple consumers (each `subscribe()` creates a new receiver)
/// - Non-blocking sends (events are cloned for each subscriber)
/// - Lagging detection (slow subscribers get `RecvError::Lagged`)
///
/// # Example
///
/// ```rust
/// use core_runtime::events::{EventBus, CoreEvent, AuthEvent};
///
/// # #[tokio::main]
/// # async fn main() {
/// let event_bus = EventBus::new(100);
///
/// // Subscribe to events
/// let mut subscriber1 = event_bus.subscribe();
/// let mut subscriber2 = event_bus.subscribe();
///
/// // Emit an event
/// let event = CoreEvent::Auth(AuthEvent::SignedIn {
///     profile_id: "user-123".to_string(),
///     provider: "GoogleDrive".to_string(),
/// });
/// event_bus.emit(event).ok();
///
/// // Both subscribers receive the event
/// # tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
/// # }
/// ```
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<CoreEvent>,
}

impl EventBus {
    /// Creates a new event bus with the specified buffer size.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to buffer per subscriber.
    ///   When a subscriber falls behind by more than this amount, it will
    ///   receive a `RecvError::Lagged` error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_runtime::events::EventBus;
    ///
    /// let event_bus = EventBus::new(100);
    /// ```
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Creates a new event bus with the default buffer size.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_runtime::events::EventBus;
    ///
    /// let event_bus = EventBus::default();
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(DEFAULT_EVENT_BUFFER_SIZE)
    }

    /// Publishes an event to all subscribers.
    ///
    /// Returns the number of subscribers that received the event.
    /// Returns an error if there are no active subscribers.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_runtime::events::{EventBus, CoreEvent, AuthEvent};
    ///
    /// let event_bus = EventBus::new(100);
    /// let event = CoreEvent::Auth(AuthEvent::SignedIn {
    ///     profile_id: "user-123".to_string(),
    ///     provider: "GoogleDrive".to_string(),
    /// });
    ///
    /// match event_bus.emit(event) {
    ///     Ok(n) => println!("Event sent to {} subscribers", n),
    ///     Err(_) => println!("No active subscribers"),
    /// }
    /// ```
    pub fn emit(&self, event: CoreEvent) -> Result<usize, SendError<CoreEvent>> {
        self.sender.send(event)
    }

    /// Creates a new subscriber to receive events.
    ///
    /// Each call creates an independent receiver that will receive all future events.
    /// Past events are not replayed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use core_runtime::events::EventBus;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let event_bus = EventBus::new(100);
    /// let mut subscriber = event_bus.subscribe();
    ///
    /// tokio::spawn(async move {
    ///     while let Ok(event) = subscriber.recv().await {
    ///         println!("Received: {:?}", event);
    ///     }
    /// });
    /// # }
    /// ```
    pub fn subscribe(&self) -> Receiver<CoreEvent> {
        self.sender.subscribe()
    }

    /// Returns the number of active subscribers.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_runtime::events::EventBus;
    ///
    /// let event_bus = EventBus::new(100);
    /// assert_eq!(event_bus.subscriber_count(), 0);
    ///
    /// let _subscriber = event_bus.subscribe();
    /// assert_eq!(event_bus.subscriber_count(), 1);
    /// ```
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("subscriber_count", &self.subscriber_count())
            .finish()
    }
}

// ============================================================================
// Event Stream Wrapper
// ============================================================================

/// Type alias for event filter functions.
type EventFilter = Box<dyn Fn(&CoreEvent) -> bool + Send + Sync>;

/// A wrapper around `broadcast::Receiver` with additional filtering capabilities.
///
/// This provides a more ergonomic API for consuming events with optional filtering
/// by event type or severity.
///
/// # Example
///
/// ```rust
/// use core_runtime::events::{EventBus, EventStream, CoreEvent};
///
/// # #[tokio::main]
/// # async fn main() {
/// let event_bus = EventBus::new(100);
/// let stream = EventStream::new(event_bus.subscribe());
///
/// // Filter for auth events only
/// let mut auth_stream = stream.filter(|event| {
///     matches!(event, CoreEvent::Auth(_))
/// });
/// # }
/// ```
pub struct EventStream {
    receiver: Receiver<CoreEvent>,
    filter: Option<EventFilter>,
}

impl EventStream {
    /// Creates a new event stream from a receiver.
    pub fn new(receiver: Receiver<CoreEvent>) -> Self {
        Self {
            receiver,
            filter: None,
        }
    }

    /// Adds a filter function to this stream.
    ///
    /// Only events that match the filter will be returned by `recv()`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core_runtime::events::{EventBus, EventStream, CoreEvent};
    ///
    /// let event_bus = EventBus::new(100);
    /// let stream = EventStream::new(event_bus.subscribe());
    ///
    /// let auth_stream = stream.filter(|event| {
    ///     matches!(event, CoreEvent::Auth(_))
    /// });
    /// ```
    pub fn filter<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&CoreEvent) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Box::new(predicate));
        self
    }

    /// Receives the next event that passes the filter (if any).
    ///
    /// This will skip events that don't match the filter and return the next matching event.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Lagged(n)` if the subscriber fell behind by `n` events.
    /// Returns `RecvError::Closed` if all senders have been dropped.
    pub async fn recv(&mut self) -> Result<CoreEvent, RecvError> {
        loop {
            let event = self.receiver.recv().await?;

            // If no filter, return immediately
            let Some(filter) = &self.filter else {
                return Ok(event);
            };

            // Apply filter
            if filter(&event) {
                return Ok(event);
            }

            // Event didn't match filter, continue to next event
        }
    }

    /// Attempts to receive an event without blocking.
    ///
    /// Returns `None` if no events are currently available.
    pub fn try_recv(&mut self) -> Option<Result<CoreEvent, RecvError>> {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => {
                    // If no filter, return immediately
                    let Some(filter) = &self.filter else {
                        return Some(Ok(event));
                    };

                    // Apply filter
                    if filter(&event) {
                        return Some(Ok(event));
                    }

                    // Event didn't match filter, continue
                }
                Err(broadcast::error::TryRecvError::Empty) => return None,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    return Some(Err(RecvError::Lagged(n)))
                }
                Err(broadcast::error::TryRecvError::Closed) => return Some(Err(RecvError::Closed)),
            }
        }
    }
}

impl fmt::Debug for EventStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventStream")
            .field("has_filter", &self.filter.is_some())
            .finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_creation() {
        let bus = EventBus::new(10);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_event_bus_subscription() {
        let bus = EventBus::new(10);
        let _sub1 = bus.subscribe();
        let _sub2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn test_event_emission_no_subscribers() {
        let bus = EventBus::new(10);
        let event = CoreEvent::Auth(AuthEvent::SignedOut {
            profile_id: "test".to_string(),
        });

        // Should error when no subscribers
        assert!(bus.emit(event).is_err());
    }

    #[tokio::test]
    async fn test_event_emission_with_subscribers() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe();

        let event = CoreEvent::Auth(AuthEvent::SignedIn {
            profile_id: "test-profile".to_string(),
            provider: "GoogleDrive".to_string(),
        });

        // Emit event
        let result = bus.emit(event.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        // Subscriber should receive it
        let received = sub.recv().await.unwrap();
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn test_multiple_subscribers_receive_same_event() {
        let bus = EventBus::new(10);
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe();

        let event = CoreEvent::Sync(SyncEvent::Started {
            job_id: "job-1".to_string(),
            profile_id: "profile-1".to_string(),
            provider: "GoogleDrive".to_string(),
            is_full_sync: true,
        });

        bus.emit(event.clone()).ok();

        // Both should receive the event
        let received1 = sub1.recv().await.unwrap();
        let received2 = sub2.recv().await.unwrap();

        assert_eq!(received1, event);
        assert_eq!(received2, event);
    }

    #[tokio::test]
    async fn test_event_stream_without_filter() {
        let bus = EventBus::new(10);
        let mut stream = EventStream::new(bus.subscribe());

        let event = CoreEvent::Library(LibraryEvent::TrackAdded {
            track_id: "track-1".to_string(),
            title: "Test Track".to_string(),
            artist: Some("Test Artist".to_string()),
            album: Some("Test Album".to_string()),
        });

        bus.emit(event.clone()).ok();

        let received = stream.recv().await.unwrap();
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn test_event_stream_with_filter() {
        let bus = EventBus::new(10);
        let mut stream =
            EventStream::new(bus.subscribe()).filter(|event| matches!(event, CoreEvent::Auth(_)));

        // Emit non-auth event (should be filtered out)
        let sync_event = CoreEvent::Sync(SyncEvent::Completed {
            job_id: "job-1".to_string(),
            items_processed: 100,
            items_added: 50,
            items_updated: 30,
            items_deleted: 20,
            duration_secs: 60,
        });
        bus.emit(sync_event).ok();

        // Emit auth event (should pass through)
        let auth_event = CoreEvent::Auth(AuthEvent::SignedIn {
            profile_id: "profile-1".to_string(),
            provider: "OneDrive".to_string(),
        });
        bus.emit(auth_event.clone()).ok();

        // Should only receive the auth event
        let received = stream.recv().await.unwrap();
        assert_eq!(received, auth_event);
    }

    #[tokio::test]
    async fn test_lagged_subscriber() {
        let bus = EventBus::new(2); // Very small buffer
        let mut sub = bus.subscribe();

        // Emit more events than buffer size
        for i in 0..5 {
            let event = CoreEvent::Auth(AuthEvent::TokenRefreshed {
                profile_id: format!("profile-{}", i),
                expires_at: 1234567890 + i,
            });
            bus.emit(event).ok();
        }

        // First recv should indicate lagging
        let result = sub.recv().await;
        assert!(matches!(result, Err(RecvError::Lagged(_))));
    }

    #[tokio::test]
    async fn test_event_severity() {
        let error_event = CoreEvent::Auth(AuthEvent::AuthError {
            profile_id: None,
            message: "Failed".to_string(),
            recoverable: false,
        });
        assert_eq!(error_event.severity(), EventSeverity::Error);

        let info_event = CoreEvent::Sync(SyncEvent::Completed {
            job_id: "job-1".to_string(),
            items_processed: 100,
            items_added: 50,
            items_updated: 30,
            items_deleted: 20,
            duration_secs: 60,
        });
        assert_eq!(info_event.severity(), EventSeverity::Info);

        let debug_event = CoreEvent::Playback(PlaybackEvent::PositionChanged {
            track_id: "track-1".to_string(),
            position_ms: 5000,
            duration_ms: 180000,
        });
        assert_eq!(debug_event.severity(), EventSeverity::Debug);
    }

    #[tokio::test]
    async fn test_event_description() {
        let event = CoreEvent::Auth(AuthEvent::SignedIn {
            profile_id: "profile-1".to_string(),
            provider: "GoogleDrive".to_string(),
        });
        assert_eq!(event.description(), "User signed in successfully");
    }

    #[tokio::test]
    async fn test_concurrent_publishers() {
        let bus = EventBus::new(100);
        let mut sub = bus.subscribe();

        let bus1 = bus.clone();
        let bus2 = bus.clone();

        // Spawn two concurrent publishers
        let handle1 = tokio::spawn(async move {
            for i in 0..10 {
                let event = CoreEvent::Auth(AuthEvent::TokenRefreshed {
                    profile_id: format!("profile-{}", i),
                    expires_at: 1234567890,
                });
                bus1.emit(event).ok();
            }
        });

        let handle2 = tokio::spawn(async move {
            for i in 0..10 {
                let event = CoreEvent::Sync(SyncEvent::Progress {
                    job_id: "job-1".to_string(),
                    items_processed: i * 10,
                    total_items: Some(100),
                    percent: (i * 10) as u8,
                    phase: "Processing".to_string(),
                });
                bus2.emit(event).ok();
            }
        });

        // Wait for publishers
        handle1.await.ok();
        handle2.await.ok();

        // Should have received 20 events
        let mut count = 0;
        while sub.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 20);
    }

    #[tokio::test]
    async fn test_event_serialization() {
        let event = CoreEvent::Sync(SyncEvent::Progress {
            job_id: "job-123".to_string(),
            items_processed: 50,
            total_items: Some(100),
            percent: 50,
            phase: "Extracting metadata".to_string(),
        });

        // Serialize to JSON
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("job-123"));

        // Deserialize back
        let deserialized: CoreEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, event);
    }

    #[test]
    fn test_event_cloning() {
        let event = CoreEvent::Library(LibraryEvent::PlaylistCreated {
            playlist_id: "playlist-1".to_string(),
            name: "My Playlist".to_string(),
        });

        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[tokio::test]
    async fn test_try_recv_empty() {
        let bus = EventBus::new(10);
        let mut stream = EventStream::new(bus.subscribe());

        // Should return None when no events
        assert!(stream.try_recv().is_none());
    }

    #[tokio::test]
    async fn test_try_recv_with_event() {
        let bus = EventBus::new(10);
        let mut stream = EventStream::new(bus.subscribe());

        let event = CoreEvent::Playback(PlaybackEvent::Started {
            track_id: "track-1".to_string(),
            title: "Test Song".to_string(),
        });

        bus.emit(event.clone()).ok();

        // Give time for event to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Should receive the event
        let result = stream.try_recv();
        assert!(result.is_some());
        let received = result.unwrap().unwrap();
        assert_eq!(received, event);
    }
}
