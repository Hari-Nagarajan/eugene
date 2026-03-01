use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::log_finding;
use crate::tools::ToolError;

/// Arguments for the log_discovery tool
#[derive(Deserialize)]
pub struct LogDiscoveryArgs {
    /// Run ID to associate this finding with
    pub run_id: Option<i64>,
    /// Host IP or hostname the finding relates to (e.g., "192.168.1.1")
    pub host: Option<String>,
    /// Type/category of finding (e.g., "open_port", "host_discovery", "service_enum", "vuln_detect")
    pub finding_type: String,
    /// Description or structured data about the finding
    pub data: String,
}

/// Structured result from logging a discovery
#[derive(Serialize)]
pub struct LogDiscoveryResult {
    /// Database ID of the persisted finding
    pub finding_id: i64,
    /// The finding type that was logged
    pub finding_type: String,
    /// ISO 8601 timestamp when the finding was logged
    pub logged_at: String,
}

/// Tool for persisting structured findings to memory.
///
/// This is NOT a CLI command -- it's a direct database operation.
/// The agent calls this after reasoning about run_command output
/// to persist what matters (hosts, ports, services, vulnerabilities).
/// Findings become queryable via the memory system.
pub struct LogDiscoveryTool {
    memory: Arc<Connection>,
}

impl LogDiscoveryTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

impl Tool for LogDiscoveryTool {
    const NAME: &'static str = "log_discovery";

    type Error = ToolError;
    type Args = LogDiscoveryArgs;
    type Output = LogDiscoveryResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "log_discovery".to_string(),
            description: "Log a structured finding to memory for later recall. \
                Use this to persist important discoveries (hosts, ports, services, \
                vulnerabilities). The finding becomes searchable via FTS5."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "run_id": {
                        "type": "integer",
                        "description": "Run ID to associate this finding with (null for standalone findings)"
                    },
                    "host": {
                        "type": "string",
                        "description": "Host IP or hostname the finding relates to"
                    },
                    "finding_type": {
                        "type": "string",
                        "description": "Category of finding: host_discovery, port_scan, service_enum, vuln_detect, etc."
                    },
                    "data": {
                        "type": "string",
                        "description": "Description or structured data about the finding"
                    }
                },
                "required": ["finding_type", "data"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let finding_type_clone = args.finding_type.clone();

        let finding_id = log_finding(
            &self.memory,
            args.run_id,
            args.host,
            args.finding_type,
            args.data,
        )
        .await?;

        let logged_at = chrono::Utc::now().to_rfc3339();

        Ok(LogDiscoveryResult {
            finding_id,
            finding_type: finding_type_clone,
            logged_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    async fn setup_tool() -> LogDiscoveryTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        LogDiscoveryTool::new(conn)
    }

    /// Test 1: log_discovery with valid args returns finding_id > 0
    #[tokio::test]
    async fn test_log_finding() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogDiscoveryArgs {
                run_id: None,
                host: Some("192.168.1.1".to_string()),
                finding_type: "open_port".to_string(),
                data: "port 22 SSH open".to_string(),
            })
            .await
            .unwrap();

        assert!(result.finding_id > 0, "finding_id should be positive");
        assert_eq!(result.finding_type, "open_port");
        assert!(!result.logged_at.is_empty(), "logged_at should be set");
    }

    /// Test 2: log_discovery persists to database (verify row exists)
    #[tokio::test]
    async fn test_finding_persisted() {
        let tool = setup_tool().await;
        let result = tool
            .call(LogDiscoveryArgs {
                run_id: None,
                host: Some("10.0.0.1".to_string()),
                finding_type: "service_enum".to_string(),
                data: "Apache httpd 2.4.41 on port 80".to_string(),
            })
            .await
            .unwrap();

        let finding_id = result.finding_id;

        // Query the database directly to verify persistence
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

        assert_eq!(ft, "service_enum");
        assert_eq!(data, "Apache httpd 2.4.41 on port 80");
    }
}
