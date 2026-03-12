use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EugeneConfig -- TOML-backed configuration file (~/.eugene/config.toml)
// ---------------------------------------------------------------------------

/// Top-level TOML config structure.
/// All fields are Option so we can distinguish "not in file" from "set in file".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EugeneConfig {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub wifi: WifiConfig,
    #[serde(default)]
    pub vulnerability: VulnConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LlmConfig {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TelegramConfig {
    pub bot_token: Option<String>,
    pub allowed_chat_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DatabaseConfig {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WifiConfig {
    pub interface: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VulnConfig {
    pub nvd_api_key: Option<String>,
}

/// Returns the eugene home directory: ~/.eugene
pub fn eugene_home() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".eugene")
}

/// Returns the config file path: ~/.eugene/config.toml
pub fn config_path() -> PathBuf {
    eugene_home().join("config.toml")
}

impl EugeneConfig {
    /// Load config from ~/.eugene/config.toml.
    /// Returns Default if file is missing or malformed (logs warning on malformed).
    pub fn load_from_file() -> Self {
        Self::load_from_path(&config_path())
    }

    /// Load config from an arbitrary path (useful for testing).
    pub fn load_from_path(path: &PathBuf) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Warning: failed to parse config file {:?}: {}", path, e);
                Self::default()
            }
        }
    }

    /// Save config to ~/.eugene/config.toml, creating ~/.eugene if needed.
    /// Sets chmod 600 on unix.
    pub fn save_to_file(&self) -> Result<(), anyhow::Error> {
        self.save_to_path(&config_path())
    }

    /// Save config to an arbitrary path (useful for testing).
    pub fn save_to_path(&self, path: &PathBuf) -> Result<(), anyhow::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, &content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }
}

/// Default per-tool timeouts in seconds.
fn default_tool_timeouts() -> HashMap<&'static str, u64> {
    HashMap::from([
        ("nmap", 900),
        ("tcpdump", 30),
        ("whois", 15),
        ("netdiscover", 60),
        ("dns", 30),
        ("arp", 10),
        ("traceroute", 90),
        ("default", 60),
        // Wifi tool timeouts
        ("iw", 15),
        ("iwconfig", 10),
        ("iwlist", 30),
        ("airmon-ng", 15),
        ("airodump-ng", 120),
        ("aireplay-ng", 30),
        ("aircrack-ng", 1800),
        ("hcxdumptool", 120),
        ("hcxpcapngtool", 30),
        ("reaver", 600),
        ("bully", 600),
        ("wash", 30),
    ])
}

/// Configuration for tool execution settings and runtime environment
pub struct Config {
    /// Per-tool default timeout in seconds
    pub tool_timeouts: HashMap<&'static str, u64>,
    /// Working directory for command execution
    pub working_directory: PathBuf,
    /// Maximum number of concurrent executor agents (bounded by Semaphore)
    pub max_concurrent_executors: usize,
    /// Telegram bot token (only needed for bot mode)
    pub telegram_bot_token: Option<String>,
    /// MiniMax API key (optional, read from env)
    pub minimax_api_key: Option<String>,
    /// Allowed Telegram chat IDs for access control
    pub allowed_chat_ids: Vec<i64>,
    /// Path to the SQLite database file
    pub db_path: String,
    /// NVD API key for authenticated CVE lookups (optional, higher rate limits)
    pub nvd_api_key: Option<String>,
    /// Discovered ALFA wifi adapter interface name (e.g., "wlan1").
    /// Set by runtime discovery or EUGENE_WIFI_IFACE env var. None if no adapter found.
    pub wifi_interface: Option<String>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Reads:
    /// - `TELEGRAM_BOT_TOKEN` -> telegram_bot_token (optional)
    /// - `MINIMAX_API_KEY` -> minimax_api_key (optional)
    /// - `ALLOWED_CHAT_IDS` -> comma-separated i64 list
    /// - `EUGENE_DB_PATH` -> db_path (default "eugene.db")
    pub fn from_env() -> Self {
        let telegram_bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
        let minimax_api_key = std::env::var("MINIMAX_API_KEY").ok();

        let allowed_chat_ids = std::env::var("ALLOWED_CHAT_IDS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();

        let db_path = std::env::var("EUGENE_DB_PATH").unwrap_or_else(|_| "eugene.db".to_string());
        let nvd_api_key = std::env::var("NVD_API_KEY").ok();
        let wifi_interface = std::env::var("EUGENE_WIFI_IFACE").ok();

        Self {
            tool_timeouts: default_tool_timeouts(),
            working_directory: PathBuf::from("/tmp"),
            max_concurrent_executors: 4,
            telegram_bot_token,
            minimax_api_key,
            allowed_chat_ids,
            db_path,
            nvd_api_key,
            wifi_interface,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tool_timeouts: default_tool_timeouts(),
            working_directory: PathBuf::from("/tmp"),
            max_concurrent_executors: 4,
            telegram_bot_token: None,
            minimax_api_key: None,
            allowed_chat_ids: Vec::new(),
            db_path: "eugene.db".to_string(),
            nvd_api_key: None,
            wifi_interface: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- EugeneConfig tests ----

    #[test]
    fn test_eugene_config_default_all_none() {
        let cfg = EugeneConfig::default();
        assert!(cfg.llm.provider.is_none());
        assert!(cfg.llm.api_key.is_none());
        assert!(cfg.llm.model.is_none());
        assert!(cfg.llm.base_url.is_none());
        assert!(cfg.telegram.bot_token.is_none());
        assert!(cfg.telegram.allowed_chat_ids.is_none());
        assert!(cfg.database.path.is_none());
        assert!(cfg.wifi.interface.is_none());
        assert!(cfg.vulnerability.nvd_api_key.is_none());
    }

    #[test]
    fn test_eugene_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let cfg = EugeneConfig {
            llm: LlmConfig {
                provider: Some("minimax".to_string()),
                api_key: Some("sk-test-123".to_string()),
                model: Some("MiniMax-M2.5".to_string()),
                base_url: Some("https://example.com".to_string()),
            },
            telegram: TelegramConfig {
                bot_token: Some("bot-token".to_string()),
                allowed_chat_ids: Some(vec![123, 456]),
            },
            database: DatabaseConfig {
                path: Some("/tmp/test.db".to_string()),
            },
            wifi: WifiConfig {
                interface: Some("wlan1".to_string()),
            },
            vulnerability: VulnConfig {
                nvd_api_key: Some("nvd-key".to_string()),
            },
        };

        cfg.save_to_path(&path).unwrap();
        let loaded = EugeneConfig::load_from_path(&path);
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn test_eugene_config_missing_file_returns_default() {
        let path = PathBuf::from("/tmp/nonexistent_eugene_test_config.toml");
        let cfg = EugeneConfig::load_from_path(&path);
        assert_eq!(cfg, EugeneConfig::default());
    }

    #[test]
    fn test_eugene_config_malformed_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not [valid toml = = =").unwrap();
        let cfg = EugeneConfig::load_from_path(&path);
        assert_eq!(cfg, EugeneConfig::default());
    }

    #[test]
    fn test_eugene_config_partial_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[llm]\nprovider = \"openrouter\"\n").unwrap();
        let cfg = EugeneConfig::load_from_path(&path);
        assert_eq!(cfg.llm.provider, Some("openrouter".to_string()));
        // Other sections should be default
        assert!(cfg.telegram.bot_token.is_none());
        assert!(cfg.database.path.is_none());
        assert!(cfg.wifi.interface.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn test_eugene_config_save_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = EugeneConfig::default();
        cfg.save_to_path(&path).unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    // ---- Existing Config tests ----

    #[test]
    fn test_config_default_max_concurrent_executors() {
        let config = Config::default();
        assert_eq!(config.max_concurrent_executors, 4);
    }

    #[test]
    fn test_config_default_has_new_fields() {
        let config = Config::default();
        assert!(config.telegram_bot_token.is_none());
        assert!(config.minimax_api_key.is_none());
        assert!(config.allowed_chat_ids.is_empty());
        assert_eq!(config.db_path, "eugene.db");
        assert!(config.nvd_api_key.is_none());
        assert!(config.wifi_interface.is_none());
    }

    #[test]
    fn test_config_from_env_constructs() {
        // Test that from_env() returns a valid Config with tool timeouts
        // (we don't test env var reading directly to avoid test races)
        let config = Config::from_env();
        assert_eq!(config.max_concurrent_executors, 4);
        assert_eq!(*config.tool_timeouts.get("nmap").unwrap(), 900);
    }

    #[test]
    fn test_allowed_chat_ids_parsing() {
        // Test the parsing logic used by from_env for ALLOWED_CHAT_IDS
        let input = "123,456,789";
        let ids: Vec<i64> = input
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        assert_eq!(ids, vec![123, 456, 789]);
    }

    #[test]
    fn test_allowed_chat_ids_parsing_with_invalid() {
        let input = "123,not_a_number,456";
        let ids: Vec<i64> = input
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        assert_eq!(ids, vec![123, 456]);
    }

    #[test]
    fn test_allowed_chat_ids_parsing_empty() {
        let input = "";
        let ids: Vec<i64> = input
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        assert!(ids.is_empty());
    }
}
