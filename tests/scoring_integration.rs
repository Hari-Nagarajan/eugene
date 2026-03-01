//! Integration tests for scoring and script tool round-trips.
//!
//! Validates end-to-end flows:
//! - Score logging round-trip: log_score -> get_score_context reads back correct totals
//! - Detection penalty: detection event reduces total score by 100
//! - Unknown action rejection: bogus action returns error
//! - Script save/search round-trip: save_script -> search_scripts via FTS5
//! - Script execution: save_script -> run_script executes and returns output
//! - Script not found: run_script with nonexistent name returns error
//! - RunSummary includes scores: after scoring events, get_run_summary has total_score
//! - Tool factory counts: executor=5, orchestrator=10

use std::sync::Arc;
use eugene::config::Config;
use eugene::memory::{open_memory_store, init_schema, create_run};
use eugene::tools::*;
use rig::tool::Tool;

async fn setup() -> (Arc<Config>, Arc<tokio_rusqlite::Connection>, i64) {
    let memory = open_memory_store(":memory:").await.unwrap();
    init_schema(&memory).await.unwrap();
    let config = Arc::new(Config::default());
    let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
    (config, memory, run_id)
}

/// Test 1: log_score -> get_score_context round-trip with correct totals
#[tokio::test]
async fn test_score_logging_round_trip() {
    let (_config, memory, run_id) = setup().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let ctx_tool = GetScoreContextTool::new(memory.clone(), run_id);

    // Log host_discovered (+10)
    let r1 = log_tool.call(LogScoreArgs {
        action: "host_discovered".to_string(),
        risk_level: Some("low".to_string()),
    }).await.unwrap();
    assert_eq!(r1.points, 10);
    assert_eq!(r1.total_score, 10);

    // Log port_found (+5)
    let r2 = log_tool.call(LogScoreArgs {
        action: "port_found".to_string(),
        risk_level: None,
    }).await.unwrap();
    assert_eq!(r2.points, 5);
    assert_eq!(r2.total_score, 15);

    // Verify via get_score_context
    let ctx = ctx_tool.call(GetScoreContextArgs {}).await.unwrap();
    assert_eq!(ctx.total_score, 15);
    assert_eq!(ctx.detection_count, 0);
    assert_eq!(ctx.recent_events.len(), 2);
}

/// Test 2: Detection event reduces total score by 100
#[tokio::test]
async fn test_score_detection_penalty() {
    let (_config, memory, run_id) = setup().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let ctx_tool = GetScoreContextTool::new(memory.clone(), run_id);

    // Log host_discovered (+10)
    log_tool.call(LogScoreArgs {
        action: "host_discovered".to_string(),
        risk_level: Some("low".to_string()),
    }).await.unwrap();

    // Log detection (-100)
    let det = log_tool.call(LogScoreArgs {
        action: "detection".to_string(),
        risk_level: None,
    }).await.unwrap();
    assert_eq!(det.points, -100);

    // Verify
    let ctx = ctx_tool.call(GetScoreContextArgs {}).await.unwrap();
    assert_eq!(ctx.total_score, -90);
    assert_eq!(ctx.detection_count, 1);
}

/// Test 3: Unknown action is rejected
#[tokio::test]
async fn test_score_unknown_action_rejected() {
    let (_config, memory, run_id) = setup().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);

    let result = log_tool.call(LogScoreArgs {
        action: "bogus_action".to_string(),
        risk_level: None,
    }).await;
    assert!(result.is_err(), "Should reject unknown action");
}

/// Test 4: save_script -> search_scripts round-trip via FTS5
#[tokio::test]
async fn test_script_save_search_round_trip() {
    let (_config, memory, _run_id) = setup().await;
    let save_tool = SaveScriptTool::new(memory.clone());
    let search_tool = SearchScriptsTool::new(memory.clone());

    // Save a script
    let saved = save_tool.call(SaveScriptArgs {
        name: "sweep.sh".to_string(),
        code: "#!/bin/bash\necho hello".to_string(),
        language: "bash".to_string(),
        description: "ARP sweep tool".to_string(),
        tags: Some("[\"recon\",\"arp\"]".to_string()),
    }).await.unwrap();
    assert!(saved.script_id > 0);
    assert_eq!(saved.name, "sweep.sh");

    // Search by name
    let r1 = search_tool.call(SearchScriptsArgs {
        query: "sweep".to_string(),
        limit: Some(5),
    }).await.unwrap();
    assert!(r1.count >= 1, "Should find script by name, got count={}", r1.count);
    assert_eq!(r1.scripts[0].name, "sweep.sh");

    // Search by tag
    let r2 = search_tool.call(SearchScriptsArgs {
        query: "arp".to_string(),
        limit: None,
    }).await.unwrap();
    assert!(r2.count >= 1, "Should find script by tag 'arp', got count={}", r2.count);

    // Search for nonexistent
    let r3 = search_tool.call(SearchScriptsArgs {
        query: "nonexistent_xyz".to_string(),
        limit: None,
    }).await.unwrap();
    assert_eq!(r3.count, 0, "Should find nothing for nonexistent query");
}

/// Test 5: save_script -> run_script executes and returns output
#[tokio::test]
async fn test_script_execution() {
    let (config, memory, _run_id) = setup().await;
    let save_tool = SaveScriptTool::new(memory.clone());
    let run_tool = RunScriptTool::new(memory.clone(), config);

    // Save a script
    save_tool.call(SaveScriptArgs {
        name: "hello.sh".to_string(),
        code: "echo 'hello from eugene'".to_string(),
        language: "bash".to_string(),
        description: "Hello world test script".to_string(),
        tags: None,
    }).await.unwrap();

    // Run the script
    let result = run_tool.call(RunScriptArgs {
        name: "hello.sh".to_string(),
        timeout: None,
    }).await.unwrap();
    assert!(result.success, "Script should succeed, stderr={}", result.stderr);
    assert!(
        result.stdout.contains("hello from eugene"),
        "Stdout should contain greeting, got: {}",
        result.stdout
    );
}

/// Test 6: run_script with nonexistent name returns error
#[tokio::test]
async fn test_script_not_found() {
    let (config, memory, _run_id) = setup().await;
    let run_tool = RunScriptTool::new(memory.clone(), config);

    let result = run_tool.call(RunScriptArgs {
        name: "nonexistent.sh".to_string(),
        timeout: None,
    }).await;
    assert!(result.is_err(), "Should error for nonexistent script");
}

/// Test 7: RunSummary includes score data after scoring events
#[tokio::test]
async fn test_run_summary_includes_scores() {
    let (_config, memory, run_id) = setup().await;
    let log_tool = LogScoreTool::new(memory.clone(), run_id);
    let summary_tool = GetRunSummaryTool::new(memory.clone(), run_id);

    // Log 2 positive score events
    log_tool.call(LogScoreArgs {
        action: "host_discovered".to_string(),
        risk_level: Some("low".to_string()),
    }).await.unwrap();
    log_tool.call(LogScoreArgs {
        action: "service_identified".to_string(),
        risk_level: Some("medium".to_string()),
    }).await.unwrap();

    // Check run summary
    let summary = summary_tool.call(GetRunSummaryArgs {}).await.unwrap();
    assert_eq!(summary.total_score, 25, "total_score should be 10+15=25");
    assert_eq!(summary.detection_count, 0, "detection_count should be 0");

    // Log a detection
    log_tool.call(LogScoreArgs {
        action: "detection".to_string(),
        risk_level: None,
    }).await.unwrap();

    let summary2 = summary_tool.call(GetRunSummaryArgs {}).await.unwrap();
    assert_eq!(summary2.total_score, -75, "total_score should be 25-100=-75");
    assert_eq!(summary2.detection_count, 1, "detection_count should be 1");
}

/// Test 8: make_executor_tools returns 5 tools
#[tokio::test]
async fn test_executor_tools_count() {
    let (config, memory, _run_id) = setup().await;
    let tools = make_executor_tools(config, memory);
    assert_eq!(tools.len(), 5, "Executor should have 5 tools (2 recon + 3 script), got {}", tools.len());
}

/// Test 9: make_orchestrator_tools returns 10 tools
#[tokio::test]
async fn test_orchestrator_tools_count() {
    let (config, memory, run_id) = setup().await;

    use eugene::agent::mock::MockCompletionModel;
    use rig::message::AssistantContent;
    use rig::OneOrMany;
    use tokio::sync::Semaphore;

    let mock = MockCompletionModel::new(vec![
        OneOrMany::one(AssistantContent::text("done")),
    ]);
    let model = Arc::new(mock);
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));

    let tools = make_orchestrator_tools(model, config, memory, semaphore, run_id);
    assert_eq!(tools.len(), 10, "Orchestrator should have 10 tools (2 dispatch + 3 memory + 2 scoring + 3 script), got {}", tools.len());
}
