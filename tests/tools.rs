//! Integration tests for tool behaviors: command execution, timeout,
//! scoring round-trips, script save/search/execute, script not found.

mod common;

use eugene::tools::*;
use rig::tool::Tool;

// ========== Command Execution Tests ==========

#[tokio::test]
async fn test_full_workflow_echo_and_log() {
    let (config, memory) = common::setup_env().await;

    // Execute a command
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

    // Log the finding
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

    // Verify finding persisted in DB
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

#[tokio::test]
async fn test_run_command_timeout() {
    let (config, _memory) = common::setup_env().await;
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

// ========== Scoring Tests ==========

#[tokio::test]
async fn test_score_logging_round_trip() {
    let (_config, memory, run_id) = common::setup_with_run().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let ctx_tool = GetScoreContextTool::new(memory.clone(), run_id);

    let r1 = log_tool
        .call(LogScoreArgs {
            action: "host_discovered".to_string(),
            risk_level: Some("low".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(r1.points, 10);
    assert_eq!(r1.total_score, 10);

    let r2 = log_tool
        .call(LogScoreArgs {
            action: "port_found".to_string(),
            risk_level: None,
        })
        .await
        .unwrap();
    assert_eq!(r2.points, 5);
    assert_eq!(r2.total_score, 15);

    let ctx = ctx_tool.call(GetScoreContextArgs {}).await.unwrap();
    assert_eq!(ctx.total_score, 15);
    assert_eq!(ctx.detection_count, 0);
    assert_eq!(ctx.recent_events.len(), 2);
}

#[tokio::test]
async fn test_score_detection_penalty() {
    let (_config, memory, run_id) = common::setup_with_run().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let ctx_tool = GetScoreContextTool::new(memory.clone(), run_id);

    log_tool
        .call(LogScoreArgs {
            action: "host_discovered".to_string(),
            risk_level: Some("low".to_string()),
        })
        .await
        .unwrap();

    let det = log_tool
        .call(LogScoreArgs {
            action: "detection".to_string(),
            risk_level: None,
        })
        .await
        .unwrap();
    assert_eq!(det.points, -100);

    let ctx = ctx_tool.call(GetScoreContextArgs {}).await.unwrap();
    assert_eq!(ctx.total_score, -90);
    assert_eq!(ctx.detection_count, 1);
}

#[tokio::test]
async fn test_score_unknown_action_rejected() {
    let (_config, memory, run_id) = common::setup_with_run().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);

    let result = log_tool
        .call(LogScoreArgs {
            action: "bogus_action".to_string(),
            risk_level: None,
        })
        .await;
    assert!(result.is_err(), "Should reject unknown action");
}

#[tokio::test]
async fn test_run_summary_includes_scores() {
    let (_config, memory, run_id) = common::setup_with_run().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let summary_tool = GetRunSummaryTool::new(memory.clone(), run_id);

    log_tool
        .call(LogScoreArgs {
            action: "host_discovered".to_string(),
            risk_level: Some("low".to_string()),
        })
        .await
        .unwrap();
    log_tool
        .call(LogScoreArgs {
            action: "service_identified".to_string(),
            risk_level: Some("medium".to_string()),
        })
        .await
        .unwrap();

    let summary = summary_tool.call(GetRunSummaryArgs {}).await.unwrap();
    assert_eq!(summary.total_score, 25, "total_score should be 10+15=25");
    assert_eq!(summary.detection_count, 0);

    log_tool
        .call(LogScoreArgs {
            action: "detection".to_string(),
            risk_level: None,
        })
        .await
        .unwrap();

    let summary2 = summary_tool.call(GetRunSummaryArgs {}).await.unwrap();
    assert_eq!(summary2.total_score, -75, "total_score should be 25-100=-75");
    assert_eq!(summary2.detection_count, 1);
}

// ========== Script Tests ==========

#[tokio::test]
async fn test_script_save_search_round_trip() {
    let (_config, memory, _run_id) = common::setup_with_run().await;
    let save_tool = SaveScriptTool::new(memory.clone());
    let search_tool = SearchScriptsTool::new(memory.clone());

    let saved = save_tool
        .call(SaveScriptArgs {
            name: "sweep.sh".to_string(),
            code: "#!/bin/bash\necho hello".to_string(),
            language: "bash".to_string(),
            description: "ARP sweep tool".to_string(),
            tags: Some("[\"recon\",\"arp\"]".to_string()),
        })
        .await
        .unwrap();
    assert!(saved.script_id > 0);
    assert_eq!(saved.name, "sweep.sh");

    let r1 = search_tool
        .call(SearchScriptsArgs {
            query: "sweep".to_string(),
            limit: Some(5),
        })
        .await
        .unwrap();
    assert!(
        r1.count >= 1,
        "Should find script by name, got count={}",
        r1.count
    );
    assert_eq!(r1.scripts[0].name, "sweep.sh");

    let r2 = search_tool
        .call(SearchScriptsArgs {
            query: "arp".to_string(),
            limit: None,
        })
        .await
        .unwrap();
    assert!(
        r2.count >= 1,
        "Should find script by tag 'arp', got count={}",
        r2.count
    );

    let r3 = search_tool
        .call(SearchScriptsArgs {
            query: "nonexistent_xyz".to_string(),
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(r3.count, 0, "Should find nothing for nonexistent query");
}

#[tokio::test]
async fn test_script_execution() {
    let (config, memory, _run_id) = common::setup_with_run().await;
    let save_tool = SaveScriptTool::new(memory.clone());
    let run_tool = RunScriptTool::new(memory.clone(), config);

    save_tool
        .call(SaveScriptArgs {
            name: "hello.sh".to_string(),
            code: "echo 'hello from eugene'".to_string(),
            language: "bash".to_string(),
            description: "Hello world test script".to_string(),
            tags: None,
        })
        .await
        .unwrap();

    let result = run_tool
        .call(RunScriptArgs {
            name: "hello.sh".to_string(),
            timeout: None,
        })
        .await
        .unwrap();
    assert!(
        result.success,
        "Script should succeed, stderr={}",
        result.stderr
    );
    assert!(
        result.stdout.contains("hello from eugene"),
        "Stdout should contain greeting, got: {}",
        result.stdout
    );
}

#[tokio::test]
async fn test_python_script_execution() {
    let (config, memory, _run_id) = common::setup_with_run().await;
    let save_tool = SaveScriptTool::new(memory.clone());
    let run_tool = RunScriptTool::new(memory.clone(), config);

    save_tool
        .call(SaveScriptArgs {
            name: "hello.py".to_string(),
            code: "print('hello from python')".to_string(),
            language: "python".to_string(),
            description: "Python hello world test".to_string(),
            tags: None,
        })
        .await
        .unwrap();

    let result = run_tool
        .call(RunScriptArgs {
            name: "hello.py".to_string(),
            timeout: None,
        })
        .await
        .unwrap();
    assert!(
        result.success,
        "Python script should succeed, stderr={}",
        result.stderr
    );
    assert!(
        result.stdout.contains("hello from python"),
        "Stdout should contain python greeting, got: {}",
        result.stdout
    );
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_script_not_found() {
    let (config, memory, _run_id) = common::setup_with_run().await;
    let run_tool = RunScriptTool::new(memory.clone(), config);

    let result = run_tool
        .call(RunScriptArgs {
            name: "nonexistent.sh".to_string(),
            timeout: None,
        })
        .await;
    assert!(result.is_err(), "Should error for nonexistent script");
}
