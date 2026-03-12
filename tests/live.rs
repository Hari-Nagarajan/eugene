//! Live integration tests gated behind `live-tests` Cargo feature.
//! Require a real MiniMax API key and network access.
//!
//! Run with: `cargo test --features live-tests`

#![cfg(feature = "live-tests")]

use std::sync::LazyLock;

mod common;

static DOTENV: LazyLock<()> = LazyLock::new(|| { let _ = dotenvy::dotenv(); });

use eugene::agent::client::create_client;
use eugene::agent::create_agent;
use eugene::config::Config;
use rig::completion::Prompt;

#[tokio::test]
async fn test_live_agent_responds() {
    LazyLock::force(&DOTENV);
    let (config, memory) = common::setup_env().await;

    let llm_config = Config::load();
    let model = create_client(&llm_config).expect("Failed to create client");

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
