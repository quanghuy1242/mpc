use bridge_traits::error::BridgeError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LibraryError {
    #[cfg(not(target_arch = "wasm32"))]
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Bridge error: {0}")]
    Bridge(#[from] BridgeError),

    #[error("Entity not found: {entity_type} with id {id}")]
    NotFound { entity_type: String, id: String },

    #[error("Invalid input: {field} - {message}")]
    InvalidInput { field: String, message: String },

    #[error("Migration failed: {0}")]
    Migration(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;
