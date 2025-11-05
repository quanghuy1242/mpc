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

    #[error("Invalid job ID: {0}")]
    InvalidJobId(String),

    #[error("Invalid sync status: {0}")]
    InvalidStatus(String),

    #[error("Invalid sync type: {0}")]
    InvalidSyncType(String),

    #[error("Invalid state transition from {from} to {to}: {reason}")]
    InvalidStateTransition {
        from: String,
        to: String,
        reason: String,
    },

    #[error("Database error: {0}")]
    Database(String),
}

pub type Result<T> = std::result::Result<T, SyncError>;
