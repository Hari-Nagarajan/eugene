use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::{get_matched_probes, get_wifi_aps, get_wifi_clients};
use crate::tools::ToolError;

/// Arguments for the get_wifi_intel tool.
#[derive(Deserialize)]
pub struct GetWifiIntelArgs {
    /// Run ID to analyze
    pub run_id: i64,
}

/// A ranked AP target in the intelligence summary.
#[derive(Debug, Clone, Serialize)]
pub struct ApTarget {
    pub bssid: String,
    pub essid: Option<String>,
    pub channel: Option<i32>,
    pub encryption: Option<String>,
    pub signal_dbm: Option<i32>,
    pub client_count: Option<i32>,
    pub score: f64,
}

/// A high-value client with a matched probe.
#[derive(Debug, Clone, Serialize)]
pub struct HighValueClient {
    pub client_mac: String,
    pub probed_ssid: String,
    pub matched_ap_bssid: String,
    pub ap_channel: Option<i32>,
}

/// Summary statistics for the scan.
#[derive(Debug, Clone, Serialize)]
pub struct IntelSummary {
    pub total_aps: usize,
    pub total_clients: usize,
    pub total_probes: usize,
    pub matched_probes: usize,
    pub scan_run_id: i64,
}

/// Structured wifi intelligence result.
#[derive(Debug, Serialize)]
pub struct GetWifiIntelResult {
    pub top_targets: Vec<ApTarget>,
    pub high_value_clients: Vec<HighValueClient>,
    pub summary: IntelSummary,
}

/// Tool that returns a ranked wifi intelligence summary for a scan run.
///
/// Aggregates AP data, client data, and matched probes into a single
/// actionable intelligence report. APs are ranked by composite attack
/// value score. High-value clients are those whose probed SSIDs match
/// visible APs (deauth target candidates for Phase 12).
pub struct GetWifiIntelTool {
    memory: Arc<Connection>,
}

impl GetWifiIntelTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

/// Calculate encryption weight for AP scoring.
/// WEP=3.0 (easiest to crack), WPA=2.0, WPA2=1.0, OPN=0.5 (already accessible).
fn encryption_weight(encryption: &Option<String>) -> f64 {
    match encryption.as_deref() {
        Some(e) if e.contains("WEP") => 3.0,
        Some(e) if e.contains("WPA2") => 1.0,
        Some(e) if e.contains("WPA") => 2.0,
        Some(e) if e.contains("OPN") || e.to_lowercase().contains("open") => 0.5,
        None => 0.5, // Unknown treated as open/low value
        Some(_) => 1.0,
    }
}

/// Calculate composite attack value score for an AP.
/// Formula: (signal_dbm + 100) * (1 + client_count) * encryption_weight
fn attack_score(signal_dbm: Option<i32>, client_count: Option<i32>, encryption: &Option<String>) -> f64 {
    let signal = signal_dbm.unwrap_or(-100) + 100;
    let clients = 1 + client_count.unwrap_or(0);
    let weight = encryption_weight(encryption);
    (signal as f64) * (clients as f64) * weight
}

impl Tool for GetWifiIntelTool {
    const NAME: &'static str = "get_wifi_intel";

    type Error = ToolError;
    type Args = GetWifiIntelArgs;
    type Output = GetWifiIntelResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "get_wifi_intel".to_string(),
            description: "Get a ranked wifi intelligence summary for a scan run. Returns target \
                APs ranked by attack value, high-value clients with matched probes, and scan \
                statistics."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID of the scan to analyze"
                    }
                },
                "required": ["run_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let run_id = args.run_id;

        // Fetch all data in parallel-ish (sequential but async)
        let aps = get_wifi_aps(&self.memory, run_id).await?;
        let clients = get_wifi_clients(&self.memory, run_id).await?;
        let matched = get_matched_probes(&self.memory, run_id).await?;

        // Total probes = total clients as a count metric (individual probe counts
        // are not stored per-client; matched_probes is the actionable metric)
        let total_probes = clients.len();

        // Rank APs by composite attack score
        let mut targets: Vec<ApTarget> = aps
            .iter()
            .map(|ap| {
                let score = attack_score(ap.signal_dbm, ap.client_count, &ap.encryption);
                ApTarget {
                    bssid: ap.bssid.clone(),
                    essid: ap.essid.clone(),
                    channel: ap.channel,
                    encryption: ap.encryption.clone(),
                    signal_dbm: ap.signal_dbm,
                    client_count: ap.client_count,
                    score,
                }
            })
            .collect();

        // Sort by score descending
        targets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Build high-value clients list
        let high_value_clients: Vec<HighValueClient> = matched
            .iter()
            .map(|m| HighValueClient {
                client_mac: m.client_mac.clone(),
                probed_ssid: m.probed_ssid.clone(),
                matched_ap_bssid: m.matched_ap_bssid.clone(),
                ap_channel: m.channel,
            })
            .collect();

        let summary = IntelSummary {
            total_aps: aps.len(),
            total_clients: clients.len(),
            total_probes,
            matched_probes: matched.len(),
            scan_run_id: run_id,
        };

        Ok(GetWifiIntelResult {
            top_targets: targets,
            high_value_clients,
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{
        create_run, init_schema, insert_client_probe, insert_wifi_ap, insert_wifi_client,
        open_memory_store,
    };

    async fn setup_tool() -> (GetWifiIntelTool, i64) {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn.clone(), "test".to_string(), None)
            .await
            .unwrap();
        let tool = GetWifiIntelTool::new(conn);
        (tool, run_id)
    }

    #[tokio::test]
    async fn test_get_wifi_intel_returns_structured_json() {
        let (tool, run_id) = setup_tool().await;

        // Insert 2 APs with different characteristics
        insert_wifi_ap(
            &tool.memory, Some(run_id), "AA:BB:CC:DD:EE:FF".to_string(),
            Some("WEP_Network".to_string()), Some(6), None,
            Some("WEP".to_string()), None, None, Some(-30), Some(3),
        ).await.unwrap();

        insert_wifi_ap(
            &tool.memory, Some(run_id), "11:22:33:44:55:66".to_string(),
            Some("WPA2_Network".to_string()), Some(11), None,
            Some("WPA2".to_string()), None, None, Some(-50), Some(1),
        ).await.unwrap();

        // Insert a client that probes "WPA2_Network" (matched probe)
        insert_wifi_client(
            &tool.memory, Some(run_id), "CC:DD:EE:FF:00:11".to_string(),
            None, Some(-45), None,
        ).await.unwrap();
        insert_client_probe(
            &tool.memory, Some(run_id), "CC:DD:EE:FF:00:11".to_string(),
            "WPA2_Network".to_string(),
        ).await.unwrap();

        let result = tool.call(GetWifiIntelArgs { run_id }).await.unwrap();

        // Verify top_targets ranked by score (WEP should rank higher due to 3x weight + more clients)
        assert_eq!(result.top_targets.len(), 2);
        assert_eq!(result.top_targets[0].bssid, "AA:BB:CC:DD:EE:FF", "WEP AP should rank first");
        assert!(result.top_targets[0].score > result.top_targets[1].score);

        // Verify high_value_clients
        assert_eq!(result.high_value_clients.len(), 1);
        assert_eq!(result.high_value_clients[0].client_mac, "CC:DD:EE:FF:00:11");
        assert_eq!(result.high_value_clients[0].probed_ssid, "WPA2_Network");
        assert_eq!(result.high_value_clients[0].matched_ap_bssid, "11:22:33:44:55:66");
        assert_eq!(result.high_value_clients[0].ap_channel, Some(11));

        // Verify summary
        assert_eq!(result.summary.total_aps, 2);
        assert_eq!(result.summary.total_clients, 1);
        assert_eq!(result.summary.matched_probes, 1);
        assert_eq!(result.summary.scan_run_id, run_id);

        // Verify JSON serialization works
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("top_targets").is_some());
        assert!(json.get("high_value_clients").is_some());
        assert!(json.get("summary").is_some());
    }

    #[test]
    fn test_encryption_weight_values() {
        assert_eq!(encryption_weight(&Some("WEP".to_string())), 3.0);
        assert_eq!(encryption_weight(&Some("WPA".to_string())), 2.0);
        assert_eq!(encryption_weight(&Some("WPA2".to_string())), 1.0);
        assert_eq!(encryption_weight(&Some("OPN".to_string())), 0.5);
        assert_eq!(encryption_weight(&None), 0.5);
    }

    #[test]
    fn test_attack_score_formula() {
        // signal=-30, clients=3, WEP: (70) * (4) * 3.0 = 840
        let score = attack_score(Some(-30), Some(3), &Some("WEP".to_string()));
        assert!((score - 840.0).abs() < f64::EPSILON);

        // signal=-50, clients=1, WPA2: (50) * (2) * 1.0 = 100
        let score = attack_score(Some(-50), Some(1), &Some("WPA2".to_string()));
        assert!((score - 100.0).abs() < f64::EPSILON);
    }
}
