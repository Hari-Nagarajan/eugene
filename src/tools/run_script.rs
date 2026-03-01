use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::io::Write;
use tokio_rusqlite::Connection;

use crate::config::Config;
use crate::memory::{get_script_by_name, update_script_usage};
use crate::tools::ToolError;

/// Arguments for the run_script tool
#[derive(Deserialize)]
pub struct RunScriptArgs {
    /// Name of the script to execute (must exist in the database)
    pub name: String,
    /// Execution timeout in seconds. Defaults to 60 if omitted.
    pub timeout: Option<u64>,
}

/// Structured result from executing a script
#[derive(Debug, Serialize)]
pub struct RunScriptResult {
    /// Name of the executed script
    pub name: String,
    /// Standard output from the script
    pub stdout: String,
    /// Standard error from the script
    pub stderr: String,
    /// Process exit code
    pub exit_code: i32,
    /// Whether the script exited successfully (exit code 0)
    pub success: bool,
}

/// Tool for executing saved scripts by name.
///
/// Fetches the script from the database, writes it to a temporary file,
/// and executes it via the appropriate interpreter (bash or python3).
/// Uses tempfile crate for safe temp file management (auto-cleanup on drop).
pub struct RunScriptTool {
    memory: Arc<Connection>,
    #[allow(dead_code)]
    config: Arc<Config>,
}

impl RunScriptTool {
    pub fn new(memory: Arc<Connection>, config: Arc<Config>) -> Self {
        Self { memory, config }
    }
}

impl Tool for RunScriptTool {
    const NAME: &'static str = "run_script";

    type Error = ToolError;
    type Args = RunScriptArgs;
    type Output = RunScriptResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "run_script".to_string(),
            description: "Execute a saved script by name. Fetches the script from the \
                database, writes to a temp file, and executes it."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the script to execute (must exist in the database)"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Execution timeout in seconds (default: 60)"
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Fetch script from database
        let script = get_script_by_name(&self.memory, args.name.clone())
            .await?
            .ok_or_else(|| {
                ToolError::ExecutionFailed(format!("Script not found: {}", args.name))
            })?;

        // Determine file suffix and interpreter based on language
        let (suffix, interpreter) = match script.language.as_str() {
            "bash" => (".sh", "bash"),
            "python" => (".py", "python3"),
            _ => {
                return Err(ToolError::ExecutionFailed(format!(
                    "Unsupported script language: {}",
                    script.language
                )));
            }
        };

        // Create temp file with appropriate suffix
        let mut tmp = tempfile::Builder::new()
            .prefix("eugene-script-")
            .suffix(suffix)
            .tempfile()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create temp file: {}", e)))?;

        // Write script code to temp file
        tmp.write_all(script.code.as_bytes())
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write script: {}", e)))?;

        // Execute via interpreter (not direct execution -- avoids permission issues)
        let timeout_secs = args.timeout.unwrap_or(60);
        let child = tokio::process::Command::new(interpreter)
            .arg(tmp.path())
            .output();

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            child,
        )
        .await
        .map_err(|_| ToolError::Timeout(timeout_secs))?
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to execute script: {}", e)))?;

        // Update usage count (ignore errors -- entropy-goblin error-as-value pattern)
        let _ = update_script_usage(&self.memory, script.id).await;

        let exit_code = output.status.code().unwrap_or(-1);

        // Temp file auto-cleaned on drop when `tmp` goes out of scope
        Ok(RunScriptResult {
            name: args.name,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code,
            success: exit_code == 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    async fn setup_tool() -> RunScriptTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = Arc::new(Config::default());
        RunScriptTool::new(conn, config)
    }

    #[tokio::test]
    async fn test_run_script_not_found() {
        let tool = setup_tool().await;
        let result = tool
            .call(RunScriptArgs {
                name: "nonexistent.sh".to_string(),
                timeout: None,
            })
            .await;

        assert!(result.is_err(), "Should error for nonexistent script");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent.sh"), "Error should mention script name");
    }
}
