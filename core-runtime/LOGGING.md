# Logging & Tracing Infrastructure

This document describes the logging and tracing infrastructure implemented in TASK-004.

## Overview

The `core-runtime` crate provides comprehensive structured logging using the [`tracing`](https://github.com/tokio-rs/tracing) ecosystem. The implementation supports:

- ✅ Multiple output formats (JSON, Pretty, Compact)
- ✅ Module-level filtering
- ✅ PII redaction (tokens, emails, paths)
- ✅ Integration with host logging via `LoggerSink` trait
- ✅ Span contexts for distributed tracing
- ✅ Runtime-configurable log levels

## Features

### Output Formats

#### Pretty Format (Development)
Human-readable format with colors, ideal for development:
```rust
use core_runtime::logging::{LoggingConfig, LogFormat, init_logging};
use bridge_traits::time::LogLevel;

let config = LoggingConfig::default()
    .with_format(LogFormat::Pretty)
    .with_level(LogLevel::Debug);

init_logging(config)?;
```

#### JSON Format (Production)
Structured JSON format for log aggregation and analysis:
```rust
let config = LoggingConfig::default()
    .with_format(LogFormat::Json)
    .with_level(LogLevel::Info);

init_logging(config)?;
```

Output example:
```json
{"timestamp":"2024-11-04T10:30:15.123Z","level":"INFO","target":"core_sync","message":"Sync completed","files_synced":150,"duration_ms":2345}
```

#### Compact Format
Minimal format for production with space constraints:
```rust
let config = LoggingConfig::default()
    .with_format(LogFormat::Compact)
    .with_level(LogLevel::Warn);

init_logging(config)?;
```

### Log Filtering

Control which modules log at which levels:

```rust
// Custom filter string
let config = LoggingConfig::default()
    .with_filter("core_auth=debug,core_sync=trace,h2=warn");

init_logging(config)?;
```

Default filter (when not specified):
- All `core_*` crates: configured level
- All `provider_*` crates: configured level
- Dependencies (h2, hyper, reqwest, sqlx): WARN level

### PII Redaction

Automatically redact sensitive information:

```rust
use core_runtime::logging::redact_if_sensitive;
use tracing::info;

let token = "secret_access_token_12345";
let email = "user@example.com";

// These will be automatically redacted
info!(
    token = %redact_if_sensitive("access_token", token),
    email = %redact_if_sensitive("email", email),
    "Authentication successful"
);

// Output: token="[REDACTED]", email="u***@[REDACTED]"
```

Redacted fields:
- `token`, `access_token`, `refresh_token`
- `password`, `secret`, `api_key`
- `authorization`, `bearer`
- Email addresses (partial redaction)

### Path Stripping

Strip full file paths for privacy:

```rust
use core_runtime::logging::strip_path;

let path = "/home/user/private/music/song.mp3";
info!(file = %strip_path(path), "Processing file");
// Logs: file="song.mp3"
```

### Span Contexts

Use spans for distributed tracing:

```rust
use tracing::{info, span, Level};

let span = span!(Level::INFO, "sync_operation", provider = "google_drive");
let _enter = span.enter();

info!("Starting sync");
// All logs within this scope will include span context
```

With instrumentation:
```rust
use tracing::instrument;

#[instrument]
async fn sync_files() {
    // Automatically creates span with function name
    info!("Syncing files");
}
```

## Configuration

### LoggingConfig Builder

```rust
let config = LoggingConfig::default()
    .with_format(LogFormat::Json)           // Output format
    .with_level(LogLevel::Debug)            // Minimum log level
    .with_pii_redaction(true)               // Enable PII redaction
    .with_filter("core_auth=trace")         // Custom filter
    .with_spans(true)                       // Enable span contexts
    .with_target(true)                      // Display target module
    .with_thread_info(true);                // Display thread info

init_logging(config)?;
```

### Default Configuration

Debug builds:
- Format: Pretty
- Level: Info
- PII Redaction: Enabled
- Spans: Enabled

Release builds:
- Format: JSON
- Level: Info
- PII Redaction: Enabled
- Spans: Enabled

## Integration with Host Logging

Forward logs to platform-specific logging systems:

```rust
use bridge_traits::time::LoggerSink;
use std::sync::Arc;

// Create platform-specific logger sink (iOS, Android, etc.)
let logger_sink: Arc<dyn LoggerSink> = create_platform_logger();

let config = LoggingConfig::default()
    .with_logger_sink(logger_sink);

init_logging(config)?;
```

Platform implementations:
- **iOS**: Forward to OSLog
- **Android**: Forward to Logcat
- **Desktop**: Console output (default)
- **Web**: Browser console API

## Usage Examples

### Basic Logging

```rust
use tracing::{trace, debug, info, warn, error};

trace!("Detailed trace information");
debug!(user_id = "123", "User action");
info!(count = 42, "Items processed");
warn!("Potential issue detected");
error!(error = %err, "Operation failed");
```

### Structured Fields

```rust
info!(
    track_id = "abc123",
    title = "Song Title",
    duration_ms = 245000,
    bitrate = 320,
    "Track metadata extracted"
);
```

### Nested Spans

```rust
let outer_span = span!(Level::INFO, "sync_operation");
let _outer = outer_span.enter();

info!("Starting sync");

{
    let inner_span = span!(Level::DEBUG, "download");
    let _inner = inner_span.enter();
    
    debug!(file_count = 150, "Downloading files");
}

info!("Sync complete");
```

### Error Context

```rust
use tracing::error;

match risky_operation().await {
    Ok(result) => info!(result = ?result, "Operation successful"),
    Err(e) => error!(
        error = %e,
        context = "during file sync",
        "Operation failed"
    ),
}
```

## Best Practices

### 1. Don't Log Sensitive Data

```rust
// ❌ BAD - Logs token in plain text
info!(access_token = token, "Got token");

// ✅ GOOD - Don't log sensitive values
info!("Authentication successful");

// ✅ ACCEPTABLE - Use redaction helper if needed
info!(token = %redact_if_sensitive("access_token", token), "Token retrieved");
```

### 2. Use Appropriate Log Levels

- **TRACE**: Very detailed, execution flow
- **DEBUG**: Detailed diagnostic information
- **INFO**: General informational messages
- **WARN**: Potential issues, degraded functionality
- **ERROR**: Errors that need attention

### 3. Add Context to Errors

```rust
// ❌ BAD - No context
error!("Failed");

// ✅ GOOD - Descriptive context
error!(
    provider = "google_drive",
    operation = "list_files",
    error = %err,
    "Failed to list files from provider"
);
```

### 4. Use Structured Fields

```rust
// ❌ BAD - String formatting
info!("Processed {} files in {} ms", count, duration);

// ✅ GOOD - Structured fields
info!(
    files_processed = count,
    duration_ms = duration,
    "File processing complete"
);
```

### 5. Use Spans for Operation Context

```rust
// ✅ Wrap operations in spans
let span = span!(Level::INFO, "sync_operation", provider = "google_drive");
let _enter = span.enter();

// All logs in this scope include span context
info!("Starting sync");
process_files().await;
info!("Sync complete");
```

## Performance Considerations

### Log Level Filtering

Set appropriate log levels to reduce overhead:
```rust
// Production: Only warnings and errors
let config = LoggingConfig::default()
    .with_level(LogLevel::Warn);

// Development: Debug information
let config = LoggingConfig::default()
    .with_level(LogLevel::Debug);
```

### Lazy Evaluation

Use the `?` operator for expensive operations:
```rust
// ✅ Only evaluates if DEBUG is enabled
debug!(data = ?expensive_to_debug_value, "Debug info");
```

### Selective Module Filtering

Enable detailed logging only for specific modules:
```rust
let config = LoggingConfig::default()
    .with_level(LogLevel::Info)
    .with_filter("core_sync=debug");  // Only core_sync at DEBUG
```

## Testing

The logging system includes comprehensive tests:

```bash
# Run all tests
cargo test --package core-runtime

# Run logging-specific integration tests
cargo test --package core-runtime --test logging_integration

# Run example
cargo run --package core-runtime --example logging_demo
cargo run --package core-runtime --example logging_demo json
```

## Migration Guide

For existing code using `println!` or other logging:

```rust
// Before
println!("User {} logged in", user_id);
eprintln!("Error: {}", err);

// After
use tracing::info;
info!(user_id = %user_id, "User logged in");
error!(error = %err, "Operation failed");
```

## Architecture

The logging system consists of:

1. **LoggingConfig**: Configuration builder
2. **init_logging()**: One-time initialization
3. **PiiRedactionLayer**: Automatic PII filtering
4. **Helper functions**: `redact_if_sensitive()`, `strip_path()`
5. **Bridge integration**: `LoggerSink` trait for platform forwarding

## Dependencies

- `tracing`: Core tracing library
- `tracing-subscriber`: Subscriber implementations (JSON, pretty, etc.)
- `bridge-traits`: Platform abstraction traits

## Further Reading

- [Tracing Documentation](https://docs.rs/tracing/)
- [Tracing Subscriber Documentation](https://docs.rs/tracing-subscriber/)
- [Instrumentation Best Practices](https://tokio.rs/tokio/topics/tracing)
- Task List: See `docs/ai_task_list.md` - TASK-004

## Acceptance Criteria ✅

All acceptance criteria from TASK-004 have been met:

- ✅ Logs are structured with contextual fields
- ✅ PII is automatically redacted
- ✅ Log levels are configurable at runtime
- ✅ Integration with host logging works via `LoggerSink`
- ✅ Multiple output formats (JSON, Pretty, Compact)
- ✅ Module-level filtering
- ✅ Span contexts for distributed tracing
- ✅ Comprehensive tests (14 tests passing)
- ✅ Zero clippy warnings
- ✅ Documentation and examples included
