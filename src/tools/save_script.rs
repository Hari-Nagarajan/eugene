use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::tools::ToolError;

/// Arguments for the save_script tool
#[derive(Deserialize)]
pub struct SaveScriptArgs {
    /// Unique name for the script (e.g., "sweep.sh")
    pub name: String,
    /// The script source code
    pub code: String,
    /// Script language: "bash" or "python"
    pub language: String,
    /// Human-readable description of what the script does
    pub description: String,
    /// Optional JSON array of tags (e.g., "[\"recon\", \"network\"]")
    pub tags: Option<String>,
}

/// Structured result from saving a script
#[derive(Debug, Serialize)]
pub struct SaveScriptResult {
    /// Database ID of the saved script
    pub script_id: i64,
    /// Name of the saved script
    pub name: String,
    /// ISO 8601 timestamp when the script was saved
    pub saved_at: String,
}

/// Tool for persisting reusable scripts to the database.
///
/// Scripts are stored with name, code, language, description, and tags.
/// If a script with the same name already exists, it will be updated (upsert).
/// Language must be "bash" or "python".
pub struct SaveScriptTool {
    memory: Arc<Connection>,
}

impl SaveScriptTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

impl Tool for SaveScriptTool {
    const NAME: &'static str = "save_script";

    type Error = ToolError;
    type Args = SaveScriptArgs;
    type Output = SaveScriptResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "save_script".to_string(),
            description: "Save a reusable script to the database. If a script with the \
                same name exists, it will be updated. Language must be 'bash' or 'python'."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique name for the script (e.g., 'sweep.sh')"
                    },
                    "code": {
                        "type": "string",
                        "description": "The script source code"
                    },
                    "language": {
                        "type": "string",
                        "description": "Script language: 'bash' or 'python'"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of what the script does"
                    },
                    "tags": {
                        "type": "string",
                        "description": "Optional JSON array of tags (e.g., '[\"recon\", \"network\"]')"
                    }
                },
                "required": ["name", "code", "language", "description"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        todo!("SaveScriptTool::call not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    async fn setup_tool() -> SaveScriptTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        SaveScriptTool::new(conn)
    }

    #[tokio::test]
    async fn test_save_script_valid() {
        let tool = setup_tool().await;
        let result = tool
            .call(SaveScriptArgs {
                name: "sweep.sh".to_string(),
                code: "#!/bin/bash\narp-scan -l".to_string(),
                language: "bash".to_string(),
                description: "ARP sweep".to_string(),
                tags: Some("[\"recon\"]".to_string()),
            })
            .await
            .unwrap();

        assert!(result.script_id > 0, "script_id should be positive");
        assert_eq!(result.name, "sweep.sh");
        assert!(!result.saved_at.is_empty(), "saved_at should be set");
    }

    #[tokio::test]
    async fn test_save_script_invalid_language() {
        let tool = setup_tool().await;
        let result = tool
            .call(SaveScriptArgs {
                name: "test.rb".to_string(),
                code: "puts 'hello'".to_string(),
                language: "ruby".to_string(),
                description: "Ruby script".to_string(),
                tags: None,
            })
            .await;

        assert!(result.is_err(), "Should reject unsupported language");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("ruby"), "Error should mention the invalid language");
    }
}
