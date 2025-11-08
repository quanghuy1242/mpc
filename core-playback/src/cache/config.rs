//! Cache configuration and policies

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the offline cache manager.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size in bytes (default: 5GB)
    pub max_cache_size_bytes: u64,

    /// Eviction policy when cache is full
    pub eviction_policy: EvictionPolicy,

    /// Whether to encrypt cached files (requires 'offline-cache' feature)
    pub enable_encryption: bool,

    /// Download timeout for individual track (default: 300s)
    pub download_timeout: Duration,

    /// Number of concurrent downloads allowed (default: 2)
    pub max_concurrent_downloads: usize,

    /// Verify file integrity after download using hash (default: true)
    pub verify_integrity: bool,

    /// Retry failed downloads automatically (default: 3 attempts)
    pub max_retry_attempts: usize,

    /// Base directory for cache files (relative to app data dir)
    pub cache_directory: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_cache_size_bytes: 5 * 1024 * 1024 * 1024, // 5GB
            eviction_policy: EvictionPolicy::LeastRecentlyUsed,
            enable_encryption: true,
            download_timeout: Duration::from_secs(300),
            max_concurrent_downloads: 2,
            verify_integrity: true,
            max_retry_attempts: 3,
            cache_directory: "offline_cache".to_string(),
        }
    }
}

impl CacheConfig {
    /// Create a new cache configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum cache size.
    pub fn with_max_size(mut self, bytes: u64) -> Self {
        self.max_cache_size_bytes = bytes;
        self
    }

    /// Set eviction policy.
    pub fn with_eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }

    /// Enable or disable encryption.
    pub fn with_encryption(mut self, enabled: bool) -> Self {
        self.enable_encryption = enabled;
        self
    }

    /// Set download timeout.
    pub fn with_download_timeout(mut self, timeout: Duration) -> Self {
        self.download_timeout = timeout;
        self
    }

    /// Set maximum concurrent downloads.
    pub fn with_max_concurrent_downloads(mut self, count: usize) -> Self {
        self.max_concurrent_downloads = count;
        self
    }

    /// Set cache directory name.
    pub fn with_cache_directory(mut self, dir: String) -> Self {
        self.cache_directory = dir;
        self
    }

    /// Validate configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_cache_size_bytes == 0 {
            return Err("max_cache_size_bytes must be greater than 0".to_string());
        }

        if self.max_concurrent_downloads == 0 {
            return Err("max_concurrent_downloads must be at least 1".to_string());
        }

        if self.cache_directory.is_empty() {
            return Err("cache_directory cannot be empty".to_string());
        }

        Ok(())
    }
}

/// Policy for evicting tracks when cache is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionPolicy {
    /// Remove tracks that haven't been played recently
    LeastRecentlyUsed,

    /// Remove tracks that have been played the least
    LeastFrequentlyUsed,

    /// Remove oldest downloaded tracks first
    FirstInFirstOut,

    /// Remove largest tracks first to free more space
    LargestFirst,
}

impl EvictionPolicy {
    /// Returns a human-readable description of the policy.
    pub fn description(&self) -> &'static str {
        match self {
            EvictionPolicy::LeastRecentlyUsed => {
                "Remove tracks that haven't been played recently"
            }
            EvictionPolicy::LeastFrequentlyUsed => "Remove tracks that have been played the least",
            EvictionPolicy::FirstInFirstOut => "Remove oldest downloaded tracks first",
            EvictionPolicy::LargestFirst => "Remove largest tracks first to free more space",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CacheConfig::default();
        assert_eq!(config.max_cache_size_bytes, 5 * 1024 * 1024 * 1024);
        assert_eq!(config.eviction_policy, EvictionPolicy::LeastRecentlyUsed);
        assert!(config.enable_encryption);
        assert!(config.verify_integrity);
    }

    #[test]
    fn test_config_builder() {
        let config = CacheConfig::new()
            .with_max_size(1024 * 1024 * 1024)
            .with_eviction_policy(EvictionPolicy::FirstInFirstOut)
            .with_encryption(false)
            .with_max_concurrent_downloads(4);

        assert_eq!(config.max_cache_size_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.eviction_policy, EvictionPolicy::FirstInFirstOut);
        assert!(!config.enable_encryption);
        assert_eq!(config.max_concurrent_downloads, 4);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = CacheConfig::default();
        assert!(valid_config.validate().is_ok());

        let invalid_size = CacheConfig::default().with_max_size(0);
        assert!(invalid_size.validate().is_err());

        let invalid_downloads = CacheConfig::default().with_max_concurrent_downloads(0);
        assert!(invalid_downloads.validate().is_err());

        let invalid_dir = CacheConfig::default().with_cache_directory(String::new());
        assert!(invalid_dir.validate().is_err());
    }

    #[test]
    fn test_eviction_policy_descriptions() {
        assert!(!EvictionPolicy::LeastRecentlyUsed.description().is_empty());
        assert!(!EvictionPolicy::LeastFrequentlyUsed
            .description()
            .is_empty());
        assert!(!EvictionPolicy::FirstInFirstOut.description().is_empty());
        assert!(!EvictionPolicy::LargestFirst.description().is_empty());
    }
}
