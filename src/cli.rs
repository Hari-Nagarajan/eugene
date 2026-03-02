use clap::{Parser, Subcommand};

/// Eugene - Autonomous Recon Agent
#[derive(Parser)]
#[command(name = "eugene", version, about = "Autonomous Recon Agent")]
pub struct Cli {
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
}
