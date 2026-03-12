//! Agent module for Eugene recon agent.
//!
//! Provides single-agent and multi-agent orchestration APIs:
//!
//! **Single-agent mode:**
//! - `create_agent()`: Build a rig agent with all recon tools (backward compat)
//! - `run_recon_task()`: Convenience function to prompt an agent and return the result
//!
//! **Multi-agent orchestration:**
//! - `create_orchestrator_agent()`: Build an orchestrator with dispatch + memory tools
//! - `create_executor_agent()`: Build an executor with recon tools and EXECUTOR_PROMPT
//! - `run_campaign()`: Full campaign lifecycle: create run, build orchestrator, execute, update status
//!
//! The agent uses rig's multi-turn tool-calling loop to chain reconnaissance operations:
//! scan -> analyze -> log findings -> chain additional scans -> summarize.
//!
//! # Example (with real MiniMax client)
//! ```no_run
//! use eugene::agent::{create_agent, run_recon_task};
//! use eugene::agent::tools_available::AvailableTools;
//! use eugene::config::Config;
//! use eugene::memory::{open_memory_store, init_schema};
//! use rig::prelude::CompletionClient;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let (client, model_name) = eugene::agent::client::create_minimax_client()?;
//!     let model = client.completion_model(&model_name);
//!     let config = Arc::new(Config::default());
//!     let memory = open_memory_store("eugene.db").await?;
//!     init_schema(&memory).await?;
//!     let tools = AvailableTools::default();
//!     let agent = create_agent(model, config, memory, &tools);
//!     let result = run_recon_task(&agent, "scan 10.0.0.1").await?;
//!     println!("{result}");
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod prompt;
pub mod tools_available;

pub mod mock;

use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_rusqlite::Connection;

use rig::agent::{Agent, AgentBuilder};
use rig::completion::{CompletionModel, Prompt, PromptError};

use crate::config::Config;
use crate::memory::{create_run, update_run};
use crate::tools::{make_all_tools, make_executor_tools, make_orchestrator_tools};
use tools_available::AvailableTools;

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
    available_tools: &AvailableTools,
) -> Agent<M> {
    let tools = make_all_tools(config, memory);
    let preamble = prompt::system_prompt(available_tools);

    AgentBuilder::new(model)
        .preamble(&preamble)
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

/// Create an orchestrator agent with dispatch + memory tools.
///
/// The orchestrator plans multi-phase recon campaigns and dispatches tasks
/// to executor agents. It gets 5 tools: dispatch_task, dispatch_parallel_tasks,
/// remember_finding, recall_findings, get_run_summary.
///
/// Higher max_turns (20) than executor because the orchestrator plans, dispatches,
/// and reasons across all 5 campaign phases.
pub fn create_orchestrator_agent<M: CompletionModel + Clone + Send + Sync + 'static>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    run_id: i64,
    available_tools: &AvailableTools,
) -> Agent<M>
where
    M::Response: Send,
    M::StreamingResponse: Send,
{
    let model_arc = Arc::new(model);
    let available_tools_arc = Arc::new(available_tools.clone());
    let tools = make_orchestrator_tools(
        model_arc.clone(),
        config,
        memory,
        semaphore,
        run_id,
        available_tools_arc,
    );
    let preamble = prompt::orchestrator_prompt(available_tools);

    AgentBuilder::new((*model_arc).clone())
        .preamble(&preamble)
        .tools(tools)
        .temperature(0.3)
        .max_tokens(4096)
        .default_max_turns(20)
        .build()
}

/// Create an executor agent with recon tools and EXECUTOR_PROMPT.
///
/// Executors are focused, task-specific agents that receive a single task
/// from the orchestrator and use run_command + log_discovery to complete it.
/// Lower max_turns (8) since executors handle focused, scoped tasks.
pub fn create_executor_agent<M: CompletionModel>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
    available_tools: &AvailableTools,
) -> Agent<M> {
    let tools = make_executor_tools(config, memory);
    let preamble = prompt::executor_prompt(available_tools);

    AgentBuilder::new(model)
        .preamble(&preamble)
        .tools(tools)
        .temperature(0.3)
        .max_tokens(4096)
        .default_max_turns(8)
        .build()
}

/// Create a wifi-specific orchestrator agent.
///
/// Identical to `create_orchestrator_agent` but uses `wifi_orchestrator_prompt`
/// with wifi-specific campaign phases and sequential-only dispatch rules.
pub fn create_wifi_orchestrator_agent<M: CompletionModel + Clone + Send + Sync + 'static>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    run_id: i64,
    available_tools: &AvailableTools,
) -> Agent<M>
where
    M::Response: Send,
    M::StreamingResponse: Send,
{
    let model_arc = Arc::new(model);
    let available_tools_arc = Arc::new(available_tools.clone());
    let tools = make_orchestrator_tools(
        model_arc.clone(),
        config,
        memory,
        semaphore,
        run_id,
        available_tools_arc,
    );
    let preamble = prompt::wifi_orchestrator_prompt(available_tools);

    AgentBuilder::new((*model_arc).clone())
        .preamble(&preamble)
        .tools(tools)
        .temperature(0.3)
        .max_tokens(4096)
        .default_max_turns(20)
        .build()
}

/// Run a full multi-phase recon campaign using the orchestrator agent.
///
/// Lifecycle:
/// 1. Check which tools are installed on the system
/// 2. Create a run record in the DB
/// 3. Create a bounded semaphore for executor concurrency
/// 4. Build the orchestrator agent with dispatch + memory tools
/// 5. Prompt the orchestrator with the campaign target
/// 6. Update run status to "completed" (or "failed" on error)
///
/// Returns the orchestrator's final campaign summary.
pub async fn run_campaign<M: CompletionModel + Clone + Send + Sync + 'static>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
    target: Option<&str>,
) -> Result<String, anyhow::Error>
where
    M::Response: Send,
    M::StreamingResponse: Send,
{
    // Check which tools are available on this system
    let available_tools = tools_available::check_available_tools().await;

    // Create run record in DB
    let run_id = create_run(&memory, "campaign".to_string(), target.map(String::from)).await?;

    // Create semaphore for bounded executor concurrency
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    // Build orchestrator agent
    let orchestrator = create_orchestrator_agent(
        model,
        config,
        memory.clone(),
        semaphore,
        run_id,
        &available_tools,
    );

    // Build campaign prompt
    let prompt = match target {
        Some(t) => format!(
            "Run a complete multi-phase recon campaign against target: {}. \
             Execute all 5 phases and report findings.",
            t,
        ),
        None => "Run a complete multi-phase recon campaign. Start by discovering your \
                 network position (interfaces, routes, ARP cache), then find live hosts \
                 on all reachable subnets. Enumerate ports, services, and vulnerabilities \
                 on everything you find. Execute all 5 phases and report findings."
            .to_string(),
    };

    // Execute and handle success/failure
    match run_recon_task(&orchestrator, &prompt).await {
        Ok(result) => {
            let _ = update_run(&memory, run_id, "completed").await;
            Ok(result)
        }
        Err(e) => {
            let _ = update_run(&memory, run_id, "failed").await;
            Err(e)
        }
    }
}

/// Run a standalone wifi offensive campaign.
///
/// Similar to `run_campaign()` but specialized for wifi:
/// - Creates run with `wifi_campaign` trigger type
/// - Uses `Semaphore::new(1)` for sequential ALFA adapter access
/// - Uses `wifi_orchestrator_prompt` with wifi-specific phases
///
/// Returns `(summary_string, run_id)` so callers can generate a WifiReport.
pub async fn run_wifi_campaign<M: CompletionModel + Clone + Send + Sync + 'static>(
    model: M,
    config: Arc<Config>,
    memory: Arc<Connection>,
    target: Option<&str>,
) -> Result<(String, i64), anyhow::Error>
where
    M::Response: Send,
    M::StreamingResponse: Send,
{
    // Check which tools are available on this system
    let available_tools = tools_available::check_available_tools().await;

    // Create run record with wifi_campaign trigger
    let run_id = create_run(&memory, "wifi_campaign".to_string(), target.map(String::from)).await?;

    // Semaphore=1: single ALFA adapter cannot run concurrent wifi operations
    let semaphore = Arc::new(Semaphore::new(1));

    // Build wifi-specific orchestrator agent
    let orchestrator = create_wifi_orchestrator_agent(
        model,
        config,
        memory.clone(),
        semaphore,
        run_id,
        &available_tools,
    );

    // Build wifi campaign prompt
    let prompt = match target {
        Some(t) => format!(
            "Run a wifi offensive campaign targeting: {}. Execute all phases.",
            t,
        ),
        None => "Run a wifi offensive campaign. Scan all visible networks, select \
                 high-value targets, attempt attacks, crack credentials."
            .to_string(),
    };

    // Execute and handle success/failure
    match run_recon_task(&orchestrator, &prompt).await {
        Ok(result) => {
            let _ = update_run(&memory, run_id, "completed").await;
            Ok((result, run_id))
        }
        Err(e) => {
            let _ = update_run(&memory, run_id, "failed").await;
            Err(e)
        }
    }
}
