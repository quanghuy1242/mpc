use thiserror::Error;

#[derive(Error, Debug)]
pub enum OneDriveError {
    #[error("API request failed: {0}")]
    ApiError(String),

    #[error("Throttled, retry after {0} seconds")]
    Throttled(u64),

    #[error("Authentication required")]
    AuthRequired,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, OneDriveError>;
