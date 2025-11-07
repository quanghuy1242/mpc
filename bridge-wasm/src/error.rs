//! Error types for WebAssembly bridge implementations

use thiserror::Error;
use wasm_bindgen::JsCast;

/// Result type for WebAssembly bridge operations
pub type WasmResult<T> = Result<T, WasmError>;

/// Errors that can occur in WebAssembly bridge implementations
#[derive(Error, Debug)]
pub enum WasmError {
    /// IndexedDB operation failed
    #[error("IndexedDB error: {0}")]
    IndexedDb(String),

    /// JavaScript error from web-sys
    #[error("JavaScript error: {0}")]
    JavaScript(String),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Directory not found
    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),

    /// Path already exists
    #[error("Path already exists: {0}")]
    AlreadyExists(String),

    /// Invalid path
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Not a directory
    #[error("Not a directory: {0}")]
    NotADirectory(String),

    /// Not a file
    #[error("Not a file: {0}")]
    NotAFile(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// Database not initialized
    #[error("Database not initialized")]
    NotInitialized,

    /// Operation cancelled
    #[error("Operation cancelled")]
    Cancelled,

    /// Unsupported operation
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

impl From<WasmError> for bridge_traits::error::BridgeError {
    fn from(err: WasmError) -> Self {
        bridge_traits::error::BridgeError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            err.to_string(),
        ))
    }
}

impl From<wasm_bindgen::JsValue> for WasmError {
    fn from(js_value: wasm_bindgen::JsValue) -> Self {
        let msg = if js_value.is_string() {
            js_value
                .as_string()
                .unwrap_or_else(|| "Unknown error".to_string())
        } else if let Some(error) = js_value.dyn_ref::<js_sys::Error>() {
            error.message().into()
        } else {
            format!("{:?}", js_value)
        };
        WasmError::JavaScript(msg)
    }
}
