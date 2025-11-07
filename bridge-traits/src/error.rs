use thiserror::Error;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("Bridge capability not available: {0}")]
    NotAvailable(String),

    #[error("Bridge operation failed: {0}")]
    OperationFailed(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
