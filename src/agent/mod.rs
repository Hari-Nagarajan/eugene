//! Agent module for Eugene recon agent.
//!
//! Provides the core agent construction and execution API:
//! - `create_agent()`: Build a rig agent with MiniMax M2.5 (or mock) model and all recon tools
//! - `run_recon_task()`: Convenience function to prompt an agent and return the result
//! - `AgentConfig`: Configuration for agent setup (e.g., database path)
//!
//! The agent uses rig's multi-turn tool-calling loop to chain reconnaissance operations:
//! scan -> analyze -> log findings -> chain additional scans -> summarize.
//!
//! # Example (with real MiniMax client)
//! ```no_run
//! use eugene::agent::{AgentConfig, create_agent, run_recon_task};
//! use eugene::config::Config;
//! use eugene::memory::{open_memory_store, init_schema};
//! use rig::prelude::CompletionClient;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let (client, model_name) = eugene::agent::client::create_minimax_client();
//!     let model = client.completion_model(&model_name);
//!     let config = Arc::new(Config::default());
//!     let memory = open_memory_store("eugene.db").await?;
//!     init_schema(&memory).await?;
//!     let agent = create_agent(model, config, memory);
//!     let result = run_recon_task(&agent, "scan 10.0.0.1").await?;
//!     println!("{result}");
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod prompt;

pub mod mock;

use std::sync::Arc;
use tokio_rusqlite::Connection;

use rig::agent::{Agent, AgentBuilder};
use rig::completion::{CompletionModel, Prompt, PromptError};

use crate::config::Config;
use crate::tools::make_all_tools;
use prompt::SYSTEM_PROMPT;

/// Configuration for agent setup.
pub struct AgentConfig {
    /// Path to the SQLite database file. Use ":memory:" for in-memory DB (tests).
    pub db_path: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            db_path: "eugene.db".to_string(),
        }
    }
}

/// Create a rig agent with the given completion model and all recon tools.
///
/// The agent is configured with:
/// - System prompt establishing the Eugene recon agent persona
/// - Both `run_command` and `log_discovery` tools via `make_all_tools()`
/// - Temperature 0.3 for focused, deterministic tool selection
/// - Max 4096 output tokens
/// - 8 multi-turn rounds (critical: default is 0, which causes MaxTurnsError)
///
/// Generic over the model type so it works with both real `CompletionsClient`
/// models and `MockCompletionModel` for testing.
pub fn create_agent<M: CompletionModel>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
) -> Agent<M> {
    let tools = make_all_tools(config, memory);

    AgentBuilder::new(model)
        .preamble(SYSTEM_PROMPT)
        .tools(tools)
        .temperature(0.3)
        .max_tokens(4096)
        .default_max_turns(8)
        .build()
}

/// Convenience function to prompt an agent with a recon task and return the result.
///
/// Maps rig's `PromptError` into `anyhow::Error` for ergonomic top-level usage.
pub async fn run_recon_task(
    agent: &impl Prompt,
    task: &str,
) -> Result<String, anyhow::Error> {
    agent
        .prompt(task)
        .await
        .map_err(|e: PromptError| anyhow::anyhow!("Agent prompt failed: {e}"))
}
