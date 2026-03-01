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
