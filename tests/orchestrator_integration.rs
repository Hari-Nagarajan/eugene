//! Integration tests for multi-agent orchestration flow.
//!
//! Uses MockCompletionModel with canned responses to validate:
//! - Orchestrator dispatches tasks to executors via dispatch_task tool
//! - Parallel dispatch with multiple tasks works correctly
//! - Memory tools (remember_finding + recall_findings) round-trip through orchestrator
//! - Campaign creates run records with correct status in DB
//! - Executor failure is caught and returned as error string

use eugene::agent::mock::MockCompletionModel;
use eugene::agent::{create_orchestrator_agent, run_campaign, run_recon_task};
use eugene::config::Config;
use eugene::memory::{create_run, init_schema, open_memory_store};

use rig::message::AssistantContent;
use rig::OneOrMany;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Helper: create in-memory DB with schema initialized.
async fn setup_test_env() -> (Arc<Config>, Arc<tokio_rusqlite::Connection>) {
    let memory = open_memory_store(":memory:").await.unwrap();
    init_schema(&memory).await.unwrap();
    let config = Arc::new(Config::default());
    (config, memory)
}

/// Test 1: Orchestrator dispatches a single task to an executor, gets result back.
///
/// Mock response queue (shared by orchestrator + executor):
/// 1. Orchestrator turn 1: tool_call dispatch_task
/// 2. Executor turn 1: text response (executor's final answer)
/// 3. Orchestrator turn 2: text response (orchestrator's final summary)
#[tokio::test]
async fn test_orchestrator_dispatches_executor() {
    let (config, memory) = setup_test_env().await;
    let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    // Shared mock response queue: orchestrator and executor share the same model
    let mock = MockCompletionModel::new(vec![
        // Orchestrator turn 1: dispatch a task
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "dispatch_task",
            serde_json::json!({
                "task_name": "arp_sweep",
                "task_description": "Run arp-scan on 10.0.0.0/24 to discover live hosts"
            }),
        )),
        // Executor turn 1: returns findings (consumed by spawned executor)
        OneOrMany::one(AssistantContent::text(
            "TASK: arp_sweep\nSTATUS: success\nFINDINGS:\n  - Found hosts: 10.0.0.1, 10.0.0.5",
        )),
        // Orchestrator turn 2: final summary after receiving dispatch result
        OneOrMany::one(AssistantContent::text(
            "Campaign complete. ARP sweep found 2 live hosts on 10.0.0.0/24.",
        )),
    ]);

    let orchestrator =
        create_orchestrator_agent(mock, config, memory.clone(), semaphore, run_id);

    let result: String = run_recon_task(&orchestrator, "Scan 10.0.0.0/24")
        .await
        .unwrap();

    assert!(
        result.contains("Campaign complete"),
        "Orchestrator should return campaign summary, got: {result}"
    );

    // Verify task record exists in DB
    let task_count: i64 = memory
        .call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE run_id = ?1",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert!(
        task_count >= 1,
        "At least one task should be logged to DB, got: {task_count}"
    );
}

/// Test 2: Parallel dispatch with 2 tasks returns results for both.
///
/// Mock response queue:
/// 1. Orchestrator: tool_call dispatch_parallel_tasks with 2 tasks
/// 2. Executor 1: text response
/// 3. Executor 2: text response
/// 4. Orchestrator: final text summary
#[tokio::test]
async fn test_parallel_dispatch_two_tasks() {
    let (config, memory) = setup_test_env().await;
    let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    let mock = MockCompletionModel::new(vec![
        // Orchestrator: dispatch 2 tasks in parallel
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "dispatch_parallel_tasks",
            serde_json::json!({
                "tasks": [
                    {"name": "iface_info", "description": "List network interfaces"},
                    {"name": "arp_table", "description": "Check ARP cache"}
                ]
            }),
        )),
        // Executor 1 response
        OneOrMany::one(AssistantContent::text("Interfaces: eth0 (10.0.0.42), wlan0 (down)")),
        // Executor 2 response
        OneOrMany::one(AssistantContent::text("ARP cache: 10.0.0.1 -> aa:bb:cc:dd:ee:ff")),
        // Orchestrator final summary
        OneOrMany::one(AssistantContent::text(
            "Done. Two orientation tasks completed successfully.",
        )),
    ]);

    let orchestrator =
        create_orchestrator_agent(mock, config, memory.clone(), semaphore, run_id);

    let result: String = run_recon_task(&orchestrator, "Run orientation phase")
        .await
        .unwrap();

    assert!(
        result.contains("Done"),
        "Result should contain final orchestrator summary, got: {result}"
    );

    // Verify 2 task records in DB
    let task_count: i64 = memory
        .call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE run_id = ?1",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(
        task_count, 2,
        "Should have 2 task records in DB, got: {task_count}"
    );
}

/// Test 3: Memory tools round-trip: remember_finding -> recall_findings.
///
/// Mock response queue:
/// 1. Orchestrator: tool_call remember_finding
/// 2. Orchestrator: tool_call recall_findings
/// 3. Orchestrator: final text with findings
#[tokio::test]
async fn test_memory_tools_round_trip() {
    let (config, memory) = setup_test_env().await;
    let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    let mock = MockCompletionModel::new(vec![
        // Orchestrator: remember a finding
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "remember_finding",
            serde_json::json!({
                "host": "10.0.0.1",
                "finding_type": "host",
                "data": "Live host discovered via ARP sweep"
            }),
        )),
        // Orchestrator: recall findings for that host
        OneOrMany::one(AssistantContent::tool_call(
            "call_002",
            "recall_findings",
            serde_json::json!({ "host": "10.0.0.1" }),
        )),
        // Orchestrator: final text summarizing findings
        OneOrMany::one(AssistantContent::text(
            "Found 1 finding for 10.0.0.1: live host via ARP.",
        )),
    ]);

    let orchestrator =
        create_orchestrator_agent(mock, config, memory.clone(), semaphore, run_id);

    let result: String = run_recon_task(&orchestrator, "Discover and recall findings for 10.0.0.1")
        .await
        .unwrap();

    assert!(
        result.contains("10.0.0.1"),
        "Result should reference the host, got: {result}"
    );

    // Verify finding persisted in DB
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

/// Test 4: Executor reports failure status, orchestrator handles gracefully.
///
/// Mock response queue:
/// 1. Orchestrator: tool_call dispatch_task
/// 2. Executor: text response reporting failure
/// 3. Orchestrator: final text acknowledging the failure
///
/// This tests the full error reporting path: executor returns an error
/// report, dispatch tool catches it, orchestrator reasons about it.
#[tokio::test]
async fn test_executor_failure_reported_to_orchestrator() {
    let (config, memory) = setup_test_env().await;
    let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    let mock = MockCompletionModel::new(vec![
        // Orchestrator: dispatch a task
        OneOrMany::one(AssistantContent::tool_call(
            "call_001",
            "dispatch_task",
            serde_json::json!({
                "task_name": "failing_scan",
                "task_description": "Scan a non-existent host"
            }),
        )),
        // Executor: reports failure (consumed by spawned executor)
        OneOrMany::one(AssistantContent::text(
            "TASK: failing_scan\nSTATUS: failed\nERRORS:\n  - Host unreachable: 192.168.99.99",
        )),
        // Orchestrator: acknowledges the failure
        OneOrMany::one(AssistantContent::text(
            "Task failing_scan failed. Host 192.168.99.99 was unreachable.",
        )),
    ]);

    let orchestrator =
        create_orchestrator_agent(mock, config, memory.clone(), semaphore, run_id);

    let result: String = run_recon_task(&orchestrator, "Scan a non-existent host")
        .await
        .unwrap();

    assert!(
        result.contains("failed") || result.contains("unreachable"),
        "Result should indicate failure, got: {result}"
    );

    // Verify task exists in DB with completed status (executor returned text, so dispatch
    // marks it completed -- the failure is in the content, not the dispatch mechanism)
    let task_count: i64 = memory
        .call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE run_id = ?1",
                rusqlite::params![run_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(
        task_count, 1,
        "Failed task should still be recorded in DB"
    );
}

/// Test 5: run_campaign() creates a run record with status "completed".
#[tokio::test]
async fn test_campaign_creates_run_record() {
    let (config, memory) = setup_test_env().await;

    // Simple mock: orchestrator returns text immediately (no tool calls)
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

    // Verify run record exists with status "completed"
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

/// Test 6 (gated): Live campaign test with real MiniMax API.
#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_live_campaign() {
    let memory = open_memory_store(":memory:").await.unwrap();
    init_schema(&memory).await.unwrap();
    let config = Arc::new(Config::default());

    let (client, model_name) = eugene::agent::client::create_minimax_client();
    let model = client.completion_model(&model_name);

    let result = run_campaign(model, config, memory.clone(), "10.0.0.0/24")
        .await
        .unwrap();

    assert!(!result.is_empty(), "Live campaign should return non-empty result");
}
