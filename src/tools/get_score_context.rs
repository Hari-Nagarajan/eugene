use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::get_score_summary;
use crate::tools::ToolError;

/// Arguments for the get_score_context tool (empty -- no parameters required)
#[derive(Deserialize)]
pub struct GetScoreContextArgs {}

/// Simplified score event for context output (drops timestamp for conciseness)
#[derive(Serialize)]
pub struct ScoreEventSummary {
    /// The action that was scored
    pub action: String,
    /// Points awarded
    pub points: i64,
    /// Risk level of the action
    pub risk_level: String,
    /// Whether this event was a detection
    pub detected: bool,
}

/// Structured result with current score context
#[derive(Serialize)]
pub struct GetScoreContextResult {
    /// Running total score for this run
    pub total_score: i64,
    /// Number of detection events
    pub detection_count: i64,
    /// Recent scoring events (most recent first)
    pub recent_events: Vec<ScoreEventSummary>,
}

/// Tool for retrieving current score context during a campaign run.
///
/// Returns total score, detection count, and recent events. Agents use
/// this before deciding on exploitation actions to calculate expected value.
pub struct GetScoreContextTool {
    memory: Arc<Connection>,
    run_id: i64,
}

impl GetScoreContextTool {
    pub fn new(memory: Arc<Connection>, run_id: i64) -> Self {
        Self { memory, run_id }
    }
}

impl Tool for GetScoreContextTool {
    const NAME: &'static str = "get_score_context";

    type Error = ToolError;
    type Args = GetScoreContextArgs;
    type Output = GetScoreContextResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "get_score_context".to_string(),
            description: "Get current score summary: total points, detection count, \
                and recent events. Use before deciding on exploitation actions to \
                calculate EV."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let summary = get_score_summary(&self.memory, self.run_id).await?;

        let recent_events = summary
            .recent_events
            .into_iter()
            .map(|e| ScoreEventSummary {
                action: e.action,
                points: e.points,
                risk_level: e.risk_level,
                detected: e.detected,
            })
            .collect();

        Ok(GetScoreContextResult {
            total_score: summary.total_score,
            detection_count: summary.detection_count,
            recent_events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run, log_score_event};

    async fn setup_tool() -> GetScoreContextTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        GetScoreContextTool::new(conn, run_id)
    }

    #[tokio::test]
    async fn test_get_score_context_empty() {
        let tool = setup_tool().await;
        let result = tool
            .call(GetScoreContextArgs {})
            .await
            .unwrap();

        assert_eq!(result.total_score, 0, "Empty run should have total_score=0");
        assert_eq!(result.detection_count, 0, "Empty run should have detection_count=0");
        assert!(result.recent_events.is_empty(), "Empty run should have no events");
    }

    #[tokio::test]
    async fn test_get_score_context_with_events() {
        let tool = setup_tool().await;

        // Log some events directly via query function
        log_score_event(&tool.memory, Some(tool.run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&tool.memory, Some(tool.run_id), "port_found".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&tool.memory, Some(tool.run_id), "detection".to_string(), "high".to_string(), true).await.unwrap();

        let result = tool
            .call(GetScoreContextArgs {})
            .await
            .unwrap();

        assert_eq!(result.total_score, -85, "Total should be 10+5-100=-85");
        assert_eq!(result.detection_count, 1, "Should count 1 detection");
        assert_eq!(result.recent_events.len(), 3, "Should have 3 recent events");
    }
}
