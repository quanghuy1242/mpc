use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("Streaming failed: {0}")]
    StreamingFailed(String),

    #[error("Decoding error: {0}")]
    DecodingError(String),

    #[error("Unsupported codec: {0}")]
    UnsupportedCodec(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Not authenticated")]
    NotAuthenticated,
}

pub type Result<T> = std::result::Result<T, PlaybackError>;
