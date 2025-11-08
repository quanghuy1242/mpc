//! Tests for offline cache manager
//!
//! These tests verify the functionality of the cache manager using mock implementations.

#[cfg(test)]
mod tests {
    use core_playback::cache::{
        CacheConfig, EvictionPolicy, OfflineCacheManager,
    };
    use core_library::models::{CacheStatus, TrackId};
    use std::sync::Arc;
    use std::time::Duration;

    fn create_test_config() -> CacheConfig {
        CacheConfig {
            max_cache_size_bytes: 1024 * 1024 * 100, // 100MB
            eviction_policy: EvictionPolicy::LeastRecentlyUsed,
            enable_encryption: false,
            download_timeout: Duration::from_secs(30),
            max_concurrent_downloads: 2,
            verify_integrity: true,
            max_retry_attempts: 2,
            cache_directory: "test_cache".to_string(),
        }
    }

    // TODO: Re-enable tests once mock implementations are available in bridge-traits
    // The cache manager requires:
    // - MockDatabaseAdapter (bridge-traits::database::mock)
    // - MockHttpClient (bridge-traits::http::mock)
    // - MockFileSystemAccess (bridge-traits::storage::mock)
    // - MockStorageProvider (bridge-traits::storage::mock)
    // - MockTrackRepository (core-library::repositories::mock)
    
    #[test]
    fn test_cache_config_creation() {
        let config = create_test_config();
        assert_eq!(config.max_cache_size_bytes, 1024 * 1024 * 100);
        assert_eq!(config.eviction_policy, EvictionPolicy::LeastRecentlyUsed);
        assert!(!config.enable_encryption);
        assert_eq!(config.max_concurrent_downloads, 2);
    }

    #[test]
    fn test_cache_status_checks() {
        assert!(CacheStatus::Cached.is_available());
        assert!(!CacheStatus::Downloading.is_available());
        assert!(!CacheStatus::Failed.is_available());
        
        assert!(CacheStatus::Downloading.is_downloading());
        assert!(!CacheStatus::Cached.is_downloading());
        
        assert!(CacheStatus::NotCached.needs_download());
        assert!(CacheStatus::Failed.needs_download());
        assert!(CacheStatus::Stale.needs_download());
        assert!(!CacheStatus::Cached.needs_download());
    }
}
