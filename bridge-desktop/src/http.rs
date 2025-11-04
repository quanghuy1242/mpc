//! HTTP Client Implementation using Reqwest

use async_trait::async_trait;
use bridge_traits::{
    error::{BridgeError, Result},
    http::{HttpClient, HttpMethod, HttpRequest, HttpResponse, RetryPolicy},
};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Reqwest-based HTTP client implementation
///
/// Provides HTTP operations with:
/// - Connection pooling via reqwest
/// - Automatic retry with exponential backoff
/// - TLS support by default
/// - Async streaming
pub struct ReqwestHttpClient {
    client: Client,
}

impl ReqwestHttpClient {
    /// Create a new HTTP client with default configuration
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(30))
    }

    /// Create a new HTTP client with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .user_agent("music-platform-core/0.1.0")
            .build()
            .expect("Failed to build HTTP client");

        Self { client }
    }

    /// Create a new HTTP client with custom configuration
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Convert bridge HttpMethod to reqwest Method
    fn convert_method(method: HttpMethod) -> reqwest::Method {
        match method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
        }
    }

    /// Build reqwest request from bridge request
    fn build_request(&self, request: HttpRequest) -> reqwest::RequestBuilder {
        let method = Self::convert_method(request.method);
        let mut req = self.client.request(method, &request.url);

        // Add headers
        for (key, value) in request.headers {
            req = req.header(key, value);
        }

        // Add body if present
        if let Some(body) = request.body {
            req = req.body(body);
        }

        // Add timeout if specified
        if let Some(timeout) = request.timeout {
            req = req.timeout(timeout);
        }

        req
    }

    /// Execute request with retry logic
    async fn execute_with_retry_internal(
        &self,
        request: HttpRequest,
        policy: RetryPolicy,
    ) -> Result<HttpResponse> {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt < policy.max_attempts {
            debug!(
                attempt = attempt + 1,
                max_attempts = policy.max_attempts,
                url = %request.url,
                "Executing HTTP request"
            );

            let req_builder = self.build_request(request.clone());

            match req_builder.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();

                    // Check if we should retry on this status code
                    if status >= 500 || status == 429 {
                        warn!(
                            status = status,
                            attempt = attempt + 1,
                            "HTTP request failed with retryable status"
                        );
                        last_error = Some(BridgeError::OperationFailed(format!(
                            "HTTP {} error",
                            status
                        )));
                    } else {
                        // Success or non-retryable error
                        let headers: HashMap<String, String> = response
                            .headers()
                            .iter()
                            .filter_map(|(k, v)| {
                                v.to_str().ok().map(|s| (k.to_string(), s.to_string()))
                            })
                            .collect();

                        let body = response
                            .bytes()
                            .await
                            .map_err(|e| BridgeError::OperationFailed(e.to_string()))?;

                        return Ok(HttpResponse {
                            status,
                            headers,
                            body,
                        });
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        attempt = attempt + 1,
                        "HTTP request failed"
                    );

                    if e.is_timeout() {
                        last_error = Some(BridgeError::OperationFailed(
                            "Request timed out".to_string(),
                        ));
                    } else if e.is_connect() {
                        last_error = Some(BridgeError::OperationFailed(format!(
                            "Connection failed: {}",
                            e
                        )));
                    } else {
                        last_error = Some(BridgeError::OperationFailed(e.to_string()));
                    }
                }
            }

            attempt += 1;

            // If we're going to retry, wait with exponential backoff
            if attempt < policy.max_attempts {
                let delay = if policy.use_exponential_backoff {
                    let exponential_delay = policy.base_delay * 2u32.pow(attempt - 1);
                    exponential_delay.min(policy.max_delay)
                } else {
                    policy.base_delay
                };

                debug!(delay_ms = delay.as_millis(), "Retrying after delay");
                sleep(delay).await;
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            BridgeError::OperationFailed("All retry attempts exhausted".to_string())
        }))
    }
}

impl Default for ReqwestHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        // Use default retry policy
        self.execute_with_retry(request, RetryPolicy::default())
            .await
    }

    async fn execute_with_retry(
        &self,
        request: HttpRequest,
        policy: RetryPolicy,
    ) -> Result<HttpResponse> {
        self.execute_with_retry_internal(request, policy).await
    }

    async fn download_stream(
        &self,
        url: String,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BridgeError::OperationFailed(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BridgeError::OperationFailed(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let stream = response.bytes_stream().map_err(std::io::Error::other);

        use futures_util::TryStreamExt;
        let reader = tokio_util::io::StreamReader::new(stream);

        Ok(Box::new(reader))
    }

    async fn is_connected(&self) -> bool {
        self.client
            .head("https://www.google.com")
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_client_creation() {
        let _client = ReqwestHttpClient::new();
        // Just verify it constructs
    }

    #[tokio::test]
    async fn test_method_conversion() {
        assert_eq!(
            ReqwestHttpClient::convert_method(HttpMethod::Get),
            reqwest::Method::GET
        );
        assert_eq!(
            ReqwestHttpClient::convert_method(HttpMethod::Post),
            reqwest::Method::POST
        );
    }
}
