//! Tool system for Eugene recon agent.
//!
//! This module provides rig Tool trait implementations:
//! - `RunCommandTool`: Executes arbitrary CLI commands on the Pi via tokio::process
//! - `LogDiscoveryTool`: Persists structured findings to SQLite memory store
//! - `RememberFindingTool`: Orchestrator tool to persist findings for cross-phase recall
//! - `RecallFindingsTool`: Orchestrator tool to retrieve findings by host
//! - `GetRunSummaryTool`: Orchestrator tool to get run statistics
//!
//! Unlike entropy-goblin's 8 separate tool structs (nmap, dns, arp, etc.), Eugene uses
//! a single generic command execution tool. The agent constructs the full command string
//! (e.g., "nmap -sS 192.168.1.0/24") and the tool just executes it. This simplifies the
//! codebase while maintaining full recon capability.
//!
//! The agent's system prompt (Phase 3) will teach it how to use nmap, dig, arp-scan, etc.
//! The tool system just provides safe execution and finding persistence.
//!
//! # Example
//! ```no_run
//! use eugene::tools::{make_all_tools, RunCommandTool, LogDiscoveryTool};
//! use eugene::config::Config;
//! use eugene::memory::{open_memory_store, init_schema};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = Arc::new(Config::default());
//!     let memory = open_memory_store("eugene.db").await.unwrap();
//!     init_schema(&memory).await.unwrap();
//!
//!     let tools = make_all_tools(config, memory);
//!     // tools can now be registered with a rig agent
//! }
//! ```

mod errors;
pub use errors::ToolError;

mod run_command;
pub use run_command::{RunCommandTool, RunCommandArgs, RunCommandResult};

mod log_discovery;
pub use log_discovery::{LogDiscoveryTool, LogDiscoveryArgs, LogDiscoveryResult};

mod remember;
pub use remember::{RememberFindingTool, RememberFindingArgs, RememberFindingResult};

mod recall;
pub use recall::{RecallFindingsTool, RecallFindingsArgs, RecallFindingsResult, FindingSummary};

mod run_summary;
pub use run_summary::{GetRunSummaryTool, GetRunSummaryArgs, GetRunSummaryResult};

use rig::tool::ToolDyn;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::config::Config;

/// Create all recon tools for agent registration.
/// Returns both run_command and log_discovery tools as dynamic trait objects.
/// Mirrors entropy-goblin's make_all_tools factory pattern.
///
/// This is the original single-agent tool set. Preserved for backward
/// compatibility with create_agent() in single-agent mode.
pub fn make_all_tools(
    config: Arc<Config>,
    memory: Arc<Connection>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RunCommandTool::new(config.clone())) as Box<dyn ToolDyn>,
        Box::new(LogDiscoveryTool::new(memory.clone())) as Box<dyn ToolDyn>,
    ]
}

/// Create executor tools for dispatched executor agents.
/// Returns run_command and log_discovery tools -- same as make_all_tools.
///
/// Executors get recon tools only (no dispatch tools, no memory recall).
/// This prevents infinite recursion (executor dispatching executor).
pub fn make_executor_tools(
    config: Arc<Config>,
    memory: Arc<Connection>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RunCommandTool::new(config.clone())) as Box<dyn ToolDyn>,
        Box::new(LogDiscoveryTool::new(memory.clone())) as Box<dyn ToolDyn>,
    ]
}

/// Create orchestrator memory tools (remember, recall, run_summary).
///
/// These are the non-generic orchestrator tools that don't require a model type.
/// Plan 02 will compose these with generic dispatch tools in `make_orchestrator_tools<M>`.
pub fn make_orchestrator_memory_tools(
    memory: Arc<Connection>,
    run_id: i64,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RememberFindingTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(RecallFindingsTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(GetRunSummaryTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
    ]
}
