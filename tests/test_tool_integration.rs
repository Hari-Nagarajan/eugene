use std::sync::Arc;

use eugene::{
    config::Config,
    memory::{init_schema, open_memory_store},
    tools::{make_all_tools, LogDiscoveryArgs, LogDiscoveryTool, RunCommandArgs, RunCommandTool},
};
use rig::tool::Tool;

/// Create a fresh test environment with in-memory SQLite and default config.
async fn setup_test_env() -> (Arc<Config>, Arc<tokio_rusqlite::Connection>) {
    let conn = open_memory_store(":memory:").await.unwrap();
    init_schema(&conn).await.unwrap();
    let config = Arc::new(Config::default());
    (config, conn)
}

/// Test 1: make_all_tools returns exactly 2 tools as ToolDyn trait objects.
#[tokio::test]
async fn test_make_all_tools_count() {
    let (config, memory) = setup_test_env().await;
    let tools = make_all_tools(config, memory);
    assert_eq!(tools.len(), 2, "make_all_tools should return 2 tools");

    // Verify both tools have names via ToolDyn interface (dynamic dispatch)
    let names: Vec<String> = tools.iter().map(|t| t.name()).collect();
    assert!(
        names.contains(&"run_command".to_string()),
        "should contain run_command tool"
    );
    assert!(
        names.contains(&"log_discovery".to_string()),
        "should contain log_discovery tool"
    );
}

/// Test 2: Full workflow - run echo command, log finding, verify in DB.
#[tokio::test]
async fn test_full_workflow_echo_and_log() {
    let (config, memory) = setup_test_env().await;

    // Step 1: Execute a command
    let run_tool = RunCommandTool::new(config);
    let run_result = run_tool
        .call(RunCommandArgs {
            command: "echo integration test".to_string(),
            timeout_override: None,
        })
        .await
        .unwrap();

    assert!(run_result.success, "echo should succeed");
    assert!(
        run_result.stdout.contains("integration test"),
        "stdout should contain 'integration test'"
    );
    assert_eq!(run_result.exit_code, 0);

    // Step 2: Log the finding
    let log_tool = LogDiscoveryTool::new(memory.clone());
    let log_result = log_tool
        .call(LogDiscoveryArgs {
            run_id: None,
            host: Some("127.0.0.1".to_string()),
            finding_type: "test_output".to_string(),
            data: run_result.stdout.clone(),
        })
        .await
        .unwrap();

    assert!(log_result.finding_id > 0, "finding_id should be positive");
    assert_eq!(log_result.finding_type, "test_output");

    // Step 3: Verify finding persisted in DB
    let finding_id = log_result.finding_id;
    let (ft, data): (String, String) = memory
        .call(move |conn| {
            let row = conn.query_row(
                "SELECT finding_type, data FROM findings WHERE id = ?1",
                rusqlite::params![finding_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok(row)
        })
        .await
        .unwrap();

    assert_eq!(ft, "test_output");
    assert!(
        data.contains("integration test"),
        "persisted data should contain command output"
    );
}

/// Test 3: run_command with a real network tool (arp -a) returns structured output.
#[tokio::test]
async fn test_run_command_with_arp() {
    let config = Arc::new(Config::default());
    let run_tool = RunCommandTool::new(config);

    let result = run_tool
        .call(RunCommandArgs {
            command: "arp -a".to_string(),
            timeout_override: Some(10),
        })
        .await;

    // arp -a should either succeed or fail gracefully (not panic)
    match result {
        Ok(run_result) => {
            // On macOS/Linux with network, this returns ARP table
            assert_eq!(run_result.command, "arp -a");
            // exit_code is a field on RunCommandResult
            assert!(
                run_result.exit_code == 0 || !run_result.success,
                "should have valid exit_code"
            );
        }
        Err(e) => {
            // PermissionDenied or ToolNotFound are acceptable
            let err_str = format!("{e}");
            assert!(
                err_str.contains("Permission denied") || err_str.contains("Tool not found"),
                "unexpected error: {e}"
            );
        }
    }
}

/// Test 4: run_command with timeout verifies timeout handling.
#[tokio::test]
async fn test_run_command_timeout() {
    let config = Arc::new(Config::default());
    let run_tool = RunCommandTool::new(config);

    let result = run_tool
        .call(RunCommandArgs {
            command: "sleep 5".to_string(),
            timeout_override: Some(1),
        })
        .await;

    assert!(result.is_err(), "sleep 5 with 1s timeout should fail");
    match result.unwrap_err() {
        eugene::tools::ToolError::Timeout(secs) => {
            assert_eq!(secs, 1, "timeout should be 1 second");
        }
        other => panic!("expected Timeout error, got: {other}"),
    }
}

/// Test 5: log_discovery with structured metadata JSON.
#[tokio::test]
async fn test_log_discovery_with_metadata() {
    let (_config, memory) = setup_test_env().await;

    let log_tool = LogDiscoveryTool::new(memory.clone());

    // Log a finding with structured metadata as JSON in the data field
    let metadata = serde_json::json!({
        "host": "192.168.1.1",
        "port": 22,
        "service": "ssh"
    });

    let log_result = log_tool
        .call(LogDiscoveryArgs {
            run_id: None,
            host: Some("192.168.1.1".to_string()),
            finding_type: "service_enum".to_string(),
            data: metadata.to_string(),
        })
        .await
        .unwrap();

    assert!(log_result.finding_id > 0);

    // Query DB and verify data field contains the JSON
    let finding_id = log_result.finding_id;
    let (ft, data): (String, String) = memory
        .call(move |conn| {
            let row = conn.query_row(
                "SELECT finding_type, data FROM findings WHERE id = ?1",
                rusqlite::params![finding_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok(row)
        })
        .await
        .unwrap();

    assert_eq!(ft, "service_enum");

    // Parse the stored data back as JSON and verify fields
    let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert_eq!(parsed["host"], "192.168.1.1");
    assert_eq!(parsed["port"], 22);
    assert_eq!(parsed["service"], "ssh");
}
