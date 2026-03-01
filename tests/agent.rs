//! Integration tests for agent + orchestrator behaviors: mock scan flow,
//! campaign lifecycle, memory tools round-trip.

mod common;

use eugene::agent::mock::MockCompletionModel;
use eugene::agent::tools_available::AvailableTools;
use eugene::agent::{create_agent, create_orchestrator_agent, run_campaign, run_recon_task};
use eugene::memory::create_run;

use rig::completion::Prompt;
use rig::message::AssistantContent;
use rig::OneOrMany;
use std::sync::Arc;
use tokio::sync::Semaphore;

// ========== Agent Tests ==========

/// Core agent loop: scan -> log_discovery -> text summary.
#[tokio::test]
async fn test_mock_scan_flow() {
    let (config, memory) = common::setup_env().await;
    let tools = AvailableTools::default();

    let mock = MockCompletionModel::new(vec![
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "run_command",
            serde_json::json!({"command": "nmap -sS 10.0.0.1"}),
        )),
        OneOrMany::one(AssistantContent::tool_call(
            "call_002",
            "log_discovery",
            serde_json::json!({
                "finding_type": "port_scan",
                "host": "10.0.0.1",
                "data": "Open ports: 22/tcp (ssh), 80/tcp (http)"
            }),
        )),
        OneOrMany::one(AssistantContent::text(
            "Scan complete. Found 2 open ports on 10.0.0.1: SSH (22) and HTTP (80). Findings logged.",
        )),
    ]);

    let agent = create_agent(mock, config, memory.clone(), &tools);
    let result: String = agent.prompt("scan 10.0.0.1").await.unwrap();

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

// ========== Orchestrator Tests ==========

/// Campaign creates run record with correct status in DB.
#[tokio::test]
async fn test_campaign_creates_run_record() {
    let (config, memory) = common::setup_env().await;

    let mock = MockCompletionModel::new(vec![OneOrMany::one(AssistantContent::text(
        "Campaign assessment complete. No active hosts found on target network.",
    ))]);

    let result = run_campaign(mock, config, memory.clone(), "192.168.1.0/24")
        .await
        .unwrap();

    assert!(
        result.contains("Campaign assessment complete"),
        "Should return orchestrator's final text, got: {result}"
    );

    let (status, trigger_data): (String, Option<String>) = memory
        .call(|conn| {
            conn.query_row(
                "SELECT status, trigger_data FROM runs ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();

    assert_eq!(
        status, "completed",
        "Run should have status 'completed', got: {status}"
    );
    assert_eq!(
        trigger_data.as_deref(),
        Some("192.168.1.0/24"),
        "Run should store target as trigger_data"
    );
}

/// Memory tools round-trip: remember_finding -> recall_findings.
#[tokio::test]
async fn test_memory_tools_round_trip() {
    let (config, memory) = common::setup_env().await;
    let run_id = create_run(&memory, "test".to_string(), None)
        .await
        .unwrap();
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));
    let tools = AvailableTools::default();

    let mock = MockCompletionModel::new(vec![
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "remember_finding",
            serde_json::json!({
                "host": "10.0.0.1",
                "finding_type": "host",
                "data": "Live host discovered via ARP sweep"
            }),
        )),
        OneOrMany::one(AssistantContent::tool_call(
            "call_002",
            "recall_findings",
            serde_json::json!({ "host": "10.0.0.1" }),
        )),
        OneOrMany::one(AssistantContent::text(
            "Found 1 finding for 10.0.0.1: live host via ARP.",
        )),
    ]);

    let orchestrator =
        create_orchestrator_agent(mock, config, memory.clone(), semaphore, run_id, &tools);

    let result: String =
        run_recon_task(&orchestrator, "Discover and recall findings for 10.0.0.1")
            .await
            .unwrap();

    assert!(
        result.contains("10.0.0.1"),
        "Result should reference the host, got: {result}"
    );

    let finding_count: i64 = memory
        .call(move |conn| {
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
        finding_count >= 1,
        "Finding should be persisted to DB for 10.0.0.1, got: {finding_count}"
    );
}
