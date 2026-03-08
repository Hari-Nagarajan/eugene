//! ALFA adapter runtime discovery and monitor mode management.
//!
//! Discovers the ALFA AWUS036ACH (RTL8812AU) adapter by matching driver names
//! in sysfs, and provides MonitorModeGuard for safe monitor mode transitions
//! with Drop-based cleanup.

use crate::wifi::types::InterfaceState;

/// Known driver names for the RTL8812AU chipset (ALFA AWUS036ACH).
const ALFA_DRIVER_NAMES: &[&str] = &["88XXau", "rtl8812au", "8812au"];

/// Discover the ALFA adapter by checking driver names in sysfs.
///
/// Reads `/sys/class/net/` and checks each interface's driver symlink
/// at `/sys/class/net/<iface>/device/driver`. Returns the first interface
/// whose driver matches a known RTL8812AU driver name.
pub async fn discover_wifi_adapter() -> Option<String> {
    let mut entries = match tokio::fs::read_dir("/sys/class/net").await {
        Ok(entries) => entries,
        Err(_) => {
            log::debug!("Cannot read /sys/class/net (not on Linux?)");
            return None;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let iface_name = entry.file_name().to_string_lossy().to_string();

        // Read the driver symlink: /sys/class/net/<iface>/device/driver
        let driver_path = format!("/sys/class/net/{}/device/driver", iface_name);
        if let Ok(driver_link) = tokio::fs::read_link(&driver_path).await {
            let driver_name = driver_link
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Match RTL8812AU driver names
            if ALFA_DRIVER_NAMES.contains(&driver_name.as_str()) {
                log::info!(
                    "Discovered ALFA adapter: {} (driver: {})",
                    iface_name,
                    driver_name
                );
                return Some(iface_name);
            }
        }
    }

    log::debug!("No ALFA wifi adapter found via sysfs driver detection");
    None
}

/// Resolve the wifi interface to use, with fallback chain:
/// 1. config_override (from EUGENE_WIFI_IFACE env var)
/// 2. sysfs driver discovery
/// 3. None (wifi operations skipped)
pub async fn resolve_wifi_interface(config_override: Option<&str>) -> Option<String> {
    // First: check config override (EUGENE_WIFI_IFACE)
    if let Some(iface) = config_override {
        if !iface.is_empty() {
            log::info!("Using wifi interface from config override: {}", iface);
            return Some(iface.to_string());
        }
    }

    // Second: try sysfs driver discovery
    if let Some(iface) = discover_wifi_adapter().await {
        return Some(iface);
    }

    // Both failed: wifi operations will be skipped
    log::warn!("No ALFA wifi adapter found. Wifi operations will be skipped.");
    None
}

/// Wifi adapter abstraction with interface state tracking.
///
/// Tracks whether the adapter is in managed or monitor mode and prevents
/// conflicting operations (e.g., scanning while in monitor mode).
pub struct WifiAdapter {
    interface: String,
    state: InterfaceState,
}

impl WifiAdapter {
    /// Create a new WifiAdapter in Managed state.
    pub fn new(interface: String) -> Self {
        todo!("Task 2: implement WifiAdapter::new")
    }

    /// Get the interface name.
    pub fn interface(&self) -> &str {
        &self.interface
    }

    /// Get the current interface state.
    pub fn state(&self) -> InterfaceState {
        self.state
    }

    /// Check if the interface still exists in the system.
    pub async fn is_available(&self) -> bool {
        todo!("Task 2: implement is_available")
    }

    /// Enable monitor mode on this adapter. Returns a guard that restores
    /// managed mode when dropped.
    ///
    /// Returns Err if already in monitor mode.
    pub async fn enable_monitor(&mut self) -> Result<MonitorModeGuard, String> {
        todo!("Task 2: implement enable_monitor")
    }

    /// Restore the adapter to managed state (called after guard is dropped).
    pub fn restore_managed(&mut self) {
        self.state = InterfaceState::Managed;
    }
}

/// RAII guard that restores managed mode when dropped.
///
/// Created by `WifiAdapter::enable_monitor()`. The Drop impl uses
/// `std::process::Command` (blocking) to run the 3-step restore sequence,
/// guaranteeing cleanup even during runtime shutdown or panic.
#[derive(Debug)]
pub struct MonitorModeGuard {
    interface: String,
}

impl MonitorModeGuard {
    /// Enable monitor mode via the 3-step ip/iw sequence.
    ///
    /// 1. `ip link set <iface> down`
    /// 2. `iw dev <iface> set type monitor`
    /// 3. `ip link set <iface> up`
    ///
    /// On step 2 failure, attempts to bring the interface back up before returning Err.
    pub async fn enable(interface: &str) -> Result<Self, String> {
        todo!("Task 2: implement MonitorModeGuard::enable")
    }
}

impl Drop for MonitorModeGuard {
    fn drop(&mut self) {
        // Use std::process::Command (blocking) -- Drop is synchronous.
        // The tokio runtime may be shutting down, making async calls unreliable.
        let _ = std::process::Command::new("ip")
            .args(["link", "set", &self.interface, "down"])
            .output();
        let _ = std::process::Command::new("iw")
            .args(["dev", &self.interface, "set", "type", "managed"])
            .output();
        let _ = std::process::Command::new("ip")
            .args(["link", "set", &self.interface, "up"])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_wifi_adapter_returns_none_on_dev_machine() {
        // On a dev machine without an ALFA adapter, discovery should return None.
        // This test validates the function runs without error and returns None
        // when no matching driver is found (which is the case on macOS/dev machines).
        let result = discover_wifi_adapter().await;
        assert!(result.is_none(), "Expected None on dev machine without ALFA adapter");
    }

    #[tokio::test]
    async fn test_resolve_wifi_interface_returns_override_when_provided() {
        let result = resolve_wifi_interface(Some("wlan_test")).await;
        assert_eq!(result, Some("wlan_test".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_wifi_interface_returns_none_with_empty_override() {
        // Empty string override should be treated as no override
        let result = resolve_wifi_interface(Some("")).await;
        // Falls through to sysfs discovery, which returns None on dev machine
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_wifi_interface_returns_none_without_override() {
        // No override, no ALFA adapter on dev machine -> None
        let result = resolve_wifi_interface(None).await;
        assert!(result.is_none());
    }

    // --- Task 2: WifiAdapter tests ---

    #[test]
    fn test_wifi_adapter_new_is_managed() {
        let adapter = WifiAdapter::new("wlan1".to_string());
        assert_eq!(adapter.state(), InterfaceState::Managed);
        assert_eq!(adapter.interface(), "wlan1");
    }

    #[test]
    fn test_wifi_adapter_restore_managed() {
        let mut adapter = WifiAdapter::new("wlan1".to_string());
        // Simulate being in monitor mode
        adapter.state = InterfaceState::Monitor;
        assert_eq!(adapter.state(), InterfaceState::Monitor);

        adapter.restore_managed();
        assert_eq!(adapter.state(), InterfaceState::Managed);
    }

    #[tokio::test]
    async fn test_wifi_adapter_is_available_false_for_nonexistent() {
        let adapter = WifiAdapter::new("nonexistent_iface_xyz".to_string());
        assert!(!adapter.is_available().await);
    }

    #[tokio::test]
    async fn test_wifi_adapter_rejects_double_monitor() {
        let mut adapter = WifiAdapter::new("wlan1".to_string());
        // Manually set state to Monitor to test the guard without running ip/iw
        adapter.state = InterfaceState::Monitor;

        let result = adapter.enable_monitor().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Already in monitor mode");
    }

    #[test]
    fn test_monitor_mode_guard_drop_uses_std_command() {
        // Verify the Drop impl exists and compiles with std::process::Command.
        // We can't actually test the ip/iw calls on a dev machine, but we verify
        // the guard can be created and dropped without panic.
        let guard = MonitorModeGuard {
            interface: "nonexistent_test_iface".to_string(),
        };
        // Drop runs here -- will fail silently since interface doesn't exist
        drop(guard);
    }
}
