use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::memory::{insert_wifi_ap, insert_wifi_client, insert_client_probe};
use crate::tools::ToolError;
use crate::wifi::airodump_parser::parse_airodump_csv;

/// Arguments for the run_airodump tool.
#[derive(Deserialize)]
pub struct RunAirodumpArgs {
    /// Wifi interface in monitor mode (e.g., "wlan1")
    pub interface: String,
    /// Scan duration in seconds (default 30)
    pub duration_secs: Option<u64>,
    /// Run ID for DB persistence (None = no persistence)
    pub run_id: Option<i64>,
}

/// Summary of a discovered AP in the scan result.
#[derive(Debug, Clone, Serialize)]
pub struct ApSummary {
    pub bssid: String,
    pub essid: Option<String>,
    pub channel: Option<i32>,
    pub encryption: Option<String>,
    pub signal_dbm: Option<i32>,
    pub client_count: i32,
}

/// Structured result from running airodump-ng.
#[derive(Serialize)]
pub struct RunAirodumpResult {
    /// Number of access points discovered
    pub ap_count: usize,
    /// Number of client stations discovered
    pub client_count: usize,
    /// Total probed SSIDs found across all clients
    pub probe_count: usize,
    /// Number of malformed CSV rows skipped
    pub skipped_rows: usize,
    /// Actual scan duration in seconds
    pub scan_duration_secs: u64,
    /// Summary of discovered APs
    pub aps: Vec<ApSummary>,
    /// Number of clients that have probed SSIDs
    pub clients_with_probes: usize,
}

/// Tool for running airodump-ng in monitor mode and returning structured scan results.
///
/// Spawns airodump-ng, waits for the specified duration, sends SIGTERM,
/// parses the CSV output, and optionally persists results to the database.
pub struct RunAirodumpTool {
    #[allow(dead_code)]
    config: Arc<Config>,
    memory: Arc<Connection>,
}

impl RunAirodumpTool {
    pub fn new(config: Arc<Config>, memory: Arc<Connection>) -> Self {
        Self { config, memory }
    }
}

impl Tool for RunAirodumpTool {
    const NAME: &'static str = "run_airodump";

    type Error = ToolError;
    type Args = RunAirodumpArgs;
    type Output = RunAirodumpResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "run_airodump".to_string(),
            description: "Run airodump-ng in monitor mode to scan for wifi access points and \
                client stations. Returns structured results with AP details (BSSID, ESSID, \
                channel, encryption, signal) and client associations. Requires the interface \
                to already be in monitor mode."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "interface": {
                        "type": "string",
                        "description": "Wifi interface in monitor mode (e.g., 'wlan1')"
                    },
                    "duration_secs": {
                        "type": "integer",
                        "description": "Scan duration in seconds (default 30)"
                    },
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID for DB persistence (omit for no persistence)"
                    }
                },
                "required": ["interface"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let duration_secs = args.duration_secs.unwrap_or(30);
        let start = Instant::now();

        // Create temp dir for airodump output
        let temp_dir = std::env::temp_dir().join("eugene_airodump");
        if let Err(_e) = std::fs::create_dir_all(&temp_dir) {
            return Ok(RunAirodumpResult {
                ap_count: 0,
                client_count: 0,
                probe_count: 0,
                skipped_rows: 0,
                scan_duration_secs: start.elapsed().as_secs(),
                aps: Vec::new(),
                clients_with_probes: 0,
            });
        }

        // Build write prefix with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let prefix = temp_dir.join(format!("scan_{}", timestamp));
        let prefix_str = prefix.to_string_lossy().to_string();

        // Spawn airodump-ng
        let child = std::process::Command::new("airodump-ng")
            .args([
                "--write",
                &prefix_str,
                "--output-format",
                "csv",
                "--band",
                "abg",
                &args.interface,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(_e) => {
                return Ok(RunAirodumpResult {
                    ap_count: 0,
                    client_count: 0,
                    probe_count: 0,
                    skipped_rows: 0,
                    scan_duration_secs: start.elapsed().as_secs(),
                    aps: Vec::new(),
                    clients_with_probes: 0,
                });
            }
        };

        let pid = child.id();

        // Sleep for scan duration
        tokio::time::sleep(std::time::Duration::from_secs(duration_secs)).await;

        // Send SIGTERM via kill command (avoids unsafe libc)
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();

        // Wait for exit with timeout (5 seconds)
        let wait_start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if wait_start.elapsed().as_secs() >= 5 {
                        // Force kill if SIGTERM didn't work
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }

        let actual_duration = start.elapsed().as_secs();

        // Read CSV file -- airodump appends -01.csv to prefix
        let csv_path = format!("{}-01.csv", prefix_str);
        let csv_text = match std::fs::read_to_string(&csv_path) {
            Ok(text) => text,
            Err(_) => {
                // Try glob pattern for alternate naming
                let pattern = format!("{}-*.csv", prefix_str);
                match glob_first_match(&pattern) {
                    Some(path) => match std::fs::read_to_string(&path) {
                        Ok(text) => text,
                        Err(_e) => {
                            return Ok(RunAirodumpResult {
                                ap_count: 0,
                                client_count: 0,
                                probe_count: 0,
                                skipped_rows: 0,
                                scan_duration_secs: actual_duration,
                                aps: Vec::new(),
                                clients_with_probes: 0,
                            });
                        }
                    },
                    None => {
                        return Ok(RunAirodumpResult {
                            ap_count: 0,
                            client_count: 0,
                            probe_count: 0,
                            skipped_rows: 0,
                            scan_duration_secs: actual_duration,
                            aps: Vec::new(),
                            clients_with_probes: 0,
                        });
                    }
                }
            }
        };

        // Parse CSV
        let parsed = parse_airodump_csv(&csv_text);

        // Count probed SSIDs and clients with probes
        let probe_count: usize = parsed.clients.iter().map(|c| c.probed_essids.len()).sum();
        let clients_with_probes = parsed
            .clients
            .iter()
            .filter(|c| !c.probed_essids.is_empty())
            .count();

        // Build AP summaries
        let ap_summaries: Vec<ApSummary> = parsed
            .aps
            .iter()
            .map(|ap| ApSummary {
                bssid: ap.bssid.clone(),
                essid: ap.essid.clone(),
                channel: ap.channel,
                encryption: ap.privacy.clone(),
                signal_dbm: ap.power,
                client_count: ap.client_count,
            })
            .collect();

        // Persist to DB if run_id provided
        if let Some(run_id) = args.run_id {
            for ap in &parsed.aps {
                let _ = insert_wifi_ap(
                    &self.memory,
                    Some(run_id),
                    ap.bssid.clone(),
                    ap.essid.clone(),
                    ap.channel,
                    None, // frequency not in airodump CSV
                    ap.privacy.clone(),
                    ap.cipher.clone(),
                    ap.auth.clone(),
                    ap.power,
                    Some(ap.client_count),
                )
                .await;
            }

            for client in &parsed.clients {
                let _ = insert_wifi_client(
                    &self.memory,
                    Some(run_id),
                    client.station_mac.clone(),
                    client.bssid.clone(),
                    client.power,
                    client.packets,
                )
                .await;

                for ssid in &client.probed_essids {
                    let _ = insert_client_probe(
                        &self.memory,
                        Some(run_id),
                        client.station_mac.clone(),
                        ssid.clone(),
                    )
                    .await;
                }
            }
        }

        // Cleanup temp files (best-effort)
        let _ = std::fs::remove_dir_all(&temp_dir);

        Ok(RunAirodumpResult {
            ap_count: parsed.aps.len(),
            client_count: parsed.clients.len(),
            probe_count,
            skipped_rows: parsed.skipped_rows,
            scan_duration_secs: actual_duration,
            aps: ap_summaries,
            clients_with_probes,
        })
    }
}

/// Find the first file matching a glob pattern. Returns None if no match.
fn glob_first_match(pattern: &str) -> Option<String> {
    // Simple glob: just check if the exact -01.csv exists, or iterate
    // We use std::fs to list directory and match prefix
    let path = std::path::Path::new(pattern);
    let parent = path.parent()?;
    let stem = path.file_name()?.to_string_lossy();
    // Extract the prefix before the wildcard
    let prefix = stem.split('*').next()?;

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".csv") {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }
    None
}
