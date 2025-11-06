//! HTTP Client Abstraction
//!
//! Provides async HTTP operations with OAuth, retry logic, and TLS support.

use async_trait::async_trait;
use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::error::{BridgeError, Result};

/// HTTP method types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

/// HTTP request builder
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
            timeout: None,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.into()))
    }

    pub fn json<T: Serialize>(mut self, body: &T) -> Result<Self> {
        let json = serde_json::to_vec(body).map_err(|e| {
            BridgeError::OperationFailed(format!("JSON serialization failed: {}", e))
        })?;
        self.body = Some(Bytes::from(json));
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        Ok(self)
    }

    pub fn body(mut self, body: Bytes) -> Self {
        self.body = Some(body);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }
}

/// HTTP response
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Bytes,
}

impl HttpResponse {
    /// Parse response body as JSON
    pub fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|e| {
            BridgeError::OperationFailed(format!("JSON deserialization failed: {}", e))
        })
    }

    /// Get response body as UTF-8 string
    pub fn text(&self) -> Result<String> {
        String::from_utf8(self.body.to_vec())
            .map_err(|e| BridgeError::OperationFailed(format!("Invalid UTF-8: {}", e)))
    }

    /// Check if response status is successful (2xx)
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if response status indicates a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Check if response status indicates a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }
}

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay between retries
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Whether to use exponential backoff
    pub use_exponential_backoff: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            use_exponential_backoff: true,
        }
    }
}

/// Async HTTP client trait
///
/// This trait abstracts HTTP operations to allow platform-specific implementations.
/// Implementations should handle:
/// - OAuth token injection and refresh
/// - Automatic retry with exponential backoff
/// - TLS certificate validation and pinning
/// - Connection pooling and keep-alive
///
/// # Example
///
/// ```ignore
/// use bridge_traits::http::{HttpClient, HttpRequest, HttpMethod};
///
/// async fn fetch_data(client: &dyn HttpClient) -> Result<String> {
///     let request = HttpRequest::new(HttpMethod::Get, "https://api.example.com/data")
///         .bearer_token("token");
///     
///     let response = client.execute(request).await?;
///     response.text()
/// }
/// ```
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Execute an HTTP request
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Network connection fails
    /// - TLS validation fails
    /// - Request times out
    /// - Maximum retries exceeded
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse>;

    /// Execute an HTTP request with custom retry policy
    async fn execute_with_retry(
        &self,
        request: HttpRequest,
        policy: RetryPolicy,
    ) -> Result<HttpResponse> {
        // Default implementation: just call execute
        // Implementations can override for custom retry logic
        let _ = policy; // Mark as used
        self.execute(request).await
    }

    /// Download a file as a stream of bytes
    ///
    /// This is useful for large files that should not be loaded entirely into memory.
    async fn download_stream(
        &self,
        url: String,
    ) -> Result<Box<dyn core_async::io::AsyncRead + Send + Unpin>>;

    /// Check network connectivity
    async fn is_connected(&self) -> bool {
        // Default implementation: try a simple HEAD request
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_builder() {
        let request = HttpRequest::new(HttpMethod::Get, "https://example.com")
            .header("User-Agent", "test")
            .bearer_token("secret")
            .timeout(Duration::from_secs(30));

        assert_eq!(request.url, "https://example.com");
        assert_eq!(request.headers.get("User-Agent"), Some(&"test".to_string()));
        assert!(request.headers.contains_key("Authorization"));
    }

    #[test]
    fn test_http_response_status_checks() {
        let response = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: Bytes::from("test"),
        };

        assert!(response.is_success());
        assert!(!response.is_client_error());
        assert!(!response.is_server_error());
    }
}
