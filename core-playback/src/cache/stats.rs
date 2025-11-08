//! Cache statistics and monitoring

use serde::{Deserialize, Serialize};

/// Statistics about the offline cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of tracks in cache
    pub total_tracks: usize,

    /// Number of tracks with status = Cached
    pub cached_tracks: usize,

    /// Number of tracks currently downloading
    pub downloading_tracks: usize,

    /// Number of failed downloads
    pub failed_tracks: usize,

    /// Total bytes used by cache (sum of cached_size)
    pub total_bytes: u64,

    /// Total bytes of original files (sum of file_size)
    pub total_original_bytes: u64,

    /// Number of encrypted tracks
    pub encrypted_tracks: usize,

    /// Total play count across all cached tracks
    pub total_plays: u64,

    /// Number of tracks that need eviction
    pub tracks_pending_eviction: usize,

    /// Timestamp when stats were calculated
    pub calculated_at: i64,
}

impl CacheStats {
    /// Calculate cache usage as a percentage of max size.
    pub fn usage_percentage(&self, max_size: u64) -> f64 {
        if max_size == 0 {
            return 0.0;
        }

        (self.total_bytes as f64 / max_size as f64) * 100.0
    }

    /// Returns true if the cache is near capacity (>90%).
    pub fn is_near_capacity(&self, max_size: u64) -> bool {
        self.usage_percentage(max_size) > 90.0
    }

    /// Returns true if the cache is full (>=100%).
    pub fn is_full(&self, max_size: u64) -> bool {
        self.total_bytes >= max_size
    }

    /// Calculate space that would be freed (in bytes).
    pub fn space_needed(&self, max_size: u64) -> u64 {
        if self.total_bytes <= max_size {
            0
        } else {
            self.total_bytes - max_size
        }
    }

    /// Calculate compression ratio (if encrypted).
    pub fn compression_ratio(&self) -> f64 {
        if self.total_original_bytes == 0 {
            return 1.0;
        }

        self.total_bytes as f64 / self.total_original_bytes as f64
    }

    /// Returns average bytes per track.
    pub fn average_track_size(&self) -> u64 {
        if self.total_tracks == 0 {
            0
        } else {
            self.total_bytes / self.total_tracks as u64
        }
    }

    /// Returns average plays per track.
    pub fn average_plays_per_track(&self) -> f64 {
        if self.cached_tracks == 0 {
            0.0
        } else {
            self.total_plays as f64 / self.cached_tracks as f64
        }
    }

    /// Returns success rate percentage (cached / total).
    pub fn success_rate(&self) -> f64 {
        if self.total_tracks == 0 {
            return 100.0;
        }

        (self.cached_tracks as f64 / self.total_tracks as f64) * 100.0
    }
}

/// Download progress information for a specific track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Track identifier
    pub track_id: String,

    /// Total file size in bytes
    pub total_bytes: u64,

    /// Bytes downloaded so far
    pub downloaded_bytes: u64,

    /// Download progress percentage (0-100)
    pub progress_percent: u8,

    /// Current download speed in bytes/second
    pub speed_bytes_per_sec: u64,

    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,

    /// Download started timestamp
    pub started_at: i64,

    /// Last update timestamp
    pub updated_at: i64,
}

impl DownloadProgress {
    /// Create new download progress tracker.
    pub fn new(track_id: String, total_bytes: u64) -> Self {
        let now = chrono::Utc::now().timestamp();

        Self {
            track_id,
            total_bytes,
            downloaded_bytes: 0,
            progress_percent: 0,
            speed_bytes_per_sec: 0,
            eta_seconds: None,
            started_at: now,
            updated_at: now,
        }
    }

    /// Update progress with new downloaded bytes.
    pub fn update(&mut self, downloaded_bytes: u64) {
        let now = chrono::Utc::now().timestamp();
        let elapsed = (now - self.started_at).max(1) as u64;

        self.downloaded_bytes = downloaded_bytes;
        self.updated_at = now;

        // Calculate progress percentage
        if self.total_bytes > 0 {
            let percent = (downloaded_bytes as f64 / self.total_bytes as f64) * 100.0;
            self.progress_percent = percent.min(100.0) as u8;
        }

        // Calculate download speed
        if elapsed > 0 {
            self.speed_bytes_per_sec = downloaded_bytes / elapsed;
        }

        // Calculate ETA
        if self.speed_bytes_per_sec > 0 {
            let remaining_bytes = self.total_bytes.saturating_sub(downloaded_bytes);
            self.eta_seconds = Some(remaining_bytes / self.speed_bytes_per_sec);
        } else {
            self.eta_seconds = None;
        }
    }

    /// Returns true if download is complete.
    pub fn is_complete(&self) -> bool {
        self.downloaded_bytes >= self.total_bytes
    }

    /// Format speed as human-readable string.
    pub fn speed_string(&self) -> String {
        format_bytes_per_sec(self.speed_bytes_per_sec)
    }

    /// Format ETA as human-readable string.
    pub fn eta_string(&self) -> String {
        match self.eta_seconds {
            Some(secs) => format_duration_seconds(secs),
            None => "calculating...".to_string(),
        }
    }
}

/// Format bytes per second as human-readable string.
fn format_bytes_per_sec(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B/s", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB/s", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB/s", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB/s", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format duration in seconds as human-readable string.
fn format_duration_seconds(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats_percentages() {
        let stats = CacheStats {
            total_tracks: 100,
            cached_tracks: 80,
            downloading_tracks: 10,
            failed_tracks: 10,
            total_bytes: 4_500_000_000, // 4.5GB
            total_original_bytes: 5_000_000_000,
            encrypted_tracks: 80,
            total_plays: 400,
            tracks_pending_eviction: 0,
            calculated_at: chrono::Utc::now().timestamp(),
        };

        let max_size = 5 * 1024 * 1024 * 1024; // 5GB

        // Usage percentage
        let usage = stats.usage_percentage(max_size);
        assert!(usage > 83.0 && usage < 85.0); // ~83.8%

        assert!(!stats.is_full(max_size));
        assert!(!stats.is_near_capacity(max_size));

        // With smaller max size
        let small_max = 4 * 1024 * 1024 * 1024; // 4GB
        assert!(stats.is_full(small_max));
        assert!(stats.is_near_capacity(small_max));
    }

    #[test]
    fn test_cache_stats_calculations() {
        let stats = CacheStats {
            total_tracks: 100,
            cached_tracks: 80,
            downloading_tracks: 10,
            failed_tracks: 10,
            total_bytes: 4_000_000_000,
            total_original_bytes: 5_000_000_000,
            encrypted_tracks: 80,
            total_plays: 400,
            tracks_pending_eviction: 0,
            calculated_at: chrono::Utc::now().timestamp(),
        };

        // Success rate
        assert_eq!(stats.success_rate(), 80.0);

        // Average track size
        assert_eq!(stats.average_track_size(), 40_000_000);

        // Average plays
        assert_eq!(stats.average_plays_per_track(), 5.0);

        // Compression ratio
        let ratio = stats.compression_ratio();
        assert!(ratio > 0.79 && ratio < 0.81); // 0.8
    }

    #[test]
    fn test_space_needed() {
        let stats = CacheStats {
            total_bytes: 6 * 1024 * 1024 * 1024, // 6GB used
            ..Default::default()
        };

        let max_size = 5 * 1024 * 1024 * 1024; // 5GB limit
        let needed = stats.space_needed(max_size);
        assert_eq!(needed, 1024 * 1024 * 1024); // Need to free 1GB

        // When under limit
        let under_stats = CacheStats {
            total_bytes: 4 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(under_stats.space_needed(max_size), 0);
    }

    #[test]
    fn test_download_progress() {
        let mut progress = DownloadProgress::new("track123".to_string(), 10_000_000);

        assert_eq!(progress.progress_percent, 0);
        assert!(!progress.is_complete());

        // Simulate download progress
        std::thread::sleep(std::time::Duration::from_millis(100));
        progress.update(5_000_000);

        assert_eq!(progress.progress_percent, 50);
        assert!(progress.speed_bytes_per_sec > 0);
        assert!(!progress.is_complete());

        // Complete download
        progress.update(10_000_000);
        assert_eq!(progress.progress_percent, 100);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_format_helpers() {
        assert_eq!(format_bytes_per_sec(500), "500 B/s");
        assert_eq!(format_bytes_per_sec(1024), "1.0 KB/s");
        assert_eq!(format_bytes_per_sec(1024 * 1024), "1.0 MB/s");
        assert_eq!(format_bytes_per_sec(1024 * 1024 * 1024), "1.0 GB/s");

        assert_eq!(format_duration_seconds(30), "30s");
        assert_eq!(format_duration_seconds(90), "1m 30s");
        assert_eq!(format_duration_seconds(3661), "1h 1m");
    }

    #[test]
    fn test_download_progress_eta() {
        let mut progress = DownloadProgress::new("track123".to_string(), 10_000_000);

        // Wait a bit for realistic timing
        std::thread::sleep(std::time::Duration::from_millis(100));
        progress.update(1_000_000);

        // Should have ETA calculated
        assert!(progress.eta_seconds.is_some());
        assert!(progress.speed_bytes_per_sec > 0);
    }
}
