use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::log_finding;
use crate::tools::ToolError;

/// Arguments for the remember_finding tool
#[derive(Deserialize)]
pub struct RememberFindingArgs {
    /// Host IP or hostname the finding relates to
    pub host: String,
    /// Category of finding (host, port, service, os, vuln, topology, note)
    pub finding_type: String,
    /// Structured description of the finding
    pub data: String,
}

/// Result from persisting a finding
#[derive(Serialize)]
pub struct RememberFindingResult {
    /// Database ID of the persisted finding
    pub finding_id: i64,
    /// ISO 8601 timestamp when the finding was logged
    pub logged_at: String,
}

/// Tool for persisting findings to memory for cross-phase recall.
///
/// Used by the orchestrator to record analyzed results from executor tasks.
/// Delegates to the `log_finding` query with a bound run_id.
pub struct RememberFindingTool {
    memory: Arc<Connection>,
    run_id: i64,
}

impl RememberFindingTool {
    pub fn new(memory: Arc<Connection>, run_id: i64) -> Self {
        Self { memory, run_id }
    }
}

impl Tool for RememberFindingTool {
    const NAME: &'static str = "remember_finding";

    type Error = ToolError;
    type Args = RememberFindingArgs;
    type Output = RememberFindingResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "remember_finding".to_string(),
            description: "Persist a finding to memory for cross-phase recall. \
                Call this after analyzing executor results to record significant \
                discoveries (hosts, ports, services, vulnerabilities)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "Host IP or hostname the finding relates to"
                    },
                    "finding_type": {
                        "type": "string",
                        "description": "Category: host, port, service, os, vuln, topology, note"
                    },
                    "data": {
                        "type": "string",
                        "description": "Structured description of the finding"
                    }
                },
                "required": ["host", "finding_type", "data"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let finding_id = log_finding(
            &self.memory,
            Some(self.run_id),
            Some(args.host),
            args.finding_type,
            args.data,
        )
        .await?;

        let logged_at = chrono::Utc::now().to_rfc3339();

        Ok(RememberFindingResult {
            finding_id,
            logged_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run};

    async fn setup_tool() -> RememberFindingTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        RememberFindingTool::new(conn, run_id)
    }

    #[tokio::test]
    async fn test_remember_finding_persists_and_returns_id() {
        let tool = setup_tool().await;
        let result = tool
            .call(RememberFindingArgs {
                host: "10.0.0.1".to_string(),
                finding_type: "port_scan".to_string(),
                data: "port 22 open".to_string(),
            })
            .await
            .unwrap();

        assert!(result.finding_id > 0, "finding_id should be positive");
        assert!(!result.logged_at.is_empty(), "logged_at should be set");
    }

    #[tokio::test]
    async fn test_remember_finding_verifiable_in_db() {
        let tool = setup_tool().await;
        let result = tool
            .call(RememberFindingArgs {
                host: "10.0.0.1".to_string(),
                finding_type: "service".to_string(),
                data: "SSH OpenSSH 8.2".to_string(),
            })
            .await
            .unwrap();

        let finding_id = result.finding_id;
        let (ft, data): (String, String) = tool
            .memory
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

        assert_eq!(ft, "service");
        assert_eq!(data, "SSH OpenSSH 8.2");
    }
}
