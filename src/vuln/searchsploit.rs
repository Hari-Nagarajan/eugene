//! SearchSploit CLI integration -- exploit availability checks via Exploit-DB.
//!
//! Wraps `searchsploit --json --cve <id>` to check if public exploits exist
//! for a given CVE. Used both for auto-enrichment during `lookup_cves()` and
//! on-demand via `CheckExploitTool`.
//!
//! When searchsploit is not installed (common outside Kali), all functions
//! return empty results with a warning string -- never `Err`.

use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Top-level JSON output from `searchsploit --json`.
///
/// searchsploit uses uppercase field names with underscores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSploitResult {
    /// Search metadata (query info) -- not used but must be present for parsing.
    #[serde(rename = "SEARCH", default)]
    pub search: Option<serde_json::Value>,

    /// Exploit entries matching the query.
    #[serde(rename = "RESULTS_EXPLOIT", default)]
    pub results_exploit: Vec<ExploitEntry>,
}

/// A single exploit entry from searchsploit results.
///
/// All fields use `#[serde(default)]` for robustness against searchsploit
/// JSON quirks (missing fields, extra fields, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploitEntry {
    /// Exploit title (e.g., "Apache HTTP Server 2.4.49 - Path Traversal")
    #[serde(rename = "Title", default)]
    pub title: String,

    /// Exploit-DB identifier (e.g., "50383")
    #[serde(rename = "EDB-ID", default)]
    pub edb_id: String,

    /// Exploit type: "remote", "local", "webapps", "dos", etc.
    #[serde(rename = "Type", default)]
    pub exploit_type: String,

    /// Target platform (e.g., "linux", "windows", "multiple")
    #[serde(rename = "Platform", default)]
    pub platform: String,

    /// Local file path to the exploit script/code on disk
    #[serde(rename = "Path", default)]
    pub path: String,
}

/// Check if the `searchsploit` CLI tool is available on the system.
///
/// Uses `which searchsploit` to detect availability. Returns `true` if
/// the binary is found in PATH.
pub async fn is_searchsploit_available() -> bool {
    Command::new("which")
        .arg("searchsploit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Search for public exploits matching a CVE identifier.
///
/// `cve_id` should be the bare CVE number (e.g., "2021-44228"), NOT
/// the full "CVE-2021-44228" form. Callers are responsible for stripping
/// the "CVE-" prefix.
///
/// Returns `(exploits, warning)`:
/// - On success: `(vec![...], None)`
/// - When searchsploit not installed: `(vec![], Some("searchsploit not installed"))`
/// - On command/parse failure: `(vec![], Some(error_description))`
pub async fn search_exploits_for_cve(cve_id: &str) -> (Vec<ExploitEntry>, Option<String>) {
    if !is_searchsploit_available().await {
        return (vec![], Some("searchsploit not installed".into()));
    }

    let output = match Command::new("searchsploit")
        .args(["--json", "--cve", cve_id])
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            log::warn!("searchsploit command failed for CVE {}: {}", cve_id, e);
            return (vec![], Some(format!("searchsploit command failed: {}", e)));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "searchsploit exited with status {} for CVE {}: {}",
            output.status,
            cve_id,
            stderr.trim()
        );
        // searchsploit may exit non-zero but still produce valid JSON on stdout
        // (e.g., when no results found). Try parsing stdout anyway.
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return (vec![], None);
    }

    match serde_json::from_str::<SearchSploitResult>(&stdout) {
        Ok(result) => (result.results_exploit, None),
        Err(e) => {
            log::warn!(
                "searchsploit JSON parse failed for CVE {}: {}",
                cve_id,
                e
            );
            (vec![], Some(format!("JSON parse failed: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exploit_entry_deserialization() {
        let json = r#"{
            "Title": "Apache HTTP Server 2.4.49 - Path Traversal",
            "EDB-ID": "50383",
            "Type": "webapps",
            "Platform": "multiple",
            "Path": "/usr/share/exploitdb/exploits/multiple/webapps/50383.py"
        }"#;

        let entry: ExploitEntry = serde_json::from_str(json).expect("deserialize ExploitEntry");
        assert_eq!(entry.title, "Apache HTTP Server 2.4.49 - Path Traversal");
        assert_eq!(entry.edb_id, "50383");
        assert_eq!(entry.exploit_type, "webapps");
        assert_eq!(entry.platform, "multiple");
        assert!(entry.path.ends_with("50383.py"));
    }

    #[test]
    fn test_searchsploit_result_deserialization() {
        let json = r#"{
            "SEARCH": "CVE 2021-44228",
            "RESULTS_EXPLOIT": [
                {
                    "Title": "Log4j RCE",
                    "EDB-ID": "50592",
                    "Type": "remote",
                    "Platform": "java",
                    "Path": "/usr/share/exploitdb/exploits/java/remote/50592.py"
                }
            ]
        }"#;

        let result: SearchSploitResult =
            serde_json::from_str(json).expect("deserialize SearchSploitResult");
        assert_eq!(result.results_exploit.len(), 1);
        assert_eq!(result.results_exploit[0].edb_id, "50592");
    }

    #[test]
    fn test_searchsploit_result_empty_results() {
        let json = r#"{
            "SEARCH": "CVE 9999-99999",
            "RESULTS_EXPLOIT": []
        }"#;

        let result: SearchSploitResult =
            serde_json::from_str(json).expect("deserialize empty SearchSploitResult");
        assert!(result.results_exploit.is_empty());
    }

    #[test]
    fn test_searchsploit_result_missing_results_field() {
        // When RESULTS_EXPLOIT is missing entirely, serde(default) gives empty vec
        let json = r#"{
            "SEARCH": "CVE 9999-99999"
        }"#;

        let result: SearchSploitResult =
            serde_json::from_str(json).expect("deserialize without RESULTS_EXPLOIT");
        assert!(result.results_exploit.is_empty());
    }

    #[test]
    fn test_exploit_entry_missing_fields() {
        // All fields have serde(default), so missing fields should not break deserialization
        let json = r#"{
            "Title": "Partial entry"
        }"#;

        let entry: ExploitEntry =
            serde_json::from_str(json).expect("deserialize partial ExploitEntry");
        assert_eq!(entry.title, "Partial entry");
        assert_eq!(entry.edb_id, "");
        assert_eq!(entry.exploit_type, "");
        assert_eq!(entry.platform, "");
        assert_eq!(entry.path, "");
    }

    #[test]
    fn test_exploit_entry_extra_fields_ignored() {
        let json = r#"{
            "Title": "Test exploit",
            "EDB-ID": "12345",
            "Type": "remote",
            "Platform": "linux",
            "Path": "/path/to/exploit",
            "Date_Published": "2023-01-01",
            "Author": "researcher",
            "Codes": "CVE-2023-12345"
        }"#;

        let entry: ExploitEntry =
            serde_json::from_str(json).expect("deserialize with extra fields");
        assert_eq!(entry.title, "Test exploit");
        assert_eq!(entry.edb_id, "12345");
    }
}
