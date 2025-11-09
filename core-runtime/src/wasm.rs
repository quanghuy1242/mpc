//! WASM bindings for core-runtime
//!
//! Exposes logging, configuration, and event system to JavaScript/TypeScript.

use crate::config::{FeatureFlags, MetadataApiConfig};
use crate::events::{
    AuthEvent, CoreEvent, EventBus, EventSeverity, LibraryEvent, PlaybackEvent, SyncEvent,
};
use crate::logging::{init_logging, LogFormat, LoggingConfig};
use bridge_traits::time::LogLevel;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

// =============================================================================
// Error Handling
// =============================================================================

fn to_js_error<E: std::fmt::Display>(err: E) -> JsValue {
    JsValue::from_str(&err.to_string())
}

// =============================================================================
// Logging
// =============================================================================

/// JavaScript-accessible logging configuration
#[wasm_bindgen]
#[derive(Clone)]
pub struct JsLoggingConfig {
    inner: LoggingConfig,
}

#[wasm_bindgen]
impl JsLoggingConfig {
    /// Create a new logging configuration with defaults
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: LoggingConfig::default(),
        }
    }

    /// Set log format (0 = Pretty, 1 = Json, 2 = Compact)
    #[wasm_bindgen(js_name = setFormat)]
    pub fn set_format(&mut self, format: u8) {
        self.inner.format = match format {
            1 => LogFormat::Json,
            2 => LogFormat::Compact,
            _ => LogFormat::Pretty,
        };
    }

    /// Set minimum log level (0 = Trace, 1 = Debug, 2 = Info, 3 = Warn, 4 = Error)
    #[wasm_bindgen(js_name = setLevel)]
    pub fn set_level(&mut self, level: u8) {
        self.inner.level = match level {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            4 => LogLevel::Error,
            _ => LogLevel::Info,
        };
    }

    /// Enable or disable PII redaction
    #[wasm_bindgen(js_name = setRedactPii)]
    pub fn set_redact_pii(&mut self, redact: bool) {
        self.inner.redact_pii = redact;
    }

    /// Set custom filter string (e.g., "core_auth=debug,core_sync=trace")
    #[wasm_bindgen(js_name = setFilter)]
    pub fn set_filter(&mut self, filter: String) {
        self.inner.filter = Some(filter);
    }

    /// Enable or disable span contexts
    #[wasm_bindgen(js_name = setSpans)]
    pub fn set_spans(&mut self, enable: bool) {
        self.inner.enable_spans = enable;
    }

    /// Enable or disable target display
    #[wasm_bindgen(js_name = setDisplayTarget)]
    pub fn set_display_target(&mut self, display: bool) {
        self.inner.display_target = display;
    }

    /// Enable or disable thread info
    #[wasm_bindgen(js_name = setDisplayThreadInfo)]
    pub fn set_display_thread_info(&mut self, display: bool) {
        self.inner.display_thread_info = display;
    }
}

/// Initialize logging system
#[wasm_bindgen(js_name = initLogging)]
pub fn init_logging_js(config: JsLoggingConfig) -> Result<(), JsValue> {
    init_logging(config.inner).map_err(to_js_error)
}

// =============================================================================
// Events
// =============================================================================

/// JavaScript-accessible Event Bus
#[wasm_bindgen]
pub struct JsEventBus {
    inner: Arc<EventBus>,
}

impl JsEventBus {
    /// Get the inner EventBus (for other crates to use)
    pub fn inner(&self) -> &Arc<EventBus> {
        &self.inner
    }
}

#[wasm_bindgen]
impl JsEventBus {
    /// Create a new event bus with specified capacity
    #[wasm_bindgen(constructor)]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(EventBus::new(capacity)),
        }
    }

    /// Emit an event (takes JSON string)
    pub fn emit(&self, event_json: &str) -> Result<usize, JsValue> {
        let event: CoreEvent = serde_json::from_str(event_json).map_err(to_js_error)?;
        self.inner.emit(event).map_err(to_js_error)
    }

    /// Get subscriber count
    #[wasm_bindgen(js_name = subscriberCount)]
    pub fn subscriber_count(&self) -> usize {
        self.inner.subscriber_count()
    }

    /// Subscribe and get a receiver handle (returns Promise<string> that resolves with event JSON)
    /// Note: For actual subscription, you'll need to use the EventStream interface
    /// This is a simplified version for demonstration
    pub fn subscribe(&self) -> JsEventReceiver {
        JsEventReceiver {
            receiver: self.inner.subscribe(),
        }
    }
}

/// JavaScript-accessible Event Receiver
#[wasm_bindgen]
pub struct JsEventReceiver {
    receiver: core_async::sync::broadcast::Receiver<CoreEvent>,
}

#[wasm_bindgen]
impl JsEventReceiver {
    /// Receive next event (returns JSON string)
    #[wasm_bindgen]
    pub async fn recv(&mut self) -> Result<String, JsValue> {
        let event = self.receiver.recv().await.map_err(to_js_error)?;
        serde_json::to_string(&event).map_err(to_js_error)
    }

    /// Try to receive without blocking (returns null if no events)
    #[wasm_bindgen(js_name = tryRecv)]
    pub fn try_recv(&mut self) -> Option<String> {
        match self.receiver.try_recv() {
            Ok(event) => serde_json::to_string(&event).ok(),
            Err(_) => None,
        }
    }
}

// =============================================================================
// Event Constructors (Helper functions)
// =============================================================================

/// Create a CoreEvent from JSON
#[wasm_bindgen(js_name = createEvent)]
pub fn create_event(event_json: &str) -> Result<String, JsValue> {
    let event: CoreEvent = serde_json::from_str(event_json).map_err(to_js_error)?;
    serde_json::to_string(&event).map_err(to_js_error)
}

/// Create an Auth.SignedIn event
#[wasm_bindgen(js_name = createAuthSignedInEvent)]
pub fn create_auth_signed_in_event(profile_id: String, provider: String) -> String {
    let event = CoreEvent::Auth(AuthEvent::SignedIn {
        profile_id,
        provider,
    });
    serde_json::to_string(&event).unwrap()
}

/// Create an Auth.SignedOut event
#[wasm_bindgen(js_name = createAuthSignedOutEvent)]
pub fn create_auth_signed_out_event(profile_id: String) -> String {
    let event = CoreEvent::Auth(AuthEvent::SignedOut { profile_id });
    serde_json::to_string(&event).unwrap()
}

/// Create a Sync.Started event
#[wasm_bindgen(js_name = createSyncStartedEvent)]
pub fn create_sync_started_event(
    job_id: String,
    profile_id: String,
    provider: String,
    is_full_sync: bool,
) -> String {
    let event = CoreEvent::Sync(SyncEvent::Started {
        job_id,
        profile_id,
        provider,
        is_full_sync,
    });
    serde_json::to_string(&event).unwrap()
}

/// Create a Sync.Progress event
#[wasm_bindgen(js_name = createSyncProgressEvent)]
pub fn create_sync_progress_event(
    job_id: String,
    items_processed: u64,
    total_items: Option<u64>,
    percent: u8,
    phase: String,
) -> String {
    let event = CoreEvent::Sync(SyncEvent::Progress {
        job_id,
        items_processed,
        total_items,
        percent,
        phase,
    });
    serde_json::to_string(&event).unwrap()
}

/// Create a Library.TrackAdded event
#[wasm_bindgen(js_name = createLibraryTrackAddedEvent)]
pub fn create_library_track_added_event(
    track_id: String,
    title: String,
    artist: Option<String>,
    album: Option<String>,
) -> String {
    let event = CoreEvent::Library(LibraryEvent::TrackAdded {
        track_id,
        title,
        artist,
        album,
    });
    serde_json::to_string(&event).unwrap()
}

/// Create a Playback.Started event
#[wasm_bindgen(js_name = createPlaybackStartedEvent)]
pub fn create_playback_started_event(track_id: String, title: String) -> String {
    let event = CoreEvent::Playback(PlaybackEvent::Started { track_id, title });
    serde_json::to_string(&event).unwrap()
}

// =============================================================================
// Feature Flags
// =============================================================================

/// JavaScript-accessible Feature Flags
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct JsFeatureFlags {
    inner: FeatureFlags,
}

#[wasm_bindgen]
impl JsFeatureFlags {
    /// Create default feature flags (all disabled)
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: FeatureFlags::default(),
        }
    }

    /// Enable or disable lyrics fetching
    #[wasm_bindgen(js_name = setEnableLyrics)]
    pub fn set_enable_lyrics(&mut self, enabled: bool) {
        self.inner.enable_lyrics = enabled;
    }

    /// Enable or disable remote artwork fetching
    #[wasm_bindgen(js_name = setEnableArtworkRemote)]
    pub fn set_enable_artwork_remote(&mut self, enabled: bool) {
        self.inner.enable_artwork_remote = enabled;
    }

    /// Enable or disable offline cache
    #[wasm_bindgen(js_name = setEnableOfflineCache)]
    pub fn set_enable_offline_cache(&mut self, enabled: bool) {
        self.inner.enable_offline_cache = enabled;
    }

    /// Enable or disable background sync
    #[wasm_bindgen(js_name = setEnableBackgroundSync)]
    pub fn set_enable_background_sync(&mut self, enabled: bool) {
        self.inner.enable_background_sync = enabled;
    }

    /// Enable or disable network awareness
    #[wasm_bindgen(js_name = setEnableNetworkAwareness)]
    pub fn set_enable_network_awareness(&mut self, enabled: bool) {
        self.inner.enable_network_awareness = enabled;
    }

    /// Get all flags as JSON
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.inner).unwrap()
    }
}

// =============================================================================
// Metadata API Config
// =============================================================================

/// JavaScript-accessible Metadata API Configuration
#[wasm_bindgen]
pub struct JsMetadataApiConfig {
    inner: MetadataApiConfig,
}

#[wasm_bindgen]
impl JsMetadataApiConfig {
    /// Create new metadata API configuration
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: MetadataApiConfig::new(),
        }
    }

    /// Set MusicBrainz user agent
    #[wasm_bindgen(js_name = setMusicBrainzUserAgent)]
    pub fn set_musicbrainz_user_agent(&mut self, user_agent: String) {
        self.inner.musicbrainz_user_agent = Some(user_agent);
    }

    /// Set Last.fm API key
    #[wasm_bindgen(js_name = setLastfmApiKey)]
    pub fn set_lastfm_api_key(&mut self, api_key: String) {
        self.inner.lastfm_api_key = Some(api_key);
    }

    /// Set rate limit delay in milliseconds
    #[wasm_bindgen(js_name = setRateLimitDelayMs)]
    pub fn set_rate_limit_delay_ms(&mut self, delay_ms: u64) {
        self.inner.rate_limit_delay_ms = delay_ms;
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner.validate().map_err(to_js_error)
    }

    /// Check if MusicBrainz is configured
    #[wasm_bindgen(js_name = hasMusicBrainz)]
    pub fn has_musicbrainz(&self) -> bool {
        self.inner.has_musicbrainz()
    }

    /// Check if Last.fm is configured
    #[wasm_bindgen(js_name = hasLastfm)]
    pub fn has_lastfm(&self) -> bool {
        self.inner.has_lastfm()
    }

    /// Get as JSON
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.inner).unwrap()
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

// NOTE: These are only exported when building as standalone WASM.
// When used as a dependency (e.g., in core-playback), these are disabled
// to avoid symbol conflicts with other modules.

#[cfg(feature = "wasm-standalone")]
/// Get library version
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(feature = "wasm-standalone")]
/// Get library name
#[wasm_bindgen]
pub fn name() -> String {
    env!("CARGO_PKG_NAME").to_string()
}

// =============================================================================
// Event Type Helpers (for TypeScript)
// =============================================================================

/// Event type enumeration for TypeScript
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JsEventType {
    Auth,
    Sync,
    Library,
    Playback,
}

/// Event severity for filtering
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JsEventSeverity {
    Debug,
    Info,
    Warning,
    Error,
}

/// Parse event type from JSON
#[wasm_bindgen(js_name = getEventType)]
pub fn get_event_type(event_json: &str) -> Result<JsEventType, JsValue> {
    let event: CoreEvent = serde_json::from_str(event_json).map_err(to_js_error)?;
    Ok(match event {
        CoreEvent::Auth(_) => JsEventType::Auth,
        CoreEvent::Sync(_) => JsEventType::Sync,
        CoreEvent::Library(_) => JsEventType::Library,
        CoreEvent::Playback(_) => JsEventType::Playback,
    })
}

/// Get event severity from JSON
#[wasm_bindgen(js_name = getEventSeverity)]
pub fn get_event_severity(event_json: &str) -> Result<JsEventSeverity, JsValue> {
    let event: CoreEvent = serde_json::from_str(event_json).map_err(to_js_error)?;
    Ok(match event.severity() {
        EventSeverity::Debug => JsEventSeverity::Debug,
        EventSeverity::Info => JsEventSeverity::Info,
        EventSeverity::Warning => JsEventSeverity::Warning,
        EventSeverity::Error => JsEventSeverity::Error,
    })
}

/// Get event description from JSON
#[wasm_bindgen(js_name = getEventDescription)]
pub fn get_event_description(event_json: &str) -> Result<String, JsValue> {
    let event: CoreEvent = serde_json::from_str(event_json).map_err(to_js_error)?;
    Ok(event.description().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_config_creation() {
        let config = JsLoggingConfig::new();
        assert_eq!(config.inner.format, LogFormat::default());
    }

    #[test]
    fn test_event_creation() {
        let event = create_auth_signed_in_event("profile-1".to_string(), "GoogleDrive".to_string());
        assert!(event.contains("profile-1"));
        assert!(event.contains("GoogleDrive"));
    }

    #[test]
    fn test_feature_flags() {
        let mut flags = JsFeatureFlags::new();
        flags.set_enable_lyrics(true);
        let json = flags.to_json();
        assert!(json.contains("enable_lyrics"));
    }
}
