use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Sync job {job_id} not found")]
    JobNotFound { job_id: String },

    #[error("Sync already in progress for profile {profile_id}")]
    SyncInProgress { profile_id: String },

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Sync timeout after {0} seconds")]
    Timeout(u64),

    #[error("Sync cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, SyncError>;
