mod errors;
pub use errors::SafetyError;

use ipnet::IpNet;
use regex::Regex;
use std::sync::LazyLock;
use std::net::IpAddr;

/// Shell metacharacter detection regex - blocks ; & | ` $ ( ) > and newlines
static SHELL_METACHAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[;&|`$()>\n]").unwrap()
});

/// Hostname validation regex - alphanumeric, hyphens, dots, underscores
static HOSTNAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._-]+$").unwrap()
});

/// Binaries that could destroy the Pi's filesystem or shut it down
const PI_DESTRUCTIVE_BINARIES: &[&str] = &[
    // Filesystem destruction
    "rm", "dd", "mkfs", "wipefs", "shred",
    "mkfs.ext4", "mkfs.vfat", "mkfs.ntfs",
    // Partition manipulation
    "fdisk", "parted", "gdisk", "cfdisk",
    // System shutdown / reboot
    "shutdown", "reboot", "halt", "poweroff", "init",
    // Secure erase
    "badblocks",
];

/// Wifi binaries that can affect network interfaces
const WIFI_ATTACK_BINARIES: &[&str] = &[
    "airmon-ng", "airodump-ng", "aireplay-ng",
    "hcxdumptool", "hostapd", "wash", "reaver", "bully",
    "iw", "iwconfig",
];

/// Enforce rate-limiting flags on scan tools.
///
/// Rewrites the command in-place to inject conservative timing so the agent
/// cannot overwhelm consumer network equipment, regardless of what the LLM
/// requests. This runs *after* validation so we know the command is safe.
pub fn enforce_scan_limits(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return command.to_string();
    }

    let binary = parts[0].rsplit('/').next().unwrap_or(parts[0]);

    let has_nmap = parts.iter().any(|p| p.rsplit('/').next() == Some("nmap"));
    let has_masscan = binary == "masscan";
    let has_netdiscover = binary == "netdiscover";

    if has_nmap {
        let mut cmd = command.to_string();
        // Strip any existing timing flag (-T0 through -T5)
        let timing_re = regex::Regex::new(r"\s-T[0-5]\b").unwrap();
        cmd = timing_re.replace_all(&cmd, "").to_string();
        // Strip any existing --max-rate
        let rate_re = regex::Regex::new(r"\s--max-rate\s+\d+").unwrap();
        cmd = rate_re.replace_all(&cmd, "").to_string();
        // Inject our limits right after 'nmap'
        cmd = cmd.replacen("nmap", "nmap -T2 --max-rate 50", 1);
        cmd
    } else if has_masscan {
        let mut cmd = command.to_string();
        let rate_re = regex::Regex::new(r"\s--rate\s+\d+").unwrap();
        cmd = rate_re.replace_all(&cmd, "").to_string();
        cmd = cmd.replacen("masscan", "masscan --rate 50", 1);
        cmd
    } else if has_netdiscover {
        let mut cmd = command.to_string();
        if !cmd.contains("-c ") {
            cmd = cmd.replacen("netdiscover", "netdiscover -c 1", 1);
        }
        cmd
    } else {
        command.to_string()
    }
}

/// Check if a wifi command targets a protected (non-ALFA) interface or is a
/// blocked wifi command pattern (e.g., airmon-ng check kill).
pub fn validate_wifi_command(
    command: &str,
    alfa_interface: Option<&str>,
) -> Result<(), SafetyError> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    let binary_idx = if parts[0] == "sudo" { 1 } else { 0 };
    if binary_idx >= parts.len() {
        return Ok(());
    }
    let binary = parts[binary_idx].rsplit('/').next().unwrap_or(parts[binary_idx]);

    // Block airmon-ng check kill unconditionally
    if binary == "airmon-ng"
        && parts.contains(&"check")
        && parts.contains(&"kill")
    {
        return Err(SafetyError::BlockedWifiCommand(
            "airmon-ng check kill is blocked: it kills NetworkManager and severs C2".into(),
        ));
    }

    // For wifi binaries, ensure they only target the ALFA interface
    if WIFI_ATTACK_BINARIES.contains(&binary)
        && let Some(alfa) = alfa_interface
    {
        for part in &parts {
            if part.starts_with("wlan")
                && *part != alfa
                && !part.starts_with(&format!("{}mon", alfa))
            {
                return Err(SafetyError::ProtectedInterface(part.to_string()));
            }
        }
    }

    Ok(())
}

/// Validate command before execution to prevent Pi self-destruction.
///
/// The `alfa_interface` parameter specifies the ALFA wifi adapter interface
/// name (e.g., "wlan1"). When provided, wifi commands targeting any other
/// wlan interface are blocked to protect the C2 channel. Pass `None` for
/// non-wifi scenarios or when no ALFA adapter is configured.
pub fn validate_command(
    command: &str,
    alfa_interface: Option<&str>,
) -> Result<(), SafetyError> {
    // Check shell metacharacters
    if SHELL_METACHAR.is_match(command) {
        return Err(SafetyError::ShellMetacharacters(command.to_string()));
    }

    // Parse command
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(SafetyError::EmptyCommand);
    }

    // Skip 'sudo' prefix if present
    let binary_idx = if parts[0] == "sudo" { 1 } else { 0 };
    if binary_idx >= parts.len() {
        return Err(SafetyError::EmptyCommand);
    }

    // Extract binary name (strip path)
    let binary = parts[binary_idx]
        .rsplit('/')
        .next()
        .unwrap();

    // Check against blocklist
    if PI_DESTRUCTIVE_BINARIES.contains(&binary) {
        return Err(SafetyError::DestructiveBinary(binary.to_string()));
    }

    // Check wifi safety constraints
    validate_wifi_command(command, alfa_interface)?;

    Ok(())
}

/// Sanitize target to validate IP/CIDR/hostname format
pub fn sanitize_target(target: &str) -> Result<String, SafetyError> {
    let target = target.trim();

    if target.is_empty() {
        return Err(SafetyError::InvalidTarget("empty target".to_string()));
    }

    // Check for shell metacharacters
    if SHELL_METACHAR.is_match(target) {
        return Err(SafetyError::InvalidTarget(target.to_string()));
    }

    // Validate as IP, CIDR, or hostname

    // Try as IP address
    if target.parse::<IpAddr>().is_ok() {
        return Ok(target.to_string());
    }

    // Try as CIDR
    if target.contains('/') {
        target.parse::<IpNet>().map_err(|_| {
            SafetyError::InvalidTarget(format!("invalid CIDR notation: {target}"))
        })?;
        return Ok(target.to_string());
    }

    // Validate as hostname: alphanumeric, hyphens, dots, underscores
    if HOSTNAME_PATTERN.is_match(target) {
        return Ok(target.to_string());
    }

    Err(SafetyError::InvalidTarget(format!(
        "target doesn't look like a valid IP/CIDR/hostname: {}",
        target
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Wifi safety tests ---

    #[test]
    fn test_wifi_rejects_non_alfa_interface() {
        // wlan0 is not the ALFA (wlan1), so should be rejected
        let result = validate_wifi_command("iw dev wlan0 scan", Some("wlan1"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SafetyError::ProtectedInterface(iface) => assert_eq!(iface, "wlan0"),
            other => panic!("expected ProtectedInterface, got: {other}"),
        }
    }

    #[test]
    fn test_wifi_allows_alfa_interface() {
        let result = validate_wifi_command("iw dev wlan1 scan", Some("wlan1"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_wifi_blocks_airmon_check_kill() {
        let result = validate_wifi_command("airmon-ng check kill", Some("wlan1"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SafetyError::BlockedWifiCommand(_) => {}
            other => panic!("expected BlockedWifiCommand, got: {other}"),
        }
    }

    #[test]
    fn test_wifi_blocks_sudo_airmon_check_kill() {
        let result = validate_wifi_command("sudo airmon-ng check kill", Some("wlan1"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SafetyError::BlockedWifiCommand(_) => {}
            other => panic!("expected BlockedWifiCommand, got: {other}"),
        }
    }

    #[test]
    fn test_wifi_allows_non_wifi_binaries() {
        // nmap is not a wifi binary, so should pass through
        let result = validate_wifi_command("nmap -sS 192.168.1.1", Some("wlan1"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_wifi_allows_when_no_alfa_configured() {
        // When alfa_interface is None, no protection possible -- allow everything
        let result = validate_wifi_command("iw dev wlan1 scan", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_wifi_blocks_airodump_on_wrong_interface() {
        let result = validate_wifi_command("airodump-ng wlan0", Some("wlan1"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SafetyError::ProtectedInterface(iface) => assert_eq!(iface, "wlan0"),
            other => panic!("expected ProtectedInterface, got: {other}"),
        }
    }

    #[test]
    fn test_validate_command_integrates_wifi_check() {
        // validate_command with alfa_interface should block non-ALFA wifi commands
        let result = validate_command("iw dev wlan0 scan", Some("wlan1"));
        assert!(result.is_err());

        // validate_command with None for alfa allows anything
        let result = validate_command("iw dev wlan0 scan", None);
        assert!(result.is_ok());
    }

    // --- Existing tests (updated for new signature) ---

    #[test]
    fn test_enforce_scan_limits_nmap() {
        // Injects -T2 --max-rate 50
        let cmd = enforce_scan_limits("nmap -sS 192.168.1.0/24");
        assert!(cmd.contains("-T2"), "should inject -T2: {cmd}");
        assert!(cmd.contains("--max-rate 50"), "should inject --max-rate 50: {cmd}");

        // Strips aggressive timing
        let cmd = enforce_scan_limits("nmap -T5 -sS 192.168.1.0/24");
        assert!(!cmd.contains("-T5"), "should strip -T5: {cmd}");
        assert!(cmd.contains("-T2"), "should inject -T2: {cmd}");

        // Strips existing --max-rate
        let cmd = enforce_scan_limits("nmap --max-rate 10000 -sS 192.168.1.0/24");
        assert!(!cmd.contains("10000"), "should strip old rate: {cmd}");
        assert!(cmd.contains("--max-rate 50"), "should inject --max-rate 50: {cmd}");
    }

    #[test]
    fn test_enforce_scan_limits_netdiscover() {
        let cmd = enforce_scan_limits("netdiscover -r 192.168.1.0/24");
        assert!(cmd.contains("-c 1"), "should inject -c 1: {cmd}");

        // Doesn't double-add if already present
        let cmd = enforce_scan_limits("netdiscover -c 3 -r 192.168.1.0/24");
        assert!(!cmd.contains("-c 1"), "should not override existing -c: {cmd}");
    }

    #[test]
    fn test_enforce_scan_limits_passthrough() {
        // Non-scan commands pass through unchanged
        let cmd = enforce_scan_limits("echo hello");
        assert_eq!(cmd, "echo hello");
    }

    #[test]
    fn test_safety_validation() {
        // Test 1: Block shell metacharacters
        assert!(validate_command("nmap -sS 192.168.1.1; rm -rf /", None).is_err());
        assert!(validate_command("cat file.txt | grep data", None).is_err());
        assert!(validate_command("echo $(whoami)", None).is_err());
        assert!(validate_command("ls -la\nrm -rf /", None).is_err());

        // Test 2: Block Pi-destructive binaries
        assert!(validate_command("rm -rf /", None).is_err());
        assert!(validate_command("sudo rm -rf /home", None).is_err());
        assert!(validate_command("dd if=/dev/zero of=/dev/sda", None).is_err());
        assert!(validate_command("mkfs.ext4 /dev/sda1", None).is_err());
        assert!(validate_command("fdisk /dev/sda", None).is_err());
        assert!(validate_command("shutdown -h now", None).is_err());
        assert!(validate_command("reboot", None).is_err());
        assert!(validate_command("/sbin/shutdown now", None).is_err());

        // Test 3: Allow offensive tools
        assert!(validate_command("nmap -sS 192.168.1.1", None).is_ok());
        assert!(validate_command("hydra -l admin -P pass.txt ssh://192.168.1.1", None).is_ok());
        assert!(validate_command("sqlmap -u http://target.com --dbs", None).is_ok());
        assert!(validate_command("msfconsole -r script.rc", None).is_ok());
        assert!(validate_command("nikto -h 192.168.1.1", None).is_ok());

        // Test 4: Allow safe system commands
        assert!(validate_command("ls -la", None).is_ok());
        assert!(validate_command("cat /etc/passwd", None).is_ok());
        assert!(validate_command("sudo nmap -sS 192.168.1.1", None).is_ok());

        // Test 5: Empty command
        assert!(validate_command("", None).is_err());
        assert!(validate_command("   ", None).is_err());
        assert!(validate_command("sudo", None).is_err());

        // Test 6: Validate targets
        assert!(sanitize_target("192.168.1.1").is_ok());
        assert!(sanitize_target("10.0.0.0/8").is_ok());
        assert!(sanitize_target("example.com").is_ok());
        assert!(sanitize_target("sub.example.com").is_ok());
        assert!(sanitize_target("host_name").is_ok());

        // Test 7: Block invalid targets
        assert!(sanitize_target("; rm -rf /").is_err());
        assert!(sanitize_target("192.168.1.1; ls").is_err());
        assert!(sanitize_target("").is_err());
        assert!(sanitize_target("   ").is_err());

        // Test 8: Case sensitivity for binary detection
        assert!(validate_command("nmap -sS target.com", None).is_ok());
        assert!(validate_command("/usr/bin/nmap -sS target.com", None).is_ok());
    }

    // --- Config-related tests ---

    #[test]
    fn test_config_default_wifi_interface_none() {
        let config = crate::config::Config::default();
        assert!(config.wifi_interface.is_none());
    }

    #[test]
    fn test_config_has_wifi_tool_timeouts() {
        let config = crate::config::Config::default();
        assert_eq!(*config.tool_timeouts.get("iw").unwrap(), 15);
        assert_eq!(*config.tool_timeouts.get("airmon-ng").unwrap(), 15);
        assert_eq!(*config.tool_timeouts.get("airodump-ng").unwrap(), 120);
        assert_eq!(*config.tool_timeouts.get("aircrack-ng").unwrap(), 1800);
    }

    // --- Wifi types tests ---

    #[test]
    fn test_interface_state_variants() {
        let managed = crate::wifi::types::InterfaceState::Managed;
        let monitor = crate::wifi::types::InterfaceState::Monitor;
        assert_ne!(managed, monitor);
        assert_eq!(managed, crate::wifi::types::InterfaceState::Managed);
    }
}
