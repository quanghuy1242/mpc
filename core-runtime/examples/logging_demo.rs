//! Logging system demonstration
//!
//! This example shows how to use the logging infrastructure in different modes.
//!
//! Run with:
//! ```bash
//! # Pretty format (default in debug)
//! cargo run --example logging_demo
//!
//! # JSON format
//! cargo run --example logging_demo -- json
//!
//! # Compact format
//! cargo run --example logging_demo -- compact
//!
//! # With custom filter
//! cargo run --example logging_demo -- pretty "core_runtime=trace"
//! ```

use bridge_traits::time::LogLevel;
use core_runtime::logging::{
    init_logging, redact_if_sensitive, strip_path, LogFormat, LoggingConfig,
};
use std::env;
use tracing::{debug, error, info, instrument, span, trace, warn, Level};

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let format = if args.len() > 1 {
        match args[1].as_str() {
            "json" => LogFormat::Json,
            "compact" => LogFormat::Compact,
            "pretty" => LogFormat::Pretty,
            _ => LogFormat::Pretty,
        }
    } else {
        LogFormat::default()
    };

    let filter = args.get(2).cloned();

    // Initialize logging
    let mut config = LoggingConfig::default()
        .with_format(format)
        .with_level(LogLevel::Trace)
        .with_pii_redaction(true)
        .with_spans(true)
        .with_target(true);

    if let Some(f) = filter {
        config = config.with_filter(f);
    }

    init_logging(config).expect("Failed to initialize logging");

    info!("=== Logging System Demo ===");
    info!(format = ?format, "Logging initialized");

    // Demonstrate different log levels
    demo_log_levels();

    // Demonstrate structured logging
    demo_structured_logging();

    // Demonstrate spans for tracing
    demo_spans().await;

    // Demonstrate PII redaction
    demo_pii_redaction();

    // Demonstrate instrumentation
    demo_instrumentation().await;

    info!("=== Demo Complete ===");
}

fn demo_log_levels() {
    let span = span!(Level::INFO, "log_levels");
    let _enter = span.enter();

    trace!("This is a TRACE level log");
    debug!("This is a DEBUG level log");
    info!("This is an INFO level log");
    warn!("This is a WARN level log");
    error!("This is an ERROR level log");
}

fn demo_structured_logging() {
    let span = span!(Level::INFO, "structured_logging");
    let _enter = span.enter();

    info!("Simple message without fields");

    info!(
        track_id = "12345",
        title = "Song Title",
        duration_ms = 245000,
        "Track information"
    );

    info!(
        user_count = 42,
        active_sessions = 7,
        cache_hit_rate = 0.95,
        "System metrics"
    );
}

async fn demo_spans() {
    let span = span!(Level::INFO, "sync_operation", provider = "google_drive");
    let _enter = span.enter();

    info!("Starting sync operation");

    {
        let inner_span = span!(Level::DEBUG, "list_files");
        let _inner = inner_span.enter();

        debug!(count = 150, "Listed files from provider");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    {
        let inner_span = span!(Level::DEBUG, "download_metadata");
        let _inner = inner_span.enter();

        debug!(processed = 50, total = 150, "Downloading metadata");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    info!(files_synced = 150, "Sync operation completed");
}

fn demo_pii_redaction() {
    let span = span!(Level::INFO, "pii_redaction");
    let _enter = span.enter();

    // These values will be automatically redacted by our helper
    let token = "secret_access_token_12345";
    let email = "user@example.com";
    let path = "/home/user/private/music/song.mp3";

    info!(
        token = %redact_if_sensitive("access_token", token),
        email = %redact_if_sensitive("email", email),
        file = %strip_path(path),
        "Sensitive data example"
    );

    // Best practice: Don't log sensitive values at all
    info!("Authentication successful for user");
    // Instead of: info!(password = user_password, "Auth successful")
}

#[instrument]
async fn demo_instrumentation() {
    info!("Instrumented function automatically creates spans");

    let items = vec!["item1", "item2", "item3"];
    process_items(&items).await;
}

#[instrument(fields(count = items.len()))]
async fn process_items(items: &[&str]) {
    debug!("Processing items");

    for (idx, item) in items.iter().enumerate() {
        process_item(idx, item).await;
    }

    info!("All items processed");
}

#[instrument(fields(item_id = idx))]
async fn process_item(idx: usize, item: &str) {
    trace!(item = %item, "Processing individual item");
    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
}
