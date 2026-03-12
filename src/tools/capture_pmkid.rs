use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::tools::ToolError;

/// Arguments for the capture_pmkid tool.
#[derive(Deserialize)]
pub struct CapturePmkidArgs {
    /// Wifi interface in monitor mode
    pub interface: String,
    /// Target BSSID (AP MAC address)
    pub bssid: String,
    /// Target channel
    pub channel: i32,
    /// Capture duration in seconds (default 60)
    pub duration_secs: Option<u64>,
    /// Run ID for DB persistence
    pub run_id: Option<i64>,
}

/// Structured result from PMKID capture attempt.
#[derive(Serialize)]
pub struct CapturePmkidResult {
    /// Whether a PMKID was captured
    pub pmkid_captured: bool,
    /// Path to the hash file (if captured)
    pub hash_file: Option<String>,
    /// Path to the raw pcapng capture
    pub pcapng_file: Option<String>,
    /// Error message (if any, error-as-value)
    pub error: Option<String>,
    /// Capture duration in seconds
    pub duration_secs: u64,
}

/// Tool for capturing PMKID from WPA/WPA2 access points using hcxdumptool.
///
/// Does not require connected clients -- the PMKID is extracted from the
/// AP's first EAPOL message during association. Produces a .22000 hash file
/// suitable for cracking with hashcat or aircrack-ng.
pub struct CapturePmkidTool {
    #[allow(dead_code)]
    config: Arc<Config>,
    #[allow(dead_code)]
    memory: Arc<Connection>,
}

impl CapturePmkidTool {
    pub fn new(config: Arc<Config>, memory: Arc<Connection>) -> Self {
        Self { config, memory }
    }
}

impl Tool for CapturePmkidTool {
    const NAME: &'static str = "capture_pmkid";

    type Error = ToolError;
    type Args = CapturePmkidArgs;
    type Output = CapturePmkidResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "capture_pmkid".to_string(),
            description: "Capture PMKID from a WPA/WPA2 access point using hcxdumptool. \
                Does not require connected clients. Returns path to hash file for cracking."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "interface": {
                        "type": "string",
                        "description": "Wifi interface in monitor mode (e.g., 'wlan1')"
                    },
                    "bssid": {
                        "type": "string",
                        "description": "Target BSSID (AP MAC address, e.g., 'AA:BB:CC:DD:EE:FF')"
                    },
                    "channel": {
                        "type": "integer",
                        "description": "Target AP channel number"
                    },
                    "duration_secs": {
                        "type": "integer",
                        "description": "Capture duration in seconds (default 60)"
                    },
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID for DB persistence (omit for no persistence)"
                    }
                },
                "required": ["interface", "bssid", "channel"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let duration_secs = args.duration_secs.unwrap_or(60);
        let start = Instant::now();

        // Create temp directory for PMKID capture output
        let temp_dir = std::env::temp_dir().join("eugene_pmkid");
        if let Err(e) = std::fs::create_dir_all(&temp_dir) {
            return Ok(CapturePmkidResult {
                pmkid_captured: false,
                hash_file: None,
                pcapng_file: None,
                error: Some(format!("Failed to create temp dir: {}", e)),
                duration_secs: start.elapsed().as_secs(),
            });
        }

        // Generate timestamped filenames
        let bssid_sanitized = args.bssid.replace(':', "");
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let pcapng_path = temp_dir.join(format!("pmkid_{}_{}.pcapng", bssid_sanitized, timestamp));
        let hash_path = temp_dir.join(format!("pmkid_{}_{}.22000", bssid_sanitized, timestamp));
        let pcapng_str = pcapng_path.to_string_lossy().to_string();
        let hash_str = hash_path.to_string_lossy().to_string();

        // Build hcxdumptool command
        let cmd = format!(
            "hcxdumptool -i {} -o {} --active_beacon --enable_status=15",
            args.interface, pcapng_str
        );

        // Validate command via safety layer
        if let Err(e) = crate::safety::validate_command(&cmd, Some(&args.interface)) {
            return Ok(CapturePmkidResult {
                pmkid_captured: false,
                hash_file: None,
                pcapng_file: None,
                error: Some(format!("Safety validation failed: {}", e)),
                duration_secs: start.elapsed().as_secs(),
            });
        }

        // Spawn hcxdumptool process
        let child = std::process::Command::new("hcxdumptool")
            .args([
                "-i", &args.interface,
                "-o", &pcapng_str,
                "--active_beacon",
                "--enable_status=15",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return Ok(CapturePmkidResult {
                    pmkid_captured: false,
                    hash_file: None,
                    pcapng_file: None,
                    error: Some(format!("Failed to spawn hcxdumptool: {}", e)),
                    duration_secs: start.elapsed().as_secs(),
                });
            }
        };

        let pid = child.id();

        // Sleep for capture duration
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

        // Convert pcapng to hash file using hcxpcapngtool with BSSID filter
        let convert_cmd = std::process::Command::new("hcxpcapngtool")
            .args([
                "-o", &hash_str,
                &format!("--filtermac={}", args.bssid),
                &pcapng_str,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();

        if let Err(e) = convert_cmd {
            return Ok(CapturePmkidResult {
                pmkid_captured: false,
                hash_file: None,
                pcapng_file: Some(pcapng_str),
                error: Some(format!("Failed to run hcxpcapngtool: {}", e)),
                duration_secs: actual_duration,
            });
        }

        // Check if hash file exists and is non-empty
        let pmkid_captured = std::fs::metadata(&hash_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);

        Ok(CapturePmkidResult {
            pmkid_captured,
            hash_file: if pmkid_captured { Some(hash_str) } else { None },
            pcapng_file: Some(pcapng_str),
            error: if !pmkid_captured {
                Some("No PMKID found in capture (AP may not support PMKID or no response received)".to_string())
            } else {
                None
            },
            duration_secs: actual_duration,
        })
    }
}
