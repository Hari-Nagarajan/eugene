use crate::tools::ToolError;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Async command executor using tokio::process
///
/// Executes CLI commands with configurable timeouts and pre-execution
/// safety validation. Commands run in /tmp to match the entropy-goblin
/// execution model.
pub struct LocalExecutor;

impl LocalExecutor {
    /// Execute a command with timeout and safety validation
    ///
    /// 1. Parses command into binary + args
    /// 2. Validates via safety::validate_command()
    /// 3. Spawns with tokio::process::Command
    /// 4. Wraps in timeout
    /// 5. Classifies errors into ToolError variants
    pub async fn execute(
        &self,
        command: &str,
        timeout_secs: u64,
    ) -> Result<String, ToolError> {
        // Validate command through safety layer before any execution
        crate::safety::validate_command(command)?;

        // Parse command into parts
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ToolError::ExecutionFailed("empty command".to_string()));
        }

        let binary = parts[0];
        let args = &parts[1..];

        // Build tokio::process::Command
        let mut cmd = Command::new(binary);
        cmd.args(args)
            .current_dir("/tmp")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Spawn process, classify spawn errors
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return Err(match e.kind() {
                    std::io::ErrorKind::NotFound => ToolError::ToolNotFound(binary.to_string()),
                    std::io::ErrorKind::PermissionDenied => {
                        ToolError::PermissionDenied(binary.to_string())
                    }
                    _ => ToolError::ExecutionFailed(format!("spawn failed: {e}")),
                });
            }
        };

        // Wait for output with timeout
        let output = match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(ToolError::ExecutionFailed(format!("execution error: {e}")));
            }
            Err(_) => {
                return Err(ToolError::Timeout(timeout_secs));
            }
        };

        // Check exit status
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Classify network errors
            if stderr.contains("Network is unreachable") || stderr.contains("No route to host") {
                return Err(ToolError::TargetUnreachable(stderr));
            }

            return Err(ToolError::ExecutionFailed(stderr));
        }

        // Return stdout
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_execution() {
        let executor = LocalExecutor;
        let result = executor.execute("echo hello", 5).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }
}
