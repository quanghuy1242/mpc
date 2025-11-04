use thiserror::Error;

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Entity not found: {entity_type} with id {id}")]
    NotFound { entity_type: String, id: String },

    #[error("Invalid input: {field} - {message}")]
    InvalidInput { field: String, message: String },

    #[error("Migration failed: {0}")]
    Migration(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;
