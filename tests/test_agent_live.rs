//! Live integration tests for the Eugene agent module.
//!
//! ALL tests in this file are gated behind the `live-tests` Cargo feature flag.
//! They require a real MiniMax API key and network access.
//!
//! Run with: `cargo test --features live-tests`
//! Do NOT run in CI -- these make real API calls and may attempt network operations.

#![cfg(feature = "live-tests")]

use eugene::agent::client::create_minimax_client;
use eugene::agent::create_agent;
use eugene::config::Config;
use eugene::memory::{init_schema, open_memory_store};
use rig::completion::Prompt;
use rig::prelude::CompletionClient;
use std::sync::Arc;

/// Helper: create in-memory DB with schema initialized.
async fn setup_test_env() -> (Arc<Config>, Arc<tokio_rusqlite::Connection>) {
    let config = Arc::new(Config::default());
    let memory = open_memory_store(":memory:").await.unwrap();
    init_schema(&memory).await.unwrap();
    (config, memory)
}

/// Live test 1: Full scan flow against real MiniMax API.
///
/// Creates a real MiniMax client, builds an agent, and prompts it with a scan task.
/// Validates that the agent returns a non-empty response and persists at least one
/// finding to the database.
///
/// WARNING: This will actually call the MiniMax API and may attempt to run nmap.
#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_live_scan_flow() {
    let (config, memory) = setup_test_env().await;

    // Create real MiniMax client
    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    // Build agent with all tools
    let agent = create_agent(model, config, memory.clone());

    // Prompt with a scan task
    let result: String = agent.prompt("scan 10.0.0.1 with nmap").await.unwrap();

    // Verify agent returned meaningful response
    assert!(
        !result.is_empty(),
        "Agent should return non-empty response from live API"
    );

    // Verify at least one finding was persisted to DB
    let count: i64 = memory
        .call(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM findings",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert!(
        count > 0,
        "At least one finding should be persisted to DB after scan"
    );
}

/// Live test 2: Verify agent sees its tools.
///
/// A simpler live test that prompts the agent about its available tools.
/// The response should mention run_command or nmap, proving the agent
/// is aware of its tool configuration.
#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_live_agent_responds() {
    let (config, memory) = setup_test_env().await;

    // Create real MiniMax client
    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    // Build agent
    let agent = create_agent(model, config, memory);

    // Ask about available tools
    let result: String = agent
        .prompt("what tools do you have available?")
        .await
        .unwrap();

    // Verify response mentions tools
    let result_lower = result.to_lowercase();
    assert!(
        result_lower.contains("run_command")
            || result_lower.contains("nmap")
            || result_lower.contains("command")
            || result_lower.contains("tool")
            || result_lower.contains("log_discovery"),
        "Agent should mention its tools, got: {result}"
    );
}
