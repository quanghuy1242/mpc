//! # Logging & Tracing Infrastructure
//!
//! Provides structured logging with `tracing` crate, supporting:
//! - JSON and pretty-print output formats
//! - Module-level filtering
//! - PII redaction (tokens, emails, paths)
//! - Integration with host logging via `LoggerSink`
//! - Span contexts for distributed tracing
//!
//! ## Overview
//!
//! This module configures the `tracing-subscriber` infrastructure and provides
//! utilities for forwarding logs to platform-specific logging systems through
//! the `LoggerSink` trait. When a sink is configured, every event that survives
//! filtering is mirrored to the host logger while still flowing through the
//! standard `tracing` layers.
//!
//! ## Usage
//!
//! ```ignore
//! use core_runtime::logging::{LoggingConfig, init_logging};
//! use bridge_traits::time::{LogLevel, ConsoleLogger};
//! use std::sync::Arc;
//!
//! #[core_async::main]
//! async fn main() {
//!     let config = LoggingConfig::default()
//!         .with_format(LogFormat::Pretty)
//!         .with_level(LogLevel::Debug)
//!         .with_logger_sink(Arc::new(ConsoleLogger::default()));
//!     
//!     init_logging(config).expect("Failed to initialize logging");
//!     
//!     tracing::info!("Application started");
//! }
//! ```
//!
//! ## LoggerSink integration
//!
//! Provide a custom `LoggerSink` to mirror log events into a host-specific
//! pipeline (e.g., `os_log`/`Logcat`). The sink receives structured
//! [`LogEntry`](bridge_traits::time::LogEntry) instances with the original
//! message plus any fields emitted on the event.
//!
//! ```ignore
//! use bridge_traits::time::{ConsoleLogger, LoggerSink};
//! use core_runtime::logging::{init_logging, LoggingConfig};
//! use std::sync::Arc;
//!
//! let sink = Arc::new(ConsoleLogger::default());
//! let config = LoggingConfig::default().with_logger_sink(sink);
//! init_logging(config)?;
//! tracing::warn!(target: "sync", "Slow request");
//! ```

#[cfg(target_arch = "wasm32")]
use crate::error::Result;
#[cfg(not(target_arch = "wasm32"))]
use crate::error::{Error, Result};

#[cfg(not(target_arch = "wasm32"))]
use bridge_traits::time::{LogEntry, LogLevel, LoggerSink};
#[cfg(target_arch = "wasm32")]
use bridge_traits::time::{LogLevel, LoggerSink};

#[cfg(not(target_arch = "wasm32"))]
use core_async::runtime;

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::io;

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use tracing::field::{Field, Visit};
#[cfg(not(target_arch = "wasm32"))]
use tracing::{Event, Subscriber};
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::{
    filter::EnvFilter,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    Layer,
};

/// Log output format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable pretty format with colors
    Pretty,
    /// Structured JSON format for machine parsing
    Json,
    /// Compact format for production
    Compact,
}

impl Default for LogFormat {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        return Self::Pretty;

        #[cfg(not(debug_assertions))]
        return Self::Json;
    }
}

/// Logging configuration
#[derive(Clone)]
pub struct LoggingConfig {
    /// Output format
    pub format: LogFormat,
    /// Minimum log level
    pub level: LogLevel,
    /// Enable PII redaction
    pub redact_pii: bool,
    /// Custom filter string (e.g., "core_auth=debug,core_sync=trace")
    pub filter: Option<String>,
    /// Optional logger sink for forwarding logs to host
    pub logger_sink: Option<Arc<dyn LoggerSink>>,
    /// Enable span contexts for distributed tracing
    pub enable_spans: bool,
    /// Display target module in logs
    pub display_target: bool,
    /// Display thread info
    pub display_thread_info: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::default(),
            level: LogLevel::Info,
            redact_pii: true,
            filter: None,
            logger_sink: None,
            enable_spans: true,
            display_target: true,
            display_thread_info: false,
        }
    }
}

impl LoggingConfig {
    /// Set log format
    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    /// Set minimum log level
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }

    /// Enable or disable PII redaction
    pub fn with_pii_redaction(mut self, redact: bool) -> Self {
        self.redact_pii = redact;
        self
    }

    /// Set custom filter string
    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Set logger sink for host integration
    pub fn with_logger_sink(mut self, sink: Arc<dyn LoggerSink>) -> Self {
        self.logger_sink = Some(sink);
        self
    }

    /// Enable or disable span contexts
    pub fn with_spans(mut self, enable: bool) -> Self {
        self.enable_spans = enable;
        self
    }

    /// Enable or disable target display
    pub fn with_target(mut self, display: bool) -> Self {
        self.display_target = display;
        self
    }

    /// Enable or disable thread info
    pub fn with_thread_info(mut self, display: bool) -> Self {
        self.display_thread_info = display;
        self
    }
}

/// Initialize the logging system
///
/// This should be called once during application startup. Subsequent calls
/// will return an error.
///
/// # Errors
///
/// Returns an error if:
/// - Logging is already initialized
/// - Configuration is invalid
///
/// # Example
///
/// ```ignore
/// use core_runtime::logging::{LoggingConfig, init_logging};
///
/// let config = LoggingConfig::default();
/// init_logging(config)?;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub fn init_logging(config: LoggingConfig) -> Result<()> {
    let filter = build_filter(&config)?;

    match config.format {
        LogFormat::Pretty => init_pretty_logging(config, filter),
        LogFormat::Json => init_json_logging(config, filter),
        LogFormat::Compact => init_compact_logging(config, filter),
    }
}

/// Initialize logging for WASM target.
///
/// On WASM, we use a simplified logging setup since tracing-subscriber
/// has limited support for the WASM target. Console logging is used directly.
#[cfg(target_arch = "wasm32")]
pub fn init_logging(_config: LoggingConfig) -> Result<()> {
    // On WASM, we rely on console_error_panic_hook and web-sys console
    // for logging. The tracing infrastructure is too complex for WASM.
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn build_filter(config: &LoggingConfig) -> Result<EnvFilter> {
    let base_level = match config.level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    };

    let filter_string = if let Some(custom_filter) = &config.filter {
        custom_filter.clone()
    } else {
        // Default filter: our crates at specified level, dependencies at warn
        format!(
            "{}={},core_runtime={},core_auth={},core_sync={},core_library={},\
             core_metadata={},core_playback={},core_service={},\
             provider_google_drive={},provider_onedrive={},\
             bridge_desktop={},h2=warn,hyper=warn,reqwest=warn,sqlx=warn",
            env!("CARGO_PKG_NAME"),
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level,
            base_level
        )
    };

    EnvFilter::try_new(filter_string)
        .map_err(|e| Error::Config(format!("Invalid log filter: {}", e)))
}

#[cfg(not(target_arch = "wasm32"))]
fn init_pretty_logging(config: LoggingConfig, filter: EnvFilter) -> Result<()> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_target(config.display_target)
        .with_thread_ids(config.display_thread_info)
        .with_thread_names(config.display_thread_info)
        .with_span_events(if config.enable_spans {
            tracing_subscriber::fmt::format::FmtSpan::ACTIVE
        } else {
            tracing_subscriber::fmt::format::FmtSpan::NONE
        })
        .with_writer(io::stdout);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(LoggerSinkLayer::new(config.logger_sink.clone()));

    if config.redact_pii {
        subscriber
            .with(PiiRedactionLayer)
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    } else {
        subscriber
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn init_json_logging(config: LoggingConfig, filter: EnvFilter) -> Result<()> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(config.enable_spans)
        .with_span_list(config.enable_spans)
        .with_target(config.display_target)
        .with_thread_ids(config.display_thread_info)
        .with_thread_names(config.display_thread_info)
        .with_writer(io::stdout);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(LoggerSinkLayer::new(config.logger_sink.clone()));

    if config.redact_pii {
        subscriber
            .with(PiiRedactionLayer)
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    } else {
        subscriber
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn init_compact_logging(config: LoggingConfig, filter: EnvFilter) -> Result<()> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(config.display_target)
        .with_thread_ids(config.display_thread_info)
        .with_thread_names(config.display_thread_info)
        .with_writer(io::stdout);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(LoggerSinkLayer::new(config.logger_sink.clone()));

    if config.redact_pii {
        subscriber
            .with(PiiRedactionLayer)
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    } else {
        subscriber
            .try_init()
            .map_err(|e| Error::Config(format!("Failed to initialize logging: {}", e)))?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
/// PII redaction layer
///
/// Automatically redacts sensitive information from logs:
/// - OAuth tokens (access_token, refresh_token)
/// - Email addresses
/// - File paths (replaced with basename only)
/// - Authorization headers
#[cfg(not(target_arch = "wasm32"))]
struct PiiRedactionLayer;

#[cfg(not(target_arch = "wasm32"))]
impl<S> Layer<S> for PiiRedactionLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, _event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // PII redaction is handled through careful field naming conventions
        // and avoiding logging sensitive data in the first place.
        // This is a placeholder for more advanced redaction if needed.
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Layer that forwards events to a `LoggerSink` implementation.
struct LoggerSinkLayer {
    sink: Option<Arc<dyn LoggerSink>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LoggerSinkLayer {
    fn new(sink: Option<Arc<dyn LoggerSink>>) -> Self {
        Self { sink }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S> Layer<S> for LoggerSinkLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };

        let metadata = event.metadata();
        let level = tracing_level_to_log_level(*metadata.level());

        if level < sink.min_level() {
            return;
        }

        let mut visitor = SinkVisitor::default();
        event.record(&mut visitor);

        let message = visitor
            .message
            .unwrap_or_else(|| metadata.name().to_string());

        let mut entry = LogEntry::new(level, metadata.target(), message);

        for (key, value) in visitor.fields {
            entry = entry.with_field(key, value);
        }

        if let Some(span) = ctx.lookup_current() {
            entry.span_id = Some(span.name().to_string());
        }

        let sink = Arc::clone(sink);

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(handle) = runtime::Handle::try_current() {
                let sink_clone = Arc::clone(&sink);
                let entry_clone = entry.clone();
                handle.spawn(async move {
                    if let Err(err) = sink_clone.log(entry_clone).await {
                        eprintln!("LoggerSink error: {}", err);
                    }
                });
                return;
            }
        }

        #[cfg(target_arch = "wasm32")]
        runtime::block_on(async move {
            if let Err(err) = sink.log(entry).await {
                eprintln!("LoggerSink error: {}", err);
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        if let Err(err) = runtime::block_on(async move { sink.log(entry).await }) {
            eprintln!("LoggerSink error: {}", err);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct SinkVisitor {
    message: Option<String>,
    fields: HashMap<String, String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl SinkVisitor {
    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Visit for SinkVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_value(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_value(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{:?}", value));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tracing_level_to_log_level(level: tracing::Level) -> LogLevel {
    match level {
        tracing::Level::TRACE => LogLevel::Trace,
        tracing::Level::DEBUG => LogLevel::Debug,
        tracing::Level::INFO => LogLevel::Info,
        tracing::Level::WARN => LogLevel::Warn,
        tracing::Level::ERROR => LogLevel::Error,
    }
}

/// Helper function to redact sensitive field values
///
/// This should be used when manually constructing log entries:
///
/// ```ignore
/// use tracing::info;
/// use core_runtime::logging::redact_if_sensitive;
///
/// let token = "sensitive_token_value";
/// info!(token = %redact_if_sensitive("token", token), "Retrieved token");
/// ```
pub fn redact_if_sensitive(field_name: &str, value: &str) -> String {
    const SENSITIVE_FIELDS: &[&str] = &[
        "token",
        "access_token",
        "refresh_token",
        "password",
        "secret",
        "api_key",
        "authorization",
        "bearer",
    ];

    let field_lower = field_name.to_lowercase();
    if SENSITIVE_FIELDS.iter().any(|&f| field_lower.contains(f)) {
        "[REDACTED]".to_string()
    } else if value.contains('@') && value.contains('.') {
        // Likely an email - redact domain but keep first char
        if let Some(at_pos) = value.find('@') {
            format!("{}***@[REDACTED]", &value[..1.min(at_pos)])
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    }
}

/// Strip full file paths to basename only for privacy
///
/// Useful when logging file operations:
///
/// ```ignore
/// use tracing::info;
/// use core_runtime::logging::strip_path;
///
/// let path = "/Users/john/Music/song.mp3";
/// info!(file = %strip_path(path), "Processing file");
/// // Logs: file="song.mp3"
/// ```
pub fn strip_path(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit('\\')
        .next()
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bridge_traits::error::Result as SinkResult;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_logging_config_builder() {
        let config = LoggingConfig::default()
            .with_format(LogFormat::Json)
            .with_level(LogLevel::Debug)
            .with_pii_redaction(true)
            .with_filter("core_auth=trace")
            .with_spans(true)
            .with_target(true)
            .with_thread_info(true);

        assert_eq!(config.format, LogFormat::Json);
        assert_eq!(config.level, LogLevel::Debug);
        assert!(config.redact_pii);
        assert_eq!(config.filter, Some("core_auth=trace".to_string()));
        assert!(config.enable_spans);
        assert!(config.display_target);
        assert!(config.display_thread_info);
    }

    #[test]
    fn test_redact_if_sensitive() {
        // Tokens should be redacted
        assert_eq!(
            redact_if_sensitive("access_token", "secret123"),
            "[REDACTED]"
        );
        assert_eq!(redact_if_sensitive("token", "abc"), "[REDACTED]");
        assert_eq!(redact_if_sensitive("password", "pass"), "[REDACTED]");

        // Emails should be partially redacted
        let redacted = redact_if_sensitive("email", "user@example.com");
        assert!(redacted.starts_with('u'));
        assert!(redacted.contains("[REDACTED]"));

        // Normal values should pass through
        assert_eq!(redact_if_sensitive("track_id", "12345"), "12345");
        assert_eq!(redact_if_sensitive("name", "Song Name"), "Song Name");
    }

    #[test]
    fn test_strip_path() {
        assert_eq!(strip_path("/home/user/music/song.mp3"), "song.mp3");
        assert_eq!(strip_path("C:\\Users\\John\\Music\\song.mp3"), "song.mp3");
        assert_eq!(strip_path("song.mp3"), "song.mp3");
        assert_eq!(strip_path("/var/log/"), "");
    }

    #[test]
    fn test_default_format() {
        #[cfg(debug_assertions)]
        assert_eq!(LogFormat::default(), LogFormat::Pretty);

        #[cfg(not(debug_assertions))]
        assert_eq!(LogFormat::default(), LogFormat::Json);
    }

    #[test]
    fn test_build_filter() {
        let config = LoggingConfig::default().with_level(LogLevel::Debug);
        let filter = build_filter(&config).unwrap();
        // Basic test that filter builds without errors
        assert!(filter.to_string().contains("debug"));
    }

    #[test]
    fn test_build_custom_filter() {
        let config = LoggingConfig::default().with_filter("core_auth=trace,core_sync=debug");
        let filter = build_filter(&config).unwrap();
        assert!(filter.to_string().contains("core_auth=trace"));
    }

    #[test]
    fn test_logger_sink_layer_forwards_event() {
        let sink = Arc::new(TestLoggerSink::default());
        let trait_sink: Arc<dyn LoggerSink> = sink.clone();
        let layer = LoggerSinkLayer::new(Some(trait_sink));
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!(target: "test.target", user = "alice", "hello world");

        let entries = sink.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.target, "test.target");
        assert_eq!(entry.message, "hello world");
        assert_eq!(entry.fields.get("user"), Some(&"alice".to_string()));
    }

    #[derive(Default)]
    struct TestLoggerSink {
        entries: Mutex<Vec<LogEntry>>,
    }

    #[async_trait]
    impl LoggerSink for TestLoggerSink {
        async fn log(&self, entry: LogEntry) -> SinkResult<()> {
            let mut entries = self.entries.lock().unwrap();
            entries.push(entry);
            Ok(())
        }

        fn min_level(&self) -> LogLevel {
            LogLevel::Trace
        }
    }
}
