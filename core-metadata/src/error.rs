use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Failed to extract metadata: {0}")]
    ExtractionFailed(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Corrupted file: {0}")]
    CorruptedFile(String),

    #[error("Artwork processing failed: {0}")]
    ArtworkError(String),

    #[error("Artwork not found: {artwork_id}")]
    ArtworkNotFound { artwork_id: String },

    #[error("Lyrics fetch failed: {0}")]
    LyricsFetchFailed(String),

    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Image processing error: {message}")]
    ImageProcessing { message: String },

    #[error("Database error: {0}")]
    Database(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image error: {0}")]
    ImageError(String),

    #[error("Bridge error: {0}")]
    Bridge(#[from] bridge_traits::error::BridgeError),

    #[error("Library error: {0}")]
    Library(#[from] core_library::error::LibraryError),

    #[error("Remote API error: {0}")]
    RemoteApi(String),

    #[error("Rate limited by {provider}: retry after {retry_after_seconds}s")]
    RateLimited {
        provider: String,
        retry_after_seconds: u64,
    },

    #[error("HTTP error: status {status}, body: {body}")]
    HttpError { status: u16, body: String },

    #[error("JSON parsing failed: {0}")]
    JsonParse(String),

    #[error("Remote artwork not found for '{artist} - {album}'")]
    ArtworkNotFoundRemote { artist: String, album: String },

    #[error("API configuration missing: {0}")]
    ApiConfigMissing(String),

    #[error("Network error: {0}")]
    NetworkError(String),
}

pub type Result<T> = std::result::Result<T, MetadataError>;
