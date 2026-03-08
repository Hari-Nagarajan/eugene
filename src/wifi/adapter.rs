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
    todo!("Task 1: implement sysfs discovery")
}

/// Resolve the wifi interface to use, with fallback chain:
/// 1. config_override (from EUGENE_WIFI_IFACE env var)
/// 2. sysfs driver discovery
/// 3. None (wifi operations skipped)
pub async fn resolve_wifi_interface(config_override: Option<&str>) -> Option<String> {
    todo!("Task 1: implement resolve fallback chain")
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
}
