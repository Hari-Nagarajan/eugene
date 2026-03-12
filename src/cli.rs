use clap::{Parser, Subcommand};

/// Eugene - Autonomous Recon Agent
#[derive(Parser)]
#[command(name = "eugene", version, about = "Autonomous Recon Agent")]
pub struct Cli {
    /// Override LLM provider for this run (minimax, openrouter)
    #[arg(long, global = true)]
    pub provider: Option<String>,

    /// Override LLM model for this run
    #[arg(long, global = true)]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level subcommands
#[derive(Subcommand)]
pub enum Commands {
    /// Run a one-shot recon task
    Run {
        /// Target network, host, or natural language instruction. If omitted, Eugene
        /// discovers the local network and enumerates everything it can find.
        target: Option<String>,
    },
    /// Start the Telegram bot (includes scheduler)
    Bot,
    /// Manage scheduled tasks
    #[command(subcommand)]
    Schedule(ScheduleCommands),
    /// Run a standalone wifi offensive campaign
    Wifi {
        /// Target BSSID or SSID filter (optional, scans all if omitted)
        target: Option<String>,
        /// Output report to stdout without TUI
        #[arg(long)]
        no_tui: bool,
    },
    /// Interactively set up LLM provider configuration
    Init,
    /// Generate systemd user service file
    Service,
}

/// Schedule sub-subcommands for managing cron tasks
#[derive(Subcommand)]
pub enum ScheduleCommands {
    /// Create a new scheduled task
    Create {
        /// Cron expression (5-field: min hour day month weekday)
        #[arg(short, long)]
        cron: String,
        /// Prompt for the recon task
        prompt: String,
    },
    /// List all scheduled tasks
    List,
    /// Delete a scheduled task
    Delete {
        /// Schedule task ID
        id: String,
    },
    /// Pause a scheduled task
    Pause {
        /// Schedule task ID
        id: String,
    },
    /// Resume a paused task
    Resume {
        /// Schedule task ID
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_run_no_target() {
        let cli = Cli::parse_from(["eugene", "run"]);
        match cli.command {
            Commands::Run { target } => assert_eq!(target, None),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_run_with_target() {
        let cli = Cli::parse_from(["eugene", "run", "192.168.1.0/24"]);
        match cli.command {
            Commands::Run { target } => assert_eq!(target.as_deref(), Some("192.168.1.0/24")),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_bot() {
        let cli = Cli::parse_from(["eugene", "bot"]);
        assert!(matches!(cli.command, Commands::Bot));
    }

    #[test]
    fn test_cli_schedule_create() {
        let cli = Cli::parse_from(["eugene", "schedule", "create", "--cron", "0 */6 * * *", "scan the network"]);
        match cli.command {
            Commands::Schedule(ScheduleCommands::Create { cron, prompt }) => {
                assert_eq!(cron, "0 */6 * * *");
                assert_eq!(prompt, "scan the network");
            }
            _ => panic!("Expected Schedule Create command"),
        }
    }

    #[test]
    fn test_cli_schedule_list() {
        let cli = Cli::parse_from(["eugene", "schedule", "list"]);
        assert!(matches!(cli.command, Commands::Schedule(ScheduleCommands::List)));
    }

    #[test]
    fn test_cli_schedule_delete() {
        let cli = Cli::parse_from(["eugene", "schedule", "delete", "some-uuid"]);
        match cli.command {
            Commands::Schedule(ScheduleCommands::Delete { id }) => {
                assert_eq!(id, "some-uuid");
            }
            _ => panic!("Expected Schedule Delete command"),
        }
    }

    #[test]
    fn test_cli_schedule_pause() {
        let cli = Cli::parse_from(["eugene", "schedule", "pause", "some-uuid"]);
        match cli.command {
            Commands::Schedule(ScheduleCommands::Pause { id }) => {
                assert_eq!(id, "some-uuid");
            }
            _ => panic!("Expected Schedule Pause command"),
        }
    }

    #[test]
    fn test_cli_schedule_resume() {
        let cli = Cli::parse_from(["eugene", "schedule", "resume", "some-uuid"]);
        match cli.command {
            Commands::Schedule(ScheduleCommands::Resume { id }) => {
                assert_eq!(id, "some-uuid");
            }
            _ => panic!("Expected Schedule Resume command"),
        }
    }

    #[test]
    fn test_cli_service() {
        let cli = Cli::parse_from(["eugene", "service"]);
        assert!(matches!(cli.command, Commands::Service));
    }

    #[test]
    fn test_cli_wifi_no_args() {
        let cli = Cli::parse_from(["eugene", "wifi"]);
        match cli.command {
            Commands::Wifi { target, no_tui } => {
                assert_eq!(target, None);
                assert!(!no_tui);
            }
            _ => panic!("Expected Wifi command"),
        }
    }

    #[test]
    fn test_cli_wifi_with_target() {
        let cli = Cli::parse_from(["eugene", "wifi", "AA:BB:CC:DD:EE:FF"]);
        match cli.command {
            Commands::Wifi { target, no_tui } => {
                assert_eq!(target.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
                assert!(!no_tui);
            }
            _ => panic!("Expected Wifi command"),
        }
    }

    #[test]
    fn test_cli_wifi_no_tui_flag() {
        let cli = Cli::parse_from(["eugene", "wifi", "--no-tui"]);
        match cli.command {
            Commands::Wifi { target, no_tui } => {
                assert_eq!(target, None);
                assert!(no_tui);
            }
            _ => panic!("Expected Wifi command"),
        }
    }

    #[test]
    fn test_cli_init() {
        let cli = Cli::parse_from(["eugene", "init"]);
        assert!(matches!(cli.command, Commands::Init));
    }

    #[test]
    fn test_cli_global_provider_flag() {
        let cli = Cli::parse_from(["eugene", "run", "--provider", "openrouter"]);
        assert_eq!(cli.provider, Some("openrouter".to_string()));
    }

    #[test]
    fn test_cli_global_model_flag() {
        let cli = Cli::parse_from(["eugene", "run", "--model", "anthropic/claude-sonnet-4"]);
        assert_eq!(cli.model, Some("anthropic/claude-sonnet-4".to_string()));
    }

    #[test]
    fn test_cli_global_flags_with_wifi() {
        let cli = Cli::parse_from(["eugene", "wifi", "--provider", "minimax", "--no-tui"]);
        assert_eq!(cli.provider, Some("minimax".to_string()));
        match cli.command {
            Commands::Wifi { no_tui, .. } => assert!(no_tui),
            _ => panic!("Expected Wifi command"),
        }
    }

    #[test]
    fn test_cli_no_global_flags_default() {
        let cli = Cli::parse_from(["eugene", "run"]);
        assert_eq!(cli.provider, None);
        assert_eq!(cli.model, None);
    }

    #[test]
    fn test_cli_global_flags_with_bot() {
        let cli = Cli::parse_from(["eugene", "bot", "--provider", "openrouter", "--model", "gpt-4o"]);
        assert_eq!(cli.provider, Some("openrouter".to_string()));
        assert_eq!(cli.model, Some("gpt-4o".to_string()));
    }
}
