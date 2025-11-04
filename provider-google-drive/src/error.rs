use thiserror::Error;

#[derive(Error, Debug)]
pub enum GoogleDriveError {
    #[error("API request failed: {0}")]
    ApiError(String),

    #[error("Rate limit exceeded, retry after {0} seconds")]
    RateLimitExceeded(u64),

    #[error("Authentication required")]
    AuthRequired,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, GoogleDriveError>;
