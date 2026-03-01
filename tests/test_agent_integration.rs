//! Integration tests for the Eugene agent module.
//!
//! These tests use MockCompletionModel with canned responses to validate
//! the full agent loop: scan -> log_discovery -> text response.
//! No live API key required.

use eugene::agent::create_agent;
use eugene::agent::mock::MockCompletionModel;
use eugene::config::Config;
use eugene::memory::{init_schema, open_memory_store};

use rig::completion::Prompt;
use rig::message::AssistantContent;
use rig::OneOrMany;

use std::sync::Arc;

/// Helper: create in-memory DB with schema initialized.
async fn setup_test_env() -> (Arc<Config>, Arc<tokio_rusqlite::Connection>) {
    let config = Arc::new(Config::default());
    let memory = open_memory_store(":memory:").await.unwrap();
    init_schema(&memory).await.unwrap();
    (config, memory)
}

/// Test 1: Full mock scan flow -- the key integration test.
///
/// Validates ROADMAP success criterion #4:
/// "scan 10.0.0.1" -> agent calls run_command(nmap) -> log_discovery -> text summary.
/// Verifies that the finding is persisted to SQLite.
#[tokio::test]
async fn test_mock_scan_flow() {
    let (config, memory) = setup_test_env().await;

    // Mock: 3 responses simulating scan -> log -> final text
    let mock = MockCompletionModel::new(vec![
        // Turn 1: LLM decides to run nmap
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "run_command",
            serde_json::json!({"command": "nmap -sS 10.0.0.1"}),
        )),
        // Turn 2: LLM logs the finding
        OneOrMany::one(AssistantContent::tool_call(
            "call_002",
            "log_discovery",
            serde_json::json!({
                "finding_type": "port_scan",
                "host": "10.0.0.1",
                "data": "Open ports: 22/tcp (ssh), 80/tcp (http)"
            }),
        )),
        // Turn 3: LLM produces final text summary
        OneOrMany::one(AssistantContent::text(
            "Scan complete. Found 2 open ports on 10.0.0.1: SSH (22) and HTTP (80). Findings logged.",
        )),
    ]);

    let agent = create_agent(mock, config, memory.clone());
    let result: String = agent.prompt("scan 10.0.0.1").await.unwrap();

    // Verify agent returned meaningful text
    assert!(
        result.contains("Scan complete"),
        "Result should contain scan summary, got: {result}"
    );

    // Verify finding was persisted to SQLite
    let count: i64 = memory
        .call(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM findings WHERE host = '10.0.0.1'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert!(
        count > 0,
        "Finding should be persisted to DB for host 10.0.0.1"
    );
}

/// Test 2: Multi-step chain with 4 tool calls doesn't hit MaxTurnsError.
///
/// Validates that default_max_turns(8) allows multi-step chains.
#[tokio::test]
async fn test_mock_multi_step_chain() {
    let (config, memory) = setup_test_env().await;

    // Mock: 4 responses -- nmap -> dig -> log_discovery -> final text
    let mock = MockCompletionModel::new(vec![
        // Turn 1: nmap scan
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "run_command",
            serde_json::json!({"command": "nmap -sS 10.0.0.1"}),
        )),
        // Turn 2: DNS lookup
        OneOrMany::one(AssistantContent::tool_call(
            "call_002",
            "run_command",
            serde_json::json!({"command": "dig 10.0.0.1"}),
        )),
        // Turn 3: log finding
        OneOrMany::one(AssistantContent::tool_call(
            "call_003",
            "log_discovery",
            serde_json::json!({
                "finding_type": "dns_record",
                "host": "10.0.0.1",
                "data": "Reverse DNS: host-10-0-0-1.local"
            }),
        )),
        // Turn 4: final text
        OneOrMany::one(AssistantContent::text(
            "Multi-step chain complete. Performed nmap scan, DNS lookup, and logged findings.",
        )),
    ]);

    let agent = create_agent(mock, config, memory);
    let result: String = agent.prompt("full recon 10.0.0.1").await.unwrap();

    assert!(
        result.contains("Multi-step chain complete"),
        "Agent should complete multi-step chain, got: {result}"
    );
}

/// Test 3: Smoke test -- agent construction and simple prompt work without panic.
#[tokio::test]
async fn test_mock_agent_creation() {
    let (config, memory) = setup_test_env().await;

    // Mock: single text response (no tool calls)
    let mock = MockCompletionModel::new(vec![OneOrMany::one(AssistantContent::text(
        "I understand. Ready to perform reconnaissance.",
    ))]);

    let agent = create_agent(mock, config, memory);
    let result: String = agent.prompt("hello").await.unwrap();

    assert!(
        !result.is_empty(),
        "Agent should return non-empty response"
    );
    assert!(
        result.contains("Ready to perform reconnaissance"),
        "Response should contain canned text, got: {result}"
    );
}
