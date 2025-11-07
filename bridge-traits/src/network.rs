//! Network Monitoring Abstraction
//!
//! Provides network connectivity and status information.

use crate::{
    error::Result,
    platform::{PlatformSend, PlatformSendSync},
};

/// Network connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkType {
    /// Cellular/mobile data connection
    Cellular,
    /// WiFi connection
    WiFi,
    /// Ethernet connection
    Ethernet,
    /// Other or unknown connection type
    Other,
}

/// Network connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    /// Connected to network
    Connected,
    /// Not connected to any network
    Disconnected,
    /// Connection status unknown or indeterminate
    Indeterminate,
}

/// Network information
#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub status: NetworkStatus,
    pub network_type: Option<NetworkType>,
    /// Whether the connection is metered (has data limits/costs)
    pub is_metered: bool,
    /// Whether the connection is considered expensive by the OS
    pub is_expensive: bool,
}

/// Network monitor trait
///
/// Provides network connectivity information to allow the core to:
/// - Defer sync operations when offline
/// - Use WiFi-only mode for large downloads
/// - Adapt behavior on metered connections
///
/// # Platform Support
///
/// - **Desktop**: System network APIs (NetworkManager, SystemConfiguration, Windows Network List Manager)
/// - **iOS**: Network framework, Reachability
/// - **Android**: ConnectivityManager
/// - **Web**: Navigator.onLine + Network Information API (limited)
///
/// # Example
///
/// ```ignore
/// use bridge_traits::network::NetworkMonitor;
///
/// async fn should_sync(monitor: &dyn NetworkMonitor) -> bool {
///     let info = monitor.get_network_info().await.ok()?;
///     matches!(info.status, NetworkStatus::Connected) && !info.is_metered
/// }
/// ```
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait NetworkMonitor: PlatformSendSync {
    /// Get current network information
    async fn get_network_info(&self) -> Result<NetworkInfo>;

    /// Check if currently connected to any network
    async fn is_connected(&self) -> bool {
        matches!(
            self.get_network_info().await,
            Ok(NetworkInfo {
                status: NetworkStatus::Connected,
                ..
            })
        )
    }

    /// Check if connected via WiFi
    async fn is_wifi(&self) -> bool {
        matches!(
            self.get_network_info().await,
            Ok(NetworkInfo {
                status: NetworkStatus::Connected,
                network_type: Some(NetworkType::WiFi),
                ..
            })
        )
    }

    /// Check if connection is metered
    async fn is_metered(&self) -> bool {
        matches!(
            self.get_network_info().await,
            Ok(NetworkInfo {
                is_metered: true,
                ..
            })
        )
    }

    /// Subscribe to network status changes
    ///
    /// Returns a stream of network info updates. Implementations should
    /// emit an event whenever network status changes.
    async fn subscribe_changes(&self) -> Result<Box<dyn NetworkChangeStream>>;
}

/// Stream of network status changes
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait NetworkChangeStream: PlatformSend {
    /// Get the next network info update
    ///
    /// Returns `None` when the stream is closed.
    async fn next(&mut self) -> Option<NetworkInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_info() {
        let info = NetworkInfo {
            status: NetworkStatus::Connected,
            network_type: Some(NetworkType::WiFi),
            is_metered: false,
            is_expensive: false,
        };

        assert_eq!(info.status, NetworkStatus::Connected);
        assert_eq!(info.network_type, Some(NetworkType::WiFi));
        assert!(!info.is_metered);
    }
}
