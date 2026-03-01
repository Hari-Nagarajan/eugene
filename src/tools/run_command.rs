use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::config::Config;
use crate::executor::LocalExecutor;
use crate::tools::ToolError;

/// Maximum output length before truncation (chars).
/// Keeps responses within LLM context window limits.
const MAX_OUTPUT_LEN: usize = 4000;

/// Arguments for the run_command tool
#[derive(Deserialize)]
pub struct RunCommandArgs {
    /// Full command string to execute (e.g., "nmap -sS 192.168.1.0/24")
    pub command: String,
    /// Optional timeout override in seconds (overrides per-tool default)
    pub timeout_override: Option<u64>,
}

/// Structured result from command execution
#[derive(Debug, Serialize)]
pub struct RunCommandResult {
    /// The command that was executed
    pub command: String,
    /// Standard output from the command
    pub stdout: String,
    /// Standard error from the command
    pub stderr: String,
    /// Process exit code (0 = success)
    pub exit_code: i32,
    /// Whether the command completed successfully
    pub success: bool,
    /// Whether stdout was truncated for LLM consumption
    pub truncated: bool,
}

/// Tool for executing CLI commands on the target system.
///
/// Wraps LocalExecutor from Plan 01 with rig's Tool trait.
/// The agent constructs full command strings; this tool validates
/// via the safety layer, executes, and returns structured output.
pub struct RunCommandTool {
    config: Arc<Config>,
}

impl RunCommandTool {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

/// Truncate output if it exceeds MAX_OUTPUT_LEN.
/// Keeps first 2000 + last 2000 chars with a truncation marker.
fn truncate_output(output: &str) -> (String, bool) {
    if output.len() <= MAX_OUTPUT_LEN {
        return (output.to_string(), false);
    }

    let half = MAX_OUTPUT_LEN / 2;
    let head = &output[..half];
    let tail = &output[output.len() - half..];
    let truncated = format!(
        "{}\n\n[... OUTPUT TRUNCATED ({} chars total) ...]\n\n{}",
        head,
        output.len(),
        tail,
    );
    (truncated, true)
}

impl Tool for RunCommandTool {
    const NAME: &'static str = "run_command";

    type Error = ToolError;
    type Args = RunCommandArgs;
    type Output = RunCommandResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".to_string(),
            description: "Execute a CLI command on the Pi. Supports any command \
                (nmap, dig, arp, tcpdump, etc.). Safety layer blocks destructive \
                commands. Returns stdout, stderr, and exit code."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Full command string to execute (e.g., 'nmap -sS 192.168.1.0/24')"
                    },
                    "timeout_override": {
                        "type": "integer",
                        "description": "Timeout in seconds (overrides default). Use for long-running scans."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Determine timeout: override > per-tool default > 60s fallback
        let timeout_secs = args.timeout_override.unwrap_or_else(|| {
            // Extract binary name (first word of command)
            let binary = args
                .command
                .split_whitespace()
                .next()
                .unwrap_or("default");
            *self
                .config
                .tool_timeouts
                .get(binary)
                .or_else(|| self.config.tool_timeouts.get("default"))
                .unwrap_or(&60)
        });

        let executor = LocalExecutor;
        match executor.execute(&args.command, timeout_secs).await {
            Ok(stdout) => {
                let (truncated_stdout, was_truncated) = truncate_output(&stdout);
                Ok(RunCommandResult {
                    command: args.command,
                    stdout: truncated_stdout,
                    stderr: String::new(),
                    exit_code: 0,
                    success: true,
                    truncated: was_truncated,
                })
            }
            Err(ToolError::ExecutionFailed(stderr)) => {
                // Non-zero exit: return structured result, not an error.
                // The agent can reason about the failure from stderr.
                let (truncated_stderr, _) = truncate_output(&stderr);
                Ok(RunCommandResult {
                    command: args.command,
                    stdout: String::new(),
                    stderr: truncated_stderr,
                    exit_code: 1,
                    success: false,
                    truncated: false,
                })
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> RunCommandTool {
        RunCommandTool::new(Arc::new(Config::default()))
    }

    /// Test 1: run_command with "echo test" returns success
    #[tokio::test]
    async fn test_run_echo_command() {
        let tool = make_tool();
        let result = tool
            .call(RunCommandArgs {
                command: "echo test".to_string(),
                timeout_override: None,
            })
            .await
            .unwrap();

        assert!(result.success, "echo should succeed");
        assert!(result.stdout.contains("test"), "stdout should contain 'test'");
        assert_eq!(result.exit_code, 0);
        assert!(!result.truncated);
    }

    /// Test 2: Output truncation at > 4000 chars
    #[tokio::test]
    async fn test_output_truncation() {
        let long = "x".repeat(5000);
        let (truncated, was_truncated) = truncate_output(&long);
        assert!(was_truncated, "should be marked truncated");
        assert!(
            truncated.contains("[... OUTPUT TRUNCATED"),
            "should contain truncation marker"
        );
        // First 2000 + marker + last 2000
        assert!(truncated.starts_with("xx"));
        assert!(truncated.ends_with("xx"));
    }

    /// Test 3: Short output is NOT truncated
    #[tokio::test]
    async fn test_no_truncation_short_output() {
        let short = "hello world";
        let (result, was_truncated) = truncate_output(short);
        assert!(!was_truncated);
        assert_eq!(result, "hello world");
    }

    /// Test 4: Timeout override with sleep command
    #[tokio::test]
    async fn test_timeout_override() {
        let tool = make_tool();
        let result = tool
            .call(RunCommandArgs {
                command: "sleep 10".to_string(),
                timeout_override: Some(1),
            })
            .await;

        assert!(result.is_err(), "should timeout");
        match result.unwrap_err() {
            ToolError::Timeout(secs) => assert_eq!(secs, 1, "timeout should be 1 second"),
            other => panic!("expected Timeout, got: {other}"),
        }
    }

    /// Test 5: Long output via real command execution is truncated
    #[tokio::test]
    async fn test_long_command_output_truncated() {
        let tool = make_tool();
        // seq 1 1200 produces ~4893 chars of output (> 4000 threshold)
        let result = tool
            .call(RunCommandArgs {
                command: "seq 1 1200".to_string(),
                timeout_override: Some(5),
            })
            .await
            .unwrap();

        assert!(result.success, "seq command should succeed");
        assert!(result.truncated, "output should be truncated");
        assert!(
            result.stdout.contains("[... OUTPUT TRUNCATED"),
            "should contain truncation marker"
        );
    }
}
