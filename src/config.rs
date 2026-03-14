use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// LlmLogLevel -- controls LLM request/response logging verbosity
// ---------------------------------------------------------------------------

/// Controls how much LLM interaction detail is logged.
///
/// Cascade resolution order: CLI flag > config.toml > EUGENE_LLM_LOG env var > Off (default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LlmLogLevel {
    /// No LLM logging (default)
    #[default]
    Off,
    /// Log prompt/response summaries (token counts, latency)
    Summary,
    /// Log full prompts and responses
    Full,
}

impl std::fmt::Display for LlmLogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmLogLevel::Off => write!(f, "off"),
            LlmLogLevel::Summary => write!(f, "summary"),
            LlmLogLevel::Full => write!(f, "full"),
        }
    }
}

impl std::str::FromStr for LlmLogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" => Ok(LlmLogLevel::Off),
            "summary" => Ok(LlmLogLevel::Summary),
            "full" => Ok(LlmLogLevel::Full),
            other => Err(format!(
                "invalid LlmLogLevel '{}': expected one of: off, summary, full",
                other
            )),
        }
    }
}

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
    pub llm_log_level: Option<LlmLogLevel>,
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
    /// LLM provider name (e.g., "minimax", "openrouter")
    pub provider: Option<String>,
    /// LLM model name (e.g., "MiniMax-M2.5")
    pub model: Option<String>,
    /// Custom LLM base URL
    pub base_url: Option<String>,
    /// LLM logging verbosity (resolved: CLI > TOML > env > Off)
    pub llm_log_level: LlmLogLevel,
}

impl Config {
    /// Load configuration with layered resolution: TOML > env var > defaults.
    ///
    /// Reads ~/.eugene/config.toml first, then falls back to env vars for any
    /// field not set in TOML, then to hardcoded defaults.
    pub fn load() -> Self {
        Self::load_with_toml(EugeneConfig::load_from_file())
    }

    /// Load configuration from a specific EugeneConfig (useful for testing).
    pub fn load_with_toml(toml: EugeneConfig) -> Self {
        let default_db = eugene_home().join("eugene.db").to_string_lossy().to_string();

        // provider: toml > infer from env > None
        let minimax_api_key_env = std::env::var("MINIMAX_API_KEY").ok();
        let provider = toml.llm.provider.clone().or_else(|| {
            if minimax_api_key_env.is_some() {
                Some("minimax".to_string())
            } else {
                None
            }
        });

        let minimax_api_key = toml.llm.api_key.clone().or(minimax_api_key_env);
        let model = toml.llm.model.clone().or_else(|| std::env::var("MINIMAX_MODEL").ok());
        let base_url = toml.llm.base_url.clone().or_else(|| std::env::var("MINIMAX_BASE_URL").ok());

        let telegram_bot_token = toml.telegram.bot_token.clone()
            .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());

        let allowed_chat_ids = toml.telegram.allowed_chat_ids.clone().unwrap_or_else(|| {
            std::env::var("ALLOWED_CHAT_IDS")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect()
        });

        let db_path = toml.database.path.clone().unwrap_or_else(|| {
            std::env::var("EUGENE_DB_PATH").unwrap_or(default_db)
        });

        let nvd_api_key = toml.vulnerability.nvd_api_key.clone()
            .or_else(|| std::env::var("NVD_API_KEY").ok());

        let wifi_interface = toml.wifi.interface.clone()
            .or_else(|| std::env::var("EUGENE_WIFI_IFACE").ok());

        let llm_log_level = toml.llm.llm_log_level.unwrap_or_else(|| {
            std::env::var("EUGENE_LLM_LOG")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(LlmLogLevel::Off)
        });

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
            provider,
            model,
            base_url,
            llm_log_level,
        }
    }

    /// Backwards-compatible alias for Config::load().
    pub fn from_env() -> Self {
        Self::load()
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
            provider: None,
            model: None,
            base_url: None,
            llm_log_level: LlmLogLevel::Off,
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
                llm_log_level: None,
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
        assert!(config.provider.is_none());
        assert!(config.model.is_none());
        assert!(config.base_url.is_none());
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

    // ---- Config::load() layered resolution tests ----
    // These tests manipulate env vars, so they must be serialized.
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_load_toml_overrides_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_LOCK mutex
        unsafe { std::env::set_var("MINIMAX_API_KEY", "env-key-override-test") };
        let toml = EugeneConfig {
            llm: LlmConfig {
                api_key: Some("toml-key".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = Config::load_with_toml(toml);
        assert_eq!(config.minimax_api_key, Some("toml-key".to_string()));
        unsafe { std::env::remove_var("MINIMAX_API_KEY") };
    }

    #[test]
    fn test_load_env_fallback_when_no_toml() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("MINIMAX_API_KEY", "env-key-fallback-test") };
        let config = Config::load_with_toml(EugeneConfig::default());
        assert_eq!(config.minimax_api_key, Some("env-key-fallback-test".to_string()));
        unsafe { std::env::remove_var("MINIMAX_API_KEY") };
    }

    #[test]
    fn test_load_default_db_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("EUGENE_DB_PATH") };
        let config = Config::load_with_toml(EugeneConfig::default());
        let expected = eugene_home().join("eugene.db").to_string_lossy().to_string();
        assert_eq!(config.db_path, expected);
    }

    #[test]
    fn test_load_infers_minimax_provider() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("MINIMAX_API_KEY", "infer-provider-test") };
        let config = Config::load_with_toml(EugeneConfig::default());
        assert_eq!(config.provider, Some("minimax".to_string()));
        unsafe { std::env::remove_var("MINIMAX_API_KEY") };
    }

    #[test]
    fn test_config_default_unchanged() {
        // Config::default() must remain unchanged for existing test compatibility
        let config = Config::default();
        assert_eq!(config.db_path, "eugene.db");
        assert!(config.telegram_bot_token.is_none());
        assert!(config.minimax_api_key.is_none());
        assert!(config.allowed_chat_ids.is_empty());
        assert!(config.nvd_api_key.is_none());
        assert!(config.wifi_interface.is_none());
        assert!(config.provider.is_none());
        assert!(config.model.is_none());
        assert!(config.base_url.is_none());
        assert_eq!(config.max_concurrent_executors, 4);
    }

    #[test]
    fn test_load_telegram_from_toml() {
        let toml = EugeneConfig {
            telegram: TelegramConfig {
                bot_token: Some("toml-bot-token".to_string()),
                allowed_chat_ids: Some(vec![111, 222]),
            },
            ..Default::default()
        };
        let config = Config::load_with_toml(toml);
        assert_eq!(config.telegram_bot_token, Some("toml-bot-token".to_string()));
        assert_eq!(config.allowed_chat_ids, vec![111, 222]);
    }

    #[test]
    fn test_load_provider_from_toml() {
        let toml = EugeneConfig {
            llm: LlmConfig {
                provider: Some("openrouter".to_string()),
                api_key: Some("or-key".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = Config::load_with_toml(toml);
        assert_eq!(config.provider, Some("openrouter".to_string()));
        assert_eq!(config.minimax_api_key, Some("or-key".to_string()));
    }

    // ---- LlmLogLevel tests ----

    #[test]
    fn test_llm_log_level_from_str() {
        assert_eq!("off".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Off);
        assert_eq!("summary".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Summary);
        assert_eq!("full".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Full);
        // Case insensitive
        assert_eq!("OFF".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Off);
        assert_eq!("Summary".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Summary);
        assert_eq!("FULL".parse::<LlmLogLevel>().unwrap(), LlmLogLevel::Full);
        // Invalid
        assert!("verbose".parse::<LlmLogLevel>().is_err());
        assert!("".parse::<LlmLogLevel>().is_err());
    }

    #[test]
    fn test_llm_log_level_display() {
        assert_eq!(LlmLogLevel::Off.to_string(), "off");
        assert_eq!(LlmLogLevel::Summary.to_string(), "summary");
        assert_eq!(LlmLogLevel::Full.to_string(), "full");
    }

    #[test]
    fn test_llm_log_level_from_toml() {
        let toml_str = r#"
[llm]
llm_log_level = "summary"
"#;
        let cfg: EugeneConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.llm.llm_log_level, Some(LlmLogLevel::Summary));
    }

    #[test]
    fn test_llm_log_level_toml_all_variants() {
        for (input, expected) in [("off", LlmLogLevel::Off), ("summary", LlmLogLevel::Summary), ("full", LlmLogLevel::Full)] {
            let toml_str = format!("[llm]\nllm_log_level = \"{}\"", input);
            let cfg: EugeneConfig = toml::from_str(&toml_str).unwrap();
            assert_eq!(cfg.llm.llm_log_level, Some(expected));
        }
    }

    #[test]
    fn test_llm_log_level_env_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("EUGENE_LLM_LOG", "summary") };
        let config = Config::load_with_toml(EugeneConfig::default());
        assert_eq!(config.llm_log_level, LlmLogLevel::Summary);
        unsafe { std::env::remove_var("EUGENE_LLM_LOG") };
    }

    #[test]
    fn test_llm_log_level_cascade_toml_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("EUGENE_LLM_LOG", "full") };
        let toml = EugeneConfig {
            llm: LlmConfig {
                llm_log_level: Some(LlmLogLevel::Summary),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = Config::load_with_toml(toml);
        assert_eq!(config.llm_log_level, LlmLogLevel::Summary);
        unsafe { std::env::remove_var("EUGENE_LLM_LOG") };
    }

    #[test]
    fn test_llm_log_level_default_off() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("EUGENE_LLM_LOG") };
        let config = Config::load_with_toml(EugeneConfig::default());
        assert_eq!(config.llm_log_level, LlmLogLevel::Off);
    }
}
