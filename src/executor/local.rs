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

    /// Test 1: Execute safe command returns Ok with stdout
    #[tokio::test]
    async fn test_execute_safe_command() {
        let executor = LocalExecutor;
        let result = executor.execute("echo hello", 5).await;
        assert!(result.is_ok(), "safe command should succeed");
        let output = result.unwrap();
        assert!(output.contains("hello"), "stdout should contain 'hello'");
    }

    /// Test 2: Command exceeding timeout returns ToolError::Timeout
    #[tokio::test]
    async fn test_execute_timeout() {
        let executor = LocalExecutor;
        let result = executor.execute("sleep 5", 1).await;
        assert!(result.is_err(), "timed-out command should fail");
        match result.unwrap_err() {
            ToolError::Timeout(secs) => assert_eq!(secs, 1, "timeout should report 1 second"),
            other => panic!("expected Timeout, got: {other}"),
        }
    }

    /// Test 3: Nonexistent binary returns ToolError::ToolNotFound
    #[tokio::test]
    async fn test_execute_tool_not_found() {
        let executor = LocalExecutor;
        let result = executor.execute("fakebinary123", 5).await;
        assert!(result.is_err(), "nonexistent binary should fail");
        match result.unwrap_err() {
            ToolError::ToolNotFound(name) => {
                assert_eq!(name, "fakebinary123", "should report the binary name");
            }
            other => panic!("expected ToolNotFound, got: {other}"),
        }
    }

    /// Test 4: Destructive command blocked by safety layer
    #[tokio::test]
    async fn test_execute_blocked_by_safety() {
        let executor = LocalExecutor;
        let result = executor.execute("rm -rf /tmp", 5).await;
        assert!(result.is_err(), "destructive command should be blocked");
        match result.unwrap_err() {
            ToolError::SafetyError(_) => {} // expected
            other => panic!("expected SafetyError, got: {other}"),
        }
    }

    /// Test 5: Shell metacharacters blocked before process spawns
    #[tokio::test]
    async fn test_execute_shell_metachar_blocked() {
        let executor = LocalExecutor;
        let result = executor.execute("echo test; ls", 5).await;
        assert!(result.is_err(), "shell metachar should be blocked");
        match result.unwrap_err() {
            ToolError::SafetyError(_) => {} // expected
            other => panic!("expected SafetyError, got: {other}"),
        }
    }
}
