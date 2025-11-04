use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Provider {provider} authentication failed: {reason}")]
    AuthenticationFailed { provider: String, reason: String },

    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),

    #[error("Secure storage unavailable: {0}")]
    SecureStorageUnavailable(String),

    #[error("Invalid provider: {0}")]
    InvalidProvider(String),

    #[error("Not authenticated")]
    NotAuthenticated,
}

pub type Result<T> = std::result::Result<T, AuthError>;
