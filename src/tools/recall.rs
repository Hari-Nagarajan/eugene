use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::get_findings_by_host;
use crate::tools::ToolError;

/// Arguments for the recall_findings tool
#[derive(Deserialize)]
pub struct RecallFindingsArgs {
    /// Host IP or hostname to retrieve findings for
    pub host: String,
}

/// Simplified view of a finding for recall results
#[derive(Serialize)]
pub struct FindingSummary {
    /// Category of finding
    pub finding_type: String,
    /// Description of the finding
    pub data: String,
    /// When the finding was recorded
    pub timestamp: String,
}

/// Result from recalling findings for a host
#[derive(Serialize)]
pub struct RecallFindingsResult {
    /// List of findings for the requested host
    pub findings: Vec<FindingSummary>,
    /// Total count of findings returned
    pub count: usize,
}

/// Tool for retrieving all findings for a specific host from memory.
///
/// Used by the orchestrator to check what's known about a host before
/// planning the next phase of reconnaissance.
pub struct RecallFindingsTool {
    memory: Arc<Connection>,
}

impl RecallFindingsTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

impl Tool for RecallFindingsTool {
    const NAME: &'static str = "recall_findings";

    type Error = ToolError;
    type Args = RecallFindingsArgs;
    type Output = RecallFindingsResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "recall_findings".to_string(),
            description: "Retrieve all findings for a specific host from memory. \
                Call this before planning the next reconnaissance phase to check \
                what is already known about a target."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host IP or hostname to retrieve findings for"
                    }
                },
                "required": ["host"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let findings = get_findings_by_host(&self.memory, args.host).await?;

        let summaries: Vec<FindingSummary> = findings
            .into_iter()
            .map(|f| FindingSummary {
                finding_type: f.finding_type,
                data: f.data,
                timestamp: f.timestamp,
            })
            .collect();

        let count = summaries.len();

        Ok(RecallFindingsResult {
            findings: summaries,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run, log_finding};

    async fn setup_with_findings() -> (RecallFindingsTool, Arc<Connection>) {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        // Insert some findings
        log_finding(&conn, Some(run_id), Some("10.0.0.1".to_string()), "port_scan".to_string(), "port 22 open".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("10.0.0.1".to_string()), "service".to_string(), "SSH OpenSSH 8.2".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("10.0.0.2".to_string()), "port_scan".to_string(), "port 80 open".to_string()).await.unwrap();

        let tool = RecallFindingsTool::new(conn.clone());
        (tool, conn)
    }

    #[tokio::test]
    async fn test_recall_findings_returns_correct_count() {
        let (tool, _conn) = setup_with_findings().await;
        let result = tool
            .call(RecallFindingsArgs {
                host: "10.0.0.1".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.count, 2, "Should find 2 findings for 10.0.0.1");
        assert_eq!(result.findings.len(), 2);
    }

    #[tokio::test]
    async fn test_recall_findings_empty_for_unknown_host() {
        let (tool, _conn) = setup_with_findings().await;
        let result = tool
            .call(RecallFindingsArgs {
                host: "unknown".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.count, 0);
        assert!(result.findings.is_empty());
    }

    #[tokio::test]
    async fn test_recall_findings_summary_fields() {
        let (tool, _conn) = setup_with_findings().await;
        let result = tool
            .call(RecallFindingsArgs {
                host: "10.0.0.1".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(result.findings[0].finding_type, "port_scan");
        assert_eq!(result.findings[0].data, "port 22 open");
        assert!(!result.findings[0].timestamp.is_empty());
    }
}
