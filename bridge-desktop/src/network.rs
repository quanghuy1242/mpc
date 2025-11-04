//! Network Monitoring Implementation

use async_trait::async_trait;
use bridge_traits::{
    error::Result,
    network::{NetworkChangeStream, NetworkInfo, NetworkMonitor, NetworkStatus, NetworkType},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// Desktop network monitor implementation
///
/// Provides basic network connectivity detection:
/// - Connection status check
/// - Simple connectivity testing
///
/// Note: Platform-specific implementations (Linux netlink, macOS SystemConfiguration,
/// Windows WinAPI) would be more robust but require additional dependencies.
pub struct DesktopNetworkMonitor {
    cached_info: Arc<Mutex<Option<NetworkInfo>>>,
}

impl DesktopNetworkMonitor {
    /// Create a new network monitor
    pub fn new() -> Self {
        Self {
            cached_info: Arc::new(Mutex::new(None)),
        }
    }

    /// Check network connectivity by attempting a simple HTTP request
    async fn check_connectivity(&self) -> NetworkStatus {
        // Try to connect to a reliable endpoint
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::TcpStream::connect("8.8.8.8:53"),
        )
        .await
        {
            Ok(Ok(_)) => NetworkStatus::Connected,
            Ok(Err(_)) => NetworkStatus::Disconnected,
            Err(_) => NetworkStatus::Disconnected,
        }
    }
}

impl Default for DesktopNetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NetworkMonitor for DesktopNetworkMonitor {
    async fn get_network_info(&self) -> Result<NetworkInfo> {
        // Check if we have cached info
        let mut cached = self.cached_info.lock().await;

        // For desktop, we do simple connectivity checks
        let status = self.check_connectivity().await;

        let info = NetworkInfo {
            status,
            network_type: if status == NetworkStatus::Connected {
                // On desktop, we assume Ethernet/WiFi but can't easily distinguish without platform-specific APIs
                Some(NetworkType::Other)
            } else {
                None
            },
            // Desktop connections are typically not metered
            is_metered: false,
            // Desktop connections are typically not expensive
            is_expensive: false,
        };

        *cached = Some(info.clone());
        debug!(status = ?status, "Network info updated");

        Ok(info)
    }

    async fn is_connected(&self) -> bool {
        matches!(
            self.get_network_info().await,
            Ok(NetworkInfo {
                status: NetworkStatus::Connected,
                ..
            })
        )
    }

    async fn is_wifi(&self) -> bool {
        // Desktop implementation doesn't distinguish network types
        // This would require platform-specific APIs
        false
    }

    async fn is_metered(&self) -> bool {
        // Desktop connections are typically not metered
        false
    }

    async fn subscribe_changes(&self) -> Result<Box<dyn NetworkChangeStream>> {
        // Simple implementation: poll periodically
        // A production implementation would use platform-specific APIs to watch for changes
        Ok(Box::new(DesktopNetworkChangeStream {
            monitor: Self::new(),
            last_status: None,
        }))
    }
}

/// Network change stream that polls for changes
struct DesktopNetworkChangeStream {
    monitor: DesktopNetworkMonitor,
    last_status: Option<NetworkStatus>,
}

#[async_trait]
impl NetworkChangeStream for DesktopNetworkChangeStream {
    async fn next(&mut self) -> Option<NetworkInfo> {
        // Poll every 5 seconds for changes
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            if let Ok(info) = self.monitor.get_network_info().await {
                // Only return if status changed
                if self.last_status.as_ref() != Some(&info.status) {
                    self.last_status = Some(info.status);
                    return Some(info);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_monitor_creation() {
        let _monitor = DesktopNetworkMonitor::new();
        assert!(true); // Just verify it constructs
    }

    #[tokio::test]
    async fn test_get_network_info() {
        let monitor = DesktopNetworkMonitor::new();
        let info = monitor.get_network_info().await.unwrap();

        // Should return some status
        assert!(matches!(
            info.status,
            NetworkStatus::Connected | NetworkStatus::Disconnected | NetworkStatus::Indeterminate
        ));
    }

    #[tokio::test]
    async fn test_is_connected() {
        let monitor = DesktopNetworkMonitor::new();
        // Just verify it doesn't panic
        let _ = monitor.is_connected().await;
    }
}
