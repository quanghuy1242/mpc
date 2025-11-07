//! Time and Logging Abstractions
//!
//! Provides injectable time source and logging sink for testing and platform integration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{error::Result, platform::PlatformSendSync};

/// Time source trait
///
/// Abstracts system time to enable deterministic testing and support
/// for host-specified timezones.
///
/// # Example
///
/// ```ignore
/// use bridge_traits::time::Clock;
///
/// fn log_timestamp(clock: &dyn Clock) {
///     let now = clock.now();
///     println!("Current time: {}", now);
/// }
/// ```
pub trait Clock: PlatformSendSync {
    /// Get current UTC time
    fn now(&self) -> DateTime<Utc>;

    /// Get current Unix timestamp in seconds
    fn unix_timestamp(&self) -> i64 {
        self.now().timestamp()
    }

    /// Get current Unix timestamp in milliseconds
    fn unix_timestamp_millis(&self) -> i64 {
        self.now().timestamp_millis()
    }
}

/// System clock implementation using actual system time
#[derive(Debug, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Structured log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Log level
    pub level: LogLevel,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Target module/component
    pub target: String,
    /// Log message
    pub message: String,
    /// Structured fields
    pub fields: HashMap<String, String>,
    /// Span/trace ID for distributed tracing
    pub span_id: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            timestamp: Utc::now(),
            target: target.into(),
            message: message.into(),
            fields: HashMap::new(),
            span_id: None,
        }
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    pub fn with_span_id(mut self, span_id: impl Into<String>) -> Self {
        self.span_id = Some(span_id.into());
        self
    }
}

/// Logger sink trait
///
/// Forwards structured logs from the core to host logging pipelines:
/// - **iOS**: OSLog
/// - **Android**: Logcat
/// - **Desktop**: Console, file logs, or system logging
/// - **Web**: Console API with proper formatting
///
/// # Security
///
/// Implementations should ensure:
/// - No sensitive data (tokens, passwords) is logged
/// - PII is redacted based on host privacy policies
/// - Log levels respect debug/release build configurations
///
/// # Example
///
/// ```ignore
/// use bridge_traits::time::{LoggerSink, LogEntry, LogLevel};
///
/// async fn log_error(logger: &dyn LoggerSink, error: &str) {
///     let entry = LogEntry::new(LogLevel::Error, "core", error)
///         .with_field("component", "sync");
///     logger.log(entry).await.ok();
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait LoggerSink: PlatformSendSync {
    /// Forward a log entry to the host logging system
    async fn log(&self, entry: LogEntry) -> Result<()>;

    /// Flush any buffered logs
    async fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Get the minimum log level that will be processed
    ///
    /// Logs below this level can be filtered out at the source for performance.
    fn min_level(&self) -> LogLevel {
        LogLevel::Info
    }
}

/// Console logger implementation for testing/development
#[derive(Debug, Clone)]
pub struct ConsoleLogger {
    pub min_level: LogLevel,
}

impl Default for ConsoleLogger {
    fn default() -> Self {
        Self {
            min_level: LogLevel::Info,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl LoggerSink for ConsoleLogger {
    async fn log(&self, entry: LogEntry) -> Result<()> {
        if entry.level >= self.min_level {
            let level_str = match entry.level {
                LogLevel::Trace => "TRACE",
                LogLevel::Debug => "DEBUG",
                LogLevel::Info => "INFO",
                LogLevel::Warn => "WARN",
                LogLevel::Error => "ERROR",
            };

            println!(
                "[{}] {} {}: {}",
                entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
                level_str,
                entry.target,
                entry.message
            );

            if !entry.fields.is_empty() {
                println!("  Fields: {:?}", entry.fields);
            }
        }
        Ok(())
    }

    fn min_level(&self) -> LogLevel {
        self.min_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_clock() {
        let clock = SystemClock;
        let now = clock.now();
        let timestamp = clock.unix_timestamp();

        assert!(timestamp > 0);
        assert!(now.timestamp() == timestamp);
    }

    #[test]
    fn test_log_entry_builder() {
        let entry = LogEntry::new(LogLevel::Info, "test", "Test message")
            .with_field("user_id", "123")
            .with_span_id("trace-456");

        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.target, "test");
        assert_eq!(entry.message, "Test message");
        assert_eq!(entry.fields.get("user_id"), Some(&"123".to_string()));
        assert_eq!(entry.span_id, Some("trace-456".to_string()));
    }

    #[core_async::test]
    async fn test_console_logger() {
        let logger = ConsoleLogger::default();
        let entry = LogEntry::new(LogLevel::Info, "test", "Test log");

        logger.log(entry).await.unwrap();
    }
}
