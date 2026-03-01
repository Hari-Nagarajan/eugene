use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::get_run_summary;
use crate::tools::ToolError;

/// Arguments for the get_run_summary tool (empty -- no parameters needed)
#[derive(Deserialize)]
pub struct GetRunSummaryArgs {}

/// Result containing run statistics including scoring data
#[derive(Serialize)]
pub struct GetRunSummaryResult {
    /// Total number of tasks in this run
    pub task_count: i64,
    /// Total number of findings in this run
    pub finding_count: i64,
    /// Number of completed tasks
    pub completed_task_count: i64,
    /// Number of failed tasks
    pub failed_task_count: i64,
    /// Total score from scoring events (Phase 5)
    pub total_score: i64,
    /// Number of detection events (Phase 5)
    pub detection_count: i64,
}

/// Tool for getting summary statistics of the current run.
///
/// Returns counts of tasks and findings for the orchestrator to
/// track campaign progress and decide next actions.
pub struct GetRunSummaryTool {
    memory: Arc<Connection>,
    run_id: i64,
}

impl GetRunSummaryTool {
    pub fn new(memory: Arc<Connection>, run_id: i64) -> Self {
        Self { memory, run_id }
    }
}

impl Tool for GetRunSummaryTool {
    const NAME: &'static str = "get_run_summary";

    type Error = ToolError;
    type Args = GetRunSummaryArgs;
    type Output = GetRunSummaryResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "get_run_summary".to_string(),
            description: "Get counts of findings and tasks for this run. \
                Use to track campaign progress and decide when to proceed \
                to the next phase."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let summary = get_run_summary(&self.memory, self.run_id).await?;

        Ok(GetRunSummaryResult {
            task_count: summary.task_count,
            finding_count: summary.finding_count,
            completed_task_count: summary.completed_task_count,
            failed_task_count: summary.failed_task_count,
            total_score: summary.total_score,
            detection_count: summary.detection_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run, log_task, update_task, log_finding};

    async fn setup_tool_with_data() -> GetRunSummaryTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        // Create tasks and findings
        let t1 = log_task(&conn, run_id, "scan1", "scan it").await.unwrap();
        let t2 = log_task(&conn, run_id, "scan2", "scan more").await.unwrap();
        update_task(&conn, t1, "completed", "ok").await.unwrap();
        update_task(&conn, t2, "failed", "timeout").await.unwrap();
        log_finding(&conn, Some(run_id), Some("host".to_string()), "port".to_string(), "22 open".to_string()).await.unwrap();

        GetRunSummaryTool::new(conn, run_id)
    }

    #[tokio::test]
    async fn test_get_run_summary_returns_counts() {
        let tool = setup_tool_with_data().await;
        let result = tool.call(GetRunSummaryArgs {}).await.unwrap();

        assert_eq!(result.task_count, 2);
        assert_eq!(result.finding_count, 1);
        assert_eq!(result.completed_task_count, 1);
        assert_eq!(result.failed_task_count, 1);
    }

    #[tokio::test]
    async fn test_get_run_summary_empty_run() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let tool = GetRunSummaryTool::new(conn, run_id);
        let result = tool.call(GetRunSummaryArgs {}).await.unwrap();

        assert_eq!(result.task_count, 0);
        assert_eq!(result.finding_count, 0);
        assert_eq!(result.completed_task_count, 0);
        assert_eq!(result.failed_task_count, 0);
    }
}
