//! Async filesystem helpers re-exported from the underlying runtime.
//!
//! On native targets this simply re-exports Tokio's `fs` module. The APIs are
//! intentionally kept identical so downstream crates can rely on the familiar
//! surface area without depending on Tokio directly.

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::fs::{
    self, copy, create_dir, create_dir_all, hard_link, metadata, read, read_dir, read_link,
    read_to_string, remove_dir, remove_dir_all, remove_file, rename, set_permissions,
    symlink_metadata, write, DirBuilder, DirEntry, File, OpenOptions,
};

#[cfg(target_arch = "wasm32")]
compile_error!("core_async::fs is not yet supported on wasm targets");
