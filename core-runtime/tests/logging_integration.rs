//! Integration tests for logging system

use bridge_traits::time::LogLevel;
use core_runtime::logging::{redact_if_sensitive, strip_path, LogFormat, LoggingConfig};

#[test]
fn test_logging_initialization() {
    // Test that we can initialize logging with different configurations
    // Note: We can only initialize once per process, so we test the config builder

    let config = LoggingConfig::default()
        .with_format(LogFormat::Json)
        .with_level(LogLevel::Debug)
        .with_pii_redaction(true)
        .with_spans(true);

    assert_eq!(config.format, LogFormat::Json);
    assert_eq!(config.level, LogLevel::Debug);
    assert!(config.redact_pii);
    assert!(config.enable_spans);
}

#[test]
fn test_pii_redaction_tokens() {
    let token = "sensitive_access_token";
    let redacted = redact_if_sensitive("access_token", token);
    assert_eq!(redacted, "[REDACTED]");

    let refresh = "refresh_token_value";
    let redacted = redact_if_sensitive("refresh_token", refresh);
    assert_eq!(redacted, "[REDACTED]");

    let password = "my_password";
    let redacted = redact_if_sensitive("password", password);
    assert_eq!(redacted, "[REDACTED]");
}

#[test]
fn test_pii_redaction_emails() {
    let email = "user@example.com";
    let redacted = redact_if_sensitive("email", email);

    // Should start with first char
    assert!(redacted.starts_with('u'));
    // Should contain redacted marker
    assert!(redacted.contains("[REDACTED]"));
    // Should not contain full email
    assert!(!redacted.contains("example.com"));
}

#[test]
fn test_pii_redaction_normal_values() {
    // Normal values should pass through unchanged
    assert_eq!(redact_if_sensitive("track_id", "12345"), "12345");
    assert_eq!(redact_if_sensitive("title", "Song Name"), "Song Name");
    assert_eq!(redact_if_sensitive("user_id", "user_123"), "user_123");
}

#[test]
fn test_path_stripping() {
    // Unix paths
    assert_eq!(strip_path("/home/user/music/song.mp3"), "song.mp3");
    assert_eq!(strip_path("/var/log/app.log"), "app.log");

    // Windows paths
    assert_eq!(strip_path("C:\\Users\\John\\Music\\song.mp3"), "song.mp3");
    assert_eq!(strip_path("D:\\data\\file.txt"), "file.txt");

    // Already basename
    assert_eq!(strip_path("filename.txt"), "filename.txt");

    // Edge cases
    assert_eq!(strip_path("/var/log/"), "");
    assert_eq!(strip_path(""), "");
}

#[test]
fn test_format_selection() {
    // Debug builds should default to Pretty
    #[cfg(debug_assertions)]
    {
        let config = LoggingConfig::default();
        assert_eq!(config.format, LogFormat::Pretty);
    }

    // Release builds should default to JSON
    #[cfg(not(debug_assertions))]
    {
        let config = LoggingConfig::default();
        assert_eq!(config.format, LogFormat::Json);
    }
}

#[test]
fn test_filter_configuration() {
    let config = LoggingConfig::default().with_filter("core_auth=debug,core_sync=trace");

    assert_eq!(
        config.filter,
        Some("core_auth=debug,core_sync=trace".to_string())
    );
}

#[test]
fn test_config_chaining() {
    let config = LoggingConfig::default()
        .with_format(LogFormat::Compact)
        .with_level(LogLevel::Warn)
        .with_pii_redaction(false)
        .with_spans(false)
        .with_target(false)
        .with_thread_info(true);

    assert_eq!(config.format, LogFormat::Compact);
    assert_eq!(config.level, LogLevel::Warn);
    assert!(!config.redact_pii);
    assert!(!config.enable_spans);
    assert!(!config.display_target);
    assert!(config.display_thread_info);
}
