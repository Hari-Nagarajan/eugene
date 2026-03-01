use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::{points_for_action, log_score_event, get_score_summary};
use crate::tools::ToolError;

/// Arguments for the log_score tool
#[derive(Deserialize)]
pub struct LogScoreArgs {
    /// The scoring action type (e.g., "host_discovered", "port_found", "detection")
    pub action: String,
    /// Risk level of the action: "low", "medium", or "high". Defaults to "low" if omitted.
    pub risk_level: Option<String>,
}

/// Structured result from logging a score event
#[derive(Debug, Serialize)]
pub struct LogScoreResult {
    /// Database ID of the score event
    pub event_id: i64,
    /// The action that was scored
    pub action: String,
    /// Points awarded (negative for detection)
    pub points: i64,
    /// Running total score for this run
    pub total_score: i64,
}

/// Tool for logging scoring events during a campaign run.
///
/// Each action maps to a fixed point value (e.g., host_discovered=10,
/// detection=-100). The tool validates the action type, logs the event,
/// and returns the updated total score.
pub struct LogScoreTool {
    memory: Arc<Connection>,
    run_id: i64,
}

impl LogScoreTool {
    pub fn new(memory: Arc<Connection>, run_id: i64) -> Self {
        Self { memory, run_id }
    }
}

impl Tool for LogScoreTool {
    const NAME: &'static str = "log_score";

    type Error = ToolError;
    type Args = LogScoreArgs;
    type Output = LogScoreResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "log_score".to_string(),
            description: "Log a scoring event for the current campaign run. \
                Each action has a fixed point value. Use this after completing \
                reconnaissance or exploitation actions to track progress."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Scoring action type. Valid actions: host_discovered (10), port_found (5), service_identified (15), os_fingerprinted (20), vuln_detected (25), credential_captured (50), successful_login (75), privilege_escalation (150), rce_achieved (200), data_exfiltrated (100), detection (-100)"
                    },
                    "risk_level": {
                        "type": "string",
                        "description": "Risk level of the action",
                        "enum": ["low", "medium", "high"]
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Validate action against fixed point table
        let points = points_for_action(&args.action).ok_or_else(|| {
            ToolError::ExecutionFailed(format!(
                "Unknown action type: {}. Valid actions: host_discovered, port_found, \
                 service_identified, os_fingerprinted, vuln_detected, credential_captured, \
                 successful_login, privilege_escalation, rce_achieved, data_exfiltrated, detection",
                args.action
            ))
        })?;

        let risk_level = args.risk_level.unwrap_or_else(|| "low".to_string());
        let detected = args.action == "detection";

        let event_id = log_score_event(
            &self.memory,
            Some(self.run_id),
            args.action.clone(),
            risk_level,
            detected,
        )
        .await?;

        let summary = get_score_summary(&self.memory, self.run_id).await?;

        Ok(LogScoreResult {
            event_id,
            action: args.action,
            points,
            total_score: summary.total_score,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run};

    async fn setup_tool() -> LogScoreTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        LogScoreTool::new(conn, run_id)
    }

    #[tokio::test]
    async fn test_log_score_valid_action() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogScoreArgs {
                action: "host_discovered".to_string(),
                risk_level: Some("low".to_string()),
            })
            .await
            .unwrap();

        assert!(result.event_id > 0, "event_id should be positive");
        assert_eq!(result.action, "host_discovered");
        assert_eq!(result.points, 10);
        assert_eq!(result.total_score, 10);
    }

    #[tokio::test]
    async fn test_log_score_unknown_action_error() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogScoreArgs {
                action: "bogus".to_string(),
                risk_level: None,
            })
            .await;

        assert!(result.is_err(), "Should reject unknown action");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bogus"), "Error should mention the invalid action");
        assert!(err.contains("host_discovered"), "Error should list valid actions");
    }

    #[tokio::test]
    async fn test_log_score_detection_penalty() {
        let tool = setup_tool().await;

        // First log a positive action
        tool.call(LogScoreArgs {
            action: "host_discovered".to_string(),
            risk_level: Some("low".to_string()),
        })
        .await
        .unwrap();

        // Then log a detection (penalty)
        let result = tool
            .call(LogScoreArgs {
                action: "detection".to_string(),
                risk_level: None,
            })
            .await
            .unwrap();

        assert_eq!(result.points, -100);
        // Total should be 10 + (-100) = -90
        assert_eq!(result.total_score, -90);
    }
}
