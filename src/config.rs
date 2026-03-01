use std::collections::HashMap;
use std::path::PathBuf;

/// Default per-tool timeouts in seconds.
fn default_tool_timeouts() -> HashMap<&'static str, u64> {
    HashMap::from([
        ("nmap", 300),
        ("tcpdump", 30),
        ("whois", 15),
        ("netdiscover", 60),
        ("dns", 30),
        ("arp", 10),
        ("traceroute", 90),
        ("default", 60),
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

        Self {
            tool_timeouts: default_tool_timeouts(),
            working_directory: PathBuf::from("/tmp"),
            max_concurrent_executors: 4,
            telegram_bot_token,
            minimax_api_key,
            allowed_chat_ids,
            db_path,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_config_from_env_constructs() {
        // Test that from_env() returns a valid Config with tool timeouts
        // (we don't test env var reading directly to avoid test races)
        let config = Config::from_env();
        assert_eq!(config.max_concurrent_executors, 4);
        assert_eq!(*config.tool_timeouts.get("nmap").unwrap(), 300);
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
