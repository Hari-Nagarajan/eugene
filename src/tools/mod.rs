//! Tool system for Eugene recon agent.
//!
//! This module provides rig Tool trait implementations:
//!
//! **Recon tools (single-agent + executor):**
//! - `RunCommandTool`: Executes arbitrary CLI commands on the Pi via tokio::process
//! - `LogDiscoveryTool`: Persists structured findings to SQLite memory store
//!
//! **Orchestrator memory tools:**
//! - `RememberFindingTool`: Persist findings for cross-phase recall
//! - `RecallFindingsTool`: Retrieve findings by host
//! - `GetRunSummaryTool`: Get run statistics (includes scoring data)
//!
//! **Scoring tools (orchestrator only):**
//! - `LogScoreTool`: Log scoring events (host_discovered, detection, etc.) with fixed point values
//! - `GetScoreContextTool`: Get current score summary for EV risk calculations
//!
//! **Script tools (orchestrator + executor):**
//! - `SaveScriptTool`: Persist reusable bash/python scripts to the database
//! - `SearchScriptsTool`: FTS5 full-text search across script names, descriptions, and tags
//! - `RunScriptTool`: Execute saved scripts via interpreter (bash/python3)
//!
//! Unlike entropy-goblin's 8 separate tool structs (nmap, dns, arp, etc.), Eugene uses
//! a single generic command execution tool. The agent constructs the full command string
//! (e.g., "nmap -sS 192.168.1.0/24") and the tool just executes it. This simplifies the
//! codebase while maintaining full recon capability.
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

mod log_score;
pub use log_score::{LogScoreTool, LogScoreArgs, LogScoreResult};

mod get_score_context;
pub use get_score_context::{GetScoreContextTool, GetScoreContextArgs, GetScoreContextResult, ScoreEventSummary};

mod save_script;
pub use save_script::{SaveScriptTool, SaveScriptArgs, SaveScriptResult};

mod search_scripts;
pub use search_scripts::{SearchScriptsTool, SearchScriptsArgs, SearchScriptsResult, ScriptSummary};

mod run_script;
pub use run_script::{RunScriptTool, RunScriptArgs, RunScriptResult};

use rig::completion::CompletionModel;
use rig::tool::ToolDyn;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::orchestrator::dispatch::{DispatchTaskTool, DispatchParallelTasksTool};

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
/// Returns 5 tools: recon tools (run_command, log_discovery) + script tools
/// (save_script, search_scripts, run_script).
///
/// Executors get recon and script tools (no dispatch tools, no memory recall,
/// no scoring tools). This prevents infinite recursion (executor dispatching executor)
/// while allowing script creation and reuse during task execution.
pub fn make_executor_tools(
    config: Arc<Config>,
    memory: Arc<Connection>,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RunCommandTool::new(config.clone())) as Box<dyn ToolDyn>,
        Box::new(LogDiscoveryTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(SaveScriptTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(SearchScriptsTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(RunScriptTool::new(memory.clone(), config)) as Box<dyn ToolDyn>,
    ]
}

/// Create orchestrator memory tools (remember, recall, run_summary, scoring).
///
/// These are the non-generic orchestrator tools that don't require a model type.
/// Includes memory tools (3) + scoring tools (2) = 5 tools.
/// For the full orchestrator tool set including dispatch tools, use `make_orchestrator_tools<M>`.
pub fn make_orchestrator_memory_tools(
    memory: Arc<Connection>,
    run_id: i64,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        Box::new(RememberFindingTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(RecallFindingsTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(GetRunSummaryTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(LogScoreTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(GetScoreContextTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
    ]
}

/// Create the full orchestrator tool set: dispatch + memory + scoring + script tools.
///
/// Returns all 10 orchestrator tools:
/// - `DispatchTaskTool`: Dispatch a single task to an executor agent
/// - `DispatchParallelTasksTool`: Dispatch multiple tasks concurrently
/// - `RememberFindingTool`: Persist findings for cross-phase recall
/// - `RecallFindingsTool`: Retrieve findings by host
/// - `GetRunSummaryTool`: Get run statistics (includes scoring data)
/// - `LogScoreTool`: Log scoring events with fixed point values
/// - `GetScoreContextTool`: Get current score summary for EV calculations
/// - `SaveScriptTool`: Persist reusable scripts to the database
/// - `SearchScriptsTool`: FTS5 search across saved scripts
/// - `RunScriptTool`: Execute saved scripts via interpreter
///
/// Generic over `M: CompletionModel` because dispatch tools need to create
/// executor agents with the same model type.
pub fn make_orchestrator_tools<M: CompletionModel + 'static>(
    model: Arc<M>,
    config: Arc<Config>,
    memory: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    run_id: i64,
) -> Vec<Box<dyn ToolDyn>> {
    vec![
        // Dispatch tools (2)
        Box::new(DispatchTaskTool::new(
            model.clone(),
            config.clone(),
            memory.clone(),
            semaphore.clone(),
            run_id,
        )) as Box<dyn ToolDyn>,
        Box::new(DispatchParallelTasksTool::new(
            model,
            config.clone(),
            memory.clone(),
            semaphore,
            run_id,
        )) as Box<dyn ToolDyn>,
        // Memory tools (3)
        Box::new(RememberFindingTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(RecallFindingsTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(GetRunSummaryTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        // Scoring tools (2)
        Box::new(LogScoreTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        Box::new(GetScoreContextTool::new(memory.clone(), run_id)) as Box<dyn ToolDyn>,
        // Script tools (3)
        Box::new(SaveScriptTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(SearchScriptsTool::new(memory.clone())) as Box<dyn ToolDyn>,
        Box::new(RunScriptTool::new(memory.clone(), config)) as Box<dyn ToolDyn>,
    ]
}
