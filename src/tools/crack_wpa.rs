use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufWriter, Write};
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::tools::ToolError;

/// Default path to rockyou.txt on Kali/Pi.
const ROCKYOU_PATH: &str = "/usr/share/wordlists/rockyou.txt";

/// Arguments for the crack_wpa tool.
#[derive(Deserialize)]
pub struct CrackWpaArgs {
    /// Path to cap file (from capture_handshake) or hash file (from capture_pmkid)
    pub capture_file: String,
    /// Target BSSID (for aircrack-ng -b filter and credential storage)
    pub bssid: String,
    /// Target ESSID (for credential storage)
    pub essid: Option<String>,
    /// Crack method: "handshake" or "pmkid" (for credential storage)
    pub crack_method: String,
    /// Maximum tier to attempt: 1 (fast only), 2 (fast+medium), 3 (all including full rockyou)
    /// Default 2 -- tier 3 should only be used after explicit agent/user decision
    pub max_tier: Option<u32>,
    /// Run ID for DB persistence
    pub run_id: Option<i64>,
}

/// Structured result from WPA cracking attempt.
#[derive(Serialize)]
pub struct CrackWpaResult {
    /// Whether the key was cracked
    pub cracked: bool,
    /// The recovered PSK (if cracked)
    pub psk: Option<String>,
    /// Which tier succeeded (1, 2, or 3)
    pub tier_cracked: Option<u32>,
    /// Tiers attempted
    pub tiers_attempted: Vec<u32>,
    /// Whether tier 3 (full rockyou) is recommended as next step
    pub tier3_recommended: bool,
    /// Error message (error-as-value)
    pub error: Option<String>,
}

impl CrackWpaResult {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            cracked: false,
            psk: None,
            tier_cracked: None,
            tiers_attempted: Vec::new(),
            tier3_recommended: false,
            error: Some(msg.into()),
        }
    }
}

/// Tool for cracking WPA/WPA2 capture files using aircrack-ng with multi-tier wordlists.
///
/// Tier 1: top 1K passwords from rockyou.txt (~2 seconds on Pi)
/// Tier 2: top 100K passwords from rockyou.txt (~3 minutes on Pi)
/// Tier 3: full rockyou.txt (~8 hours on Pi, requires explicit decision)
pub struct CrackWpaTool {
    #[allow(dead_code)]
    config: Arc<Config>,
    #[allow(dead_code)]
    memory: Arc<Connection>,
}

impl CrackWpaTool {
    pub fn new(config: Arc<Config>, memory: Arc<Connection>) -> Self {
        Self { config, memory }
    }
}

/// Generate a wordlist file containing the first `count` lines from rockyou.txt.
///
/// Uses BufReader/BufWriter for efficiency. Returns Ok(()) on success.
fn generate_wordlist(
    rockyou_path: &str,
    count: usize,
    output_path: &std::path::Path,
) -> Result<(), std::io::Error> {
    let input = std::fs::File::open(rockyou_path)?;
    let reader = std::io::BufReader::new(input);

    let output = std::fs::File::create(output_path)?;
    let mut writer = BufWriter::new(output);

    for (i, line) in reader.lines().enumerate() {
        if i >= count {
            break;
        }
        match line {
            Ok(l) => {
                writer.write_all(l.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            Err(_) => continue, // Skip lines with encoding errors (rockyou has some)
        }
    }

    writer.flush()?;
    Ok(())
}

/// Parse aircrack-ng stdout for a cracked key.
///
/// Looks for "KEY FOUND! [ " and extracts the key between "[ " and " ]".
/// IMPORTANT: Do NOT check exit code -- aircrack-ng returns 0 even without finding a key.
fn parse_crack_result(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if line.contains("KEY FOUND!") {
            // Extract text between "[ " and " ]"
            if let Some(start) = line.find("[ ") {
                let after = &line[start + 2..];
                if let Some(end) = after.find(" ]") {
                    let key = after[..end].to_string();
                    if !key.is_empty() {
                        return Some(key);
                    }
                }
            }
        }
    }
    None
}

impl Tool for CrackWpaTool {
    const NAME: &'static str = "crack_wpa";

    type Error = ToolError;
    type Args = CrackWpaArgs;
    type Output = CrackWpaResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "crack_wpa".to_string(),
            description: "Crack a WPA/WPA2 capture file using aircrack-ng with multi-tier \
                wordlist strategy. Tier 1: top 1K passwords (~2s). Tier 2: top 100K passwords \
                (~3 min). Tier 3: full rockyou.txt (~8 hours, requires explicit decision). \
                Stops on first successful crack."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "capture_file": {
                        "type": "string",
                        "description": "Path to .cap file (from capture_handshake) or .22000 hash file (from capture_pmkid)"
                    },
                    "bssid": {
                        "type": "string",
                        "description": "Target BSSID (AP MAC address, e.g., 'AA:BB:CC:DD:EE:FF')"
                    },
                    "essid": {
                        "type": "string",
                        "description": "Target ESSID (for credential storage)"
                    },
                    "crack_method": {
                        "type": "string",
                        "description": "Crack method: 'handshake' or 'pmkid' (for credential storage)"
                    },
                    "max_tier": {
                        "type": "integer",
                        "description": "Maximum tier to attempt: 1 (fast), 2 (fast+medium, default), 3 (all including full rockyou)"
                    },
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID for DB persistence (omit for no persistence)"
                    }
                },
                "required": ["capture_file", "bssid", "crack_method"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let max_tier = args.max_tier.unwrap_or(2).clamp(1, 3);

        // Verify capture file exists
        if !std::path::Path::new(&args.capture_file).exists() {
            return Ok(CrackWpaResult::error(format!(
                "Capture file not found: {}", args.capture_file
            )));
        }

        // Verify rockyou.txt exists
        if !std::path::Path::new(ROCKYOU_PATH).exists() {
            return Ok(CrackWpaResult::error(format!(
                "Wordlist not found: {} -- install wordlists package", ROCKYOU_PATH
            )));
        }

        // Create temp directory for generated wordlists
        let temp_dir = std::env::temp_dir().join("eugene_wordlists");
        if let Err(e) = std::fs::create_dir_all(&temp_dir) {
            return Ok(CrackWpaResult::error(format!(
                "Failed to create temp dir: {}", e
            )));
        }

        let tiers: Vec<(u32, usize)> = vec![
            (1, 1_000),      // Tier 1: top 1K
            (2, 100_000),    // Tier 2: top 100K
            (3, 0),          // Tier 3: full rockyou (0 = use file directly)
        ];

        let mut tiers_attempted = Vec::new();

        for &(tier, count) in &tiers {
            if tier > max_tier {
                break;
            }

            tiers_attempted.push(tier);

            // Generate or select wordlist
            let wordlist_path = if count > 0 {
                let path = temp_dir.join(format!("rockyou_top{}.txt", count));
                if let Err(e) = generate_wordlist(ROCKYOU_PATH, count, &path) {
                    return Ok(CrackWpaResult {
                        cracked: false,
                        psk: None,
                        tier_cracked: None,
                        tiers_attempted,
                        tier3_recommended: false,
                        error: Some(format!("Failed to generate tier {} wordlist: {}", tier, e)),
                    });
                }
                path
            } else {
                std::path::PathBuf::from(ROCKYOU_PATH)
            };

            let wordlist_str = wordlist_path.to_string_lossy().to_string();

            // Build aircrack-ng command
            let cmd = format!(
                "aircrack-ng -w {} -b {} {}",
                wordlist_str, args.bssid, args.capture_file
            );

            // Validate via safety layer
            if let Err(e) = crate::safety::validate_command(&cmd, None) {
                // Clean up temp wordlists
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Ok(CrackWpaResult {
                    cracked: false,
                    psk: None,
                    tier_cracked: None,
                    tiers_attempted,
                    tier3_recommended: false,
                    error: Some(format!("Safety validation failed for aircrack-ng: {}", e)),
                });
            }

            // Run aircrack-ng via tokio::process::Command
            let result = tokio::process::Command::new("aircrack-ng")
                .args(["-w", &wordlist_str, "-b", &args.bssid, &args.capture_file])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .await;

            let output = match result {
                Ok(o) => o,
                Err(e) => {
                    // Clean up temp wordlists
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return Ok(CrackWpaResult {
                        cracked: false,
                        psk: None,
                        tier_cracked: None,
                        tiers_attempted,
                        tier3_recommended: false,
                        error: Some(format!("Failed to run aircrack-ng: {}", e)),
                    });
                }
            };

            let stdout = String::from_utf8_lossy(&output.stdout);

            // IMPORTANT: Parse stdout for KEY FOUND, do NOT check exit code
            if let Some(psk) = parse_crack_result(&stdout) {
                // Store credential
                if let Some(run_id) = args.run_id {
                    let conn = self.memory.clone();
                    let _ = crate::memory::insert_wifi_credential(
                        &conn,
                        Some(run_id),
                        args.bssid.clone(),
                        args.essid.clone(),
                        psk.clone(),
                        args.crack_method.clone(),
                        Some(args.capture_file.clone()),
                    )
                    .await;
                }

                // Clean up temp wordlists
                let _ = std::fs::remove_dir_all(&temp_dir);

                return Ok(CrackWpaResult {
                    cracked: true,
                    psk: Some(psk),
                    tier_cracked: Some(tier),
                    tiers_attempted,
                    tier3_recommended: false,
                    error: None,
                });
            }

            // Not found at this tier, continue to next
        }

        // Clean up temp wordlists
        let _ = std::fs::remove_dir_all(&temp_dir);

        // All attempted tiers exhausted
        let tier3_recommended = max_tier < 3;

        Ok(CrackWpaResult {
            cracked: false,
            psk: None,
            tier_cracked: None,
            tiers_attempted,
            tier3_recommended,
            error: Some(if tier3_recommended {
                format!(
                    "Key not found in tiers 1-{}. Tier 3 (full rockyou, ~8hrs) available with max_tier=3",
                    max_tier
                )
            } else {
                "Key not found in any tier (full rockyou exhausted)".to_string()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_crack_result_found() {
        let stdout = "Opening test.cap\nRead 1234 packets.\n\n                          KEY FOUND! [ mysecretpassword ]\n\n      Master Key     : AA BB CC\n";
        let result = parse_crack_result(stdout);
        assert_eq!(result.as_deref(), Some("mysecretpassword"));
    }

    #[test]
    fn test_parse_crack_result_not_found() {
        let stdout = "Opening test.cap\nRead 1234 packets.\nPassphrase not in dictionary\n";
        let result = parse_crack_result(stdout);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_crack_result_with_spaces_in_key() {
        let stdout = "KEY FOUND! [ my secret password ]\n";
        let result = parse_crack_result(stdout);
        assert_eq!(result.as_deref(), Some("my secret password"));
    }

    #[test]
    fn test_crack_wpa_result_error_helper() {
        let result = CrackWpaResult::error("test error");
        assert!(!result.cracked);
        assert!(result.tiers_attempted.is_empty());
        assert_eq!(result.error.as_deref(), Some("test error"));
    }

    #[test]
    fn test_generate_wordlist_missing_source() {
        let result = generate_wordlist(
            "/nonexistent/rockyou.txt",
            100,
            std::path::Path::new("/tmp/eugene_test_wordlist.txt"),
        );
        assert!(result.is_err());
    }
}
