use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::insert_wifi_ap;
use crate::tools::ToolError;

/// Arguments for the log_wifi_discovery tool
#[derive(Deserialize)]
pub struct LogWifiDiscoveryArgs {
    /// Run ID to associate this AP with
    pub run_id: Option<i64>,
    /// BSSID (MAC address) of the access point -- required
    pub bssid: String,
    /// ESSID (network name) if broadcast is enabled
    pub essid: Option<String>,
    /// Channel number the AP operates on
    pub channel: Option<i32>,
    /// Frequency in MHz (e.g., 2437 for channel 6)
    pub frequency: Option<i32>,
    /// Signal strength in dBm (e.g., -42)
    pub signal_dbm: Option<i32>,
    /// Encryption type (e.g., "WPA2", "WPA3", "WEP", "Open")
    pub encryption: Option<String>,
    /// Cipher suite (e.g., "CCMP", "TKIP")
    pub cipher: Option<String>,
    /// Authentication method (e.g., "PSK", "SAE", "802.1X")
    pub auth: Option<String>,
}

/// Structured result from logging a wifi AP discovery
#[derive(Serialize)]
pub struct LogWifiDiscoveryResult {
    /// Database ID of the persisted AP record
    pub ap_id: i64,
    /// BSSID that was logged
    pub bssid: String,
    /// ISO 8601 timestamp when the AP was logged
    pub logged_at: String,
}

/// Tool for persisting discovered wifi access points to memory.
///
/// This is NOT a CLI command -- it's a direct database operation.
/// The agent calls this after parsing `iw dev scan` output to persist
/// AP findings (BSSID, SSID, channel, encryption, signal strength).
pub struct LogWifiDiscoveryTool {
    memory: Arc<Connection>,
}

impl LogWifiDiscoveryTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

impl Tool for LogWifiDiscoveryTool {
    const NAME: &'static str = "log_wifi_discovery";

    type Error = ToolError;
    type Args = LogWifiDiscoveryArgs;
    type Output = LogWifiDiscoveryResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "log_wifi_discovery".to_string(),
            description: "Log a discovered wifi access point to memory. Use after parsing \
                iw scan output to persist AP findings (BSSID, SSID, channel, encryption, \
                signal strength)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID to associate this AP with (null for standalone discoveries)"
                    },
                    "bssid": {
                        "type": "string",
                        "description": "BSSID (MAC address) of the access point"
                    },
                    "essid": {
                        "type": "string",
                        "description": "ESSID (network name) if broadcast is enabled"
                    },
                    "channel": {
                        "type": "integer",
                        "description": "Channel number the AP operates on"
                    },
                    "frequency": {
                        "type": "integer",
                        "description": "Frequency in MHz (e.g., 2437 for channel 6)"
                    },
                    "signal_dbm": {
                        "type": "integer",
                        "description": "Signal strength in dBm (e.g., -42)"
                    },
                    "encryption": {
                        "type": "string",
                        "description": "Encryption type (WPA2, WPA3, WEP, Open)"
                    },
                    "cipher": {
                        "type": "string",
                        "description": "Cipher suite (CCMP, TKIP)"
                    },
                    "auth": {
                        "type": "string",
                        "description": "Authentication method (PSK, SAE, 802.1X)"
                    }
                },
                "required": ["bssid"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let bssid_clone = args.bssid.clone();

        let ap_id = insert_wifi_ap(
            &self.memory,
            args.run_id,
            args.bssid,
            args.essid,
            args.channel,
            args.frequency,
            args.encryption,
            args.cipher,
            args.auth,
            args.signal_dbm,
        )
        .await?;

        let logged_at = chrono::Utc::now().to_rfc3339();

        Ok(LogWifiDiscoveryResult {
            ap_id,
            bssid: bssid_clone,
            logged_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    async fn setup_tool() -> LogWifiDiscoveryTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        LogWifiDiscoveryTool::new(conn)
    }

    /// Test: call with valid args returns ap_id > 0
    #[tokio::test]
    async fn test_log_wifi_discovery_returns_ap_id() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogWifiDiscoveryArgs {
                run_id: None,
                bssid: "AA:BB:CC:DD:EE:FF".to_string(),
                essid: Some("TestNetwork".to_string()),
                channel: Some(6),
                frequency: Some(2437),
                signal_dbm: Some(-42),
                encryption: Some("WPA2".to_string()),
                cipher: Some("CCMP".to_string()),
                auth: Some("PSK".to_string()),
            })
            .await
            .unwrap();

        assert!(result.ap_id > 0, "ap_id should be positive");
        assert_eq!(result.bssid, "AA:BB:CC:DD:EE:FF");
        assert!(!result.logged_at.is_empty(), "logged_at should be set");
    }

    /// Test: persisted row exists in database with correct bssid and essid
    #[tokio::test]
    async fn test_wifi_discovery_persisted_to_db() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogWifiDiscoveryArgs {
                run_id: None,
                bssid: "11:22:33:44:55:66".to_string(),
                essid: Some("PersistenceTest".to_string()),
                channel: Some(11),
                frequency: None,
                signal_dbm: Some(-55),
                encryption: None,
                cipher: None,
                auth: None,
            })
            .await
            .unwrap();

        let ap_id = result.ap_id;

        // Query the database directly to verify persistence
        let (bssid, essid): (String, Option<String>) = tool
            .memory
            .call(move |conn| {
                let row = conn.query_row(
                    "SELECT bssid, essid FROM wifi_access_points WHERE id = ?1",
                    rusqlite::params![ap_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                Ok(row)
            })
            .await
            .unwrap();

        assert_eq!(bssid, "11:22:33:44:55:66");
        assert_eq!(essid.as_deref(), Some("PersistenceTest"));
    }
}
