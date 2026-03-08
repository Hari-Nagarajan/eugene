//! Startup tool availability check.
//!
//! At campaign start, checks which Kali/recon tools are actually installed
//! on the system via `which`. Only available tools are injected into agent
//! prompts, preventing wasted turns on "command not found" errors.

use std::fmt;

use tokio::process::Command;

/// Curated Kali tool lists by category.
const RECON_TOOLS: &[&str] = &[
    "nmap",
    "masscan",
    "netdiscover",
    "arp-scan",
    "arping",
    "ping",
    "traceroute",
    "hping3",
];
const DNS_TOOLS: &[&str] = &[
    "dig",
    "nslookup",
    "host",
    "whois",
    "dnsenum",
    "dnsrecon",
    "fierce",
    "dnschef",
];
const WEB_TOOLS: &[&str] = &[
    "nikto",
    "gobuster",
    "dirb",
    "dirsearch",
    "ffuf",
    "feroxbuster",
    "wfuzz",
    "whatweb",
    "wafw00f",
    "sqlmap",
    "wapiti",
];
const SNIFFING_TOOLS: &[&str] = &[
    "tcpdump",
    "tshark",
    "ettercap",
    "responder",
    "bettercap",
    "mitm6",
    "arp",
    "macchanger",
];
const PASSWORD_TOOLS: &[&str] = &[
    "hydra",
    "john",
    "hashcat",
    "medusa",
    "crackmapexec",
    "netexec",
    "cewl",
    "crunch",
];
const EXPLOIT_TOOLS: &[&str] = &["msfconsole", "msfvenom", "searchsploit"];
const UTIL_TOOLS: &[&str] = &[
    "curl", "wget", "netcat", "socat", "python3", "ip", "ss", "ifconfig",
];

/// Wifi-specific tools for adapter management, scanning, and attacks.
pub const WIFI_TOOLS: &[&str] = &[
    "iw",
    "iwconfig",
    "iwlist",
    "airmon-ng",
    "airodump-ng",
    "aireplay-ng",
    "aircrack-ng",
    "hcxdumptool",
    "hcxpcapngtool",
    "reaver",
    "bully",
    "wash",
    "hostapd",
    "dnsmasq",
    "macchanger",
];

/// A categorized snapshot of which tools are installed on this system.
#[derive(Debug, Clone, Default)]
pub struct AvailableTools {
    pub recon: Vec<String>,
    pub dns: Vec<String>,
    pub web: Vec<String>,
    pub sniffing: Vec<String>,
    pub password: Vec<String>,
    pub exploit: Vec<String>,
    pub util: Vec<String>,
    pub wifi: Vec<String>,
}

impl AvailableTools {
    /// Format as a markdown section suitable for injection into agent prompts.
    ///
    /// Only categories with at least one available tool are included.
    pub fn format_section(&self) -> String {
        let mut out = String::from("## Installed Tools\n");
        let categories: &[(&str, &[String])] = &[
            ("Recon", &self.recon),
            ("DNS", &self.dns),
            ("Web", &self.web),
            ("Sniffing & Spoofing", &self.sniffing),
            ("Password Attacks", &self.password),
            ("Exploitation", &self.exploit),
            ("Utilities", &self.util),
            ("Wifi", &self.wifi),
        ];
        let mut any = false;
        for (name, tools) in categories {
            if !tools.is_empty() {
                out.push_str(&format!("\n### {name}\n"));
                out.push_str(&tools.join(", "));
                out.push('\n');
                any = true;
            }
        }
        if !any {
            out.push_str("\nNo tools detected.\n");
        }
        out
    }
}

impl fmt::Display for AvailableTools {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_section())
    }
}

/// Check a single tool name via `which`, returning true if installed.
async fn is_installed(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

/// Filter a tool list to only those that are installed.
async fn filter_available(tools: &[&str]) -> Vec<String> {
    let mut available = Vec::new();
    for &tool in tools {
        if is_installed(tool).await {
            available.push(tool.to_string());
        }
    }
    available
}

/// Discover which tools from the curated list are installed on this system.
///
/// Runs `which` against ~70 tool names. Each check is ~1ms so the total
/// wall time is negligible. Results are cached in the returned struct for
/// the lifetime of the campaign (tool availability doesn't change mid-run).
pub async fn check_available_tools() -> AvailableTools {
    // Run all categories concurrently
    let (recon, dns, web, sniffing, password, exploit, util, wifi) = tokio::join!(
        filter_available(RECON_TOOLS),
        filter_available(DNS_TOOLS),
        filter_available(WEB_TOOLS),
        filter_available(SNIFFING_TOOLS),
        filter_available(PASSWORD_TOOLS),
        filter_available(EXPLOIT_TOOLS),
        filter_available(UTIL_TOOLS),
        filter_available(WIFI_TOOLS),
    );

    AvailableTools {
        recon,
        dns,
        web,
        sniffing,
        password,
        exploit,
        util,
        wifi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_section_empty() {
        let tools = AvailableTools::default();
        let section = tools.format_section();
        assert!(section.contains("No tools detected"));
    }

    #[test]
    fn test_format_section_with_tools() {
        let tools = AvailableTools {
            recon: vec!["nmap".into(), "ping".into()],
            dns: vec!["dig".into()],
            web: vec![],
            sniffing: vec![],
            password: vec![],
            exploit: vec![],
            util: vec!["curl".into()],
            wifi: vec![],
        };
        let section = tools.format_section();
        assert!(section.contains("### Recon\nnmap, ping"));
        assert!(section.contains("### DNS\ndig"));
        assert!(section.contains("### Utilities\ncurl"));
        assert!(!section.contains("### Web"));
        assert!(!section.contains("### Password"));
        assert!(!section.contains("### Wifi"));
        assert!(!section.contains("No tools detected"));
    }

    #[test]
    fn test_format_section_with_wifi_tools() {
        let tools = AvailableTools {
            wifi: vec!["iw".into(), "aircrack-ng".into()],
            ..Default::default()
        };
        let section = tools.format_section();
        assert!(section.contains("### Wifi\niw, aircrack-ng"));
        assert!(!section.contains("No tools detected"));
    }

    #[test]
    fn test_wifi_tools_constant_has_15_entries() {
        assert_eq!(WIFI_TOOLS.len(), 15);
        assert!(WIFI_TOOLS.contains(&"iw"));
        assert!(WIFI_TOOLS.contains(&"airmon-ng"));
        assert!(WIFI_TOOLS.contains(&"macchanger"));
        assert!(WIFI_TOOLS.contains(&"hostapd"));
        assert!(WIFI_TOOLS.contains(&"dnsmasq"));
    }

    #[test]
    fn test_display_matches_format_section() {
        let tools = AvailableTools {
            recon: vec!["nmap".into()],
            ..Default::default()
        };
        assert_eq!(format!("{tools}"), tools.format_section());
    }

    #[tokio::test]
    async fn test_check_available_tools_returns_struct() {
        // Just verify it runs without panicking and returns a valid struct.
        // On CI/dev machines, at least some utils (curl, ping) should be found.
        let tools = check_available_tools().await;
        // Struct should be valid regardless of what's installed
        let _ = tools.format_section();
    }
}
