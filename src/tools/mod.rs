//! Tool system for Eugene recon agent.
//!
//! This module provides two rig Tool trait implementations:
//! - `RunCommandTool`: Executes arbitrary CLI commands on the Pi via tokio::process
//! - `LogDiscoveryTool`: Persists structured findings to SQLite memory store
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

use rig::tool::ToolDyn;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::config::Config;

/// Create all recon tools for agent registration.
/// Returns both run_command and log_discovery tools as dynamic trait objects.
/// Mirrors entropy-goblin's make_all_tools factory pattern.
///
/// Phase 3 will call this when creating agents. The `Vec<Box<dyn ToolDyn>>`
/// allows dynamic tool registration with rig's agent builder via
/// [`ToolSet::from_tools_boxed`](rig::tool::ToolSet::from_tools_boxed).
pub fn make_all_tools(
    config: Arc<Config>,
    memory: Arc<Connection>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RunCommandTool::new(config.clone())) as Box<dyn ToolDyn>,
        Box::new(LogDiscoveryTool::new(memory.clone())) as Box<dyn ToolDyn>,
    ]
}
