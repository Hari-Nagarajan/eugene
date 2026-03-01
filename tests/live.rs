//! Live integration tests gated behind `live-tests` Cargo feature.
//! Require a real MiniMax API key and network access.
//!
//! Run with: `cargo test --features live-tests`

#![cfg(feature = "live-tests")]

mod common;

use eugene::agent::client::create_minimax_client;
use eugene::agent::{create_agent, run_campaign};
use rig::completion::Prompt;
use rig::prelude::CompletionClient;

#[tokio::test]
async fn test_live_scan_flow() {
    let (config, memory) = common::setup_env().await;

    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    let agent = create_agent(model, config, memory.clone());
    let result: String = agent.prompt("scan 10.0.0.1 with nmap").await.unwrap();

    assert!(
        !result.is_empty(),
        "Agent should return non-empty response from live API"
    );

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

#[tokio::test]
async fn test_live_agent_responds() {
    let (config, memory) = common::setup_env().await;

    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    let agent = create_agent(model, config, memory);
    let result: String = agent
        .prompt("what tools do you have available?")
        .await
        .unwrap();

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

#[tokio::test]
async fn test_live_campaign() {
    let (config, memory) = common::setup_env().await;

    let (client, model_name) = create_minimax_client();
    let model = client.completion_model(&model_name);

    let result = run_campaign(model, config, memory.clone(), "10.0.0.0/24")
        .await
        .unwrap();

    assert!(
        !result.is_empty(),
        "Live campaign should return non-empty result"
    );
}
