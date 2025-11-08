//! # Offline Cache Module
//!
//! Provides offline caching capabilities for audio tracks with optional encryption.
//!
//! ## Overview
//!
//! The offline cache allows users to download tracks for playback without network access.
//! Key features:
//! - Persistent storage using `FileSystemAccess` trait (native/WASM compatible)
//! - Optional AES-GCM encryption for DRM and privacy
//! - LRU eviction policy with configurable size limits
//! - Database-backed metadata tracking
//! - Progress tracking for downloads
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────┐
//! │     OfflineCacheManager                │
//! │  - download_track()                    │
//! │  - is_cached()                         │
//! │  - evict_oldest()                      │
//! └────────┬───────────────────────────────┘
//!          │
//!          ├──> CacheMetadataRepository (DB)
//!          ├──> FileSystemAccess (Storage)
//!          ├──> StorageProvider (Downloads)
//!          └──> Encryptor (Optional AES-GCM)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use core_playback::cache::{OfflineCacheManager, CacheConfig};
//! use core_library::models::TrackId;
//!
//! # async fn example(manager: &OfflineCacheManager) -> Result<(), Box<dyn std::error::Error>> {
//! let track_id = TrackId::new();
//!
//! // Download track to cache
//! manager.download_track(track_id).await?;
//!
//! // Check if cached
//! if manager.is_cached(&track_id).await? {
//!     println!("Track is available offline");
//! }
//!
//! // Get cache statistics
//! let stats = manager.get_cache_stats().await?;
//! println!("Cache size: {} MB", stats.total_bytes / 1_000_000);
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod encryption;
pub mod manager;
pub mod stats;

// Re-export commonly used types
pub use config::{CacheConfig, EvictionPolicy};
pub use encryption::{CacheEncryptor, EncryptionKey};
pub use manager::OfflineCacheManager;
pub use stats::{CacheStats, DownloadProgress};

// Re-export from core-library
pub use core_library::models::{CachedTrack, CacheStatus};
pub use core_library::models::CacheStats as RepoCacheStats;
pub use core_library::repositories::{CacheMetadataRepository, SqliteCacheMetadataRepository};
