use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Core initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Capability missing: {capability} - {message}")]
    CapabilityMissing { capability: String, message: String },

    #[error("Authentication error: {0}")]
    Auth(#[from] core_auth::AuthError),

    #[error("Sync error: {0}")]
    Sync(#[from] core_sync::SyncError),

    #[error("Library error: {0}")]
    Library(#[from] core_library::LibraryError),

    #[error("Metadata error: {0}")]
    Metadata(#[from] core_metadata::MetadataError),

    #[error("Playback error: {0}")]
    Playback(#[from] core_playback::PlaybackError),
}

pub type Result<T> = std::result::Result<T, CoreError>;
