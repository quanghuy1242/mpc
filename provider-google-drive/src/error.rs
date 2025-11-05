//! Error types for Google Drive provider

use thiserror::Error;

/// Google Drive provider errors
#[derive(Error, Debug)]
pub enum GoogleDriveError {
    /// Authentication failed or token is invalid
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// API request returned an error
    #[error("Google Drive API error (status {status_code}): {message}")]
    ApiError { status_code: u16, message: String },

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_seconds} seconds")]
    RateLimitExceeded { retry_after_seconds: u64 },

    /// File not found
    #[error("File not found: {file_id}")]
    FileNotFound { file_id: String },

    /// Failed to parse API response
    #[error("Failed to parse API response: {0}")]
    ParseError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Invalid change token
    #[error("Invalid or expired change token: {0}")]
    InvalidChangeToken(String),

    /// Bridge error
    #[error(transparent)]
    BridgeError(#[from] bridge_traits::error::BridgeError),
}

/// Result type for Google Drive operations
pub type Result<T> = std::result::Result<T, GoogleDriveError>;

impl From<GoogleDriveError> for bridge_traits::error::BridgeError {
    fn from(error: GoogleDriveError) -> Self {
        match error {
            GoogleDriveError::AuthenticationFailed(msg) => {
                bridge_traits::error::BridgeError::OperationFailed(format!(
                    "Authentication failed: {}",
                    msg
                ))
            }
            GoogleDriveError::ApiError {
                status_code,
                message,
            } => bridge_traits::error::BridgeError::OperationFailed(format!(
                "API error (status {}): {}",
                status_code, message
            )),
            GoogleDriveError::RateLimitExceeded {
                retry_after_seconds,
            } => bridge_traits::error::BridgeError::OperationFailed(format!(
                "Rate limit exceeded, retry after {} seconds",
                retry_after_seconds
            )),
            GoogleDriveError::FileNotFound { file_id } => {
                bridge_traits::error::BridgeError::OperationFailed(format!(
                    "File not found: {}",
                    file_id
                ))
            }
            GoogleDriveError::ParseError(msg) => {
                bridge_traits::error::BridgeError::OperationFailed(format!("Parse error: {}", msg))
            }
            GoogleDriveError::NetworkError(msg) => {
                bridge_traits::error::BridgeError::OperationFailed(format!(
                    "Network error: {}",
                    msg
                ))
            }
            GoogleDriveError::InvalidChangeToken(msg) => {
                bridge_traits::error::BridgeError::OperationFailed(format!(
                    "Invalid change token: {}",
                    msg
                ))
            }
            GoogleDriveError::BridgeError(e) => e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = GoogleDriveError::ApiError {
            status_code: 404,
            message: "File not found".to_string(),
        };

        assert_eq!(
            error.to_string(),
            "Google Drive API error (status 404): File not found"
        );
    }

    #[test]
    fn test_error_conversion() {
        let error = GoogleDriveError::AuthenticationFailed("Token expired".to_string());
        let bridge_error: bridge_traits::error::BridgeError = error.into();

        assert!(matches!(
            bridge_error,
            bridge_traits::error::BridgeError::OperationFailed(_)
        ));
    }
}
