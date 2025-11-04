use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Failed to extract metadata: {0}")]
    ExtractionFailed(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Artwork processing failed: {0}")]
    ArtworkError(String),

    #[error("Lyrics fetch failed: {0}")]
    LyricsFetchFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MetadataError>;
