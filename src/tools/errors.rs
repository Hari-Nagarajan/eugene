use thiserror::Error;

/// Error types for tool execution that the agent can reason about
#[derive(Error, Debug)]
pub enum ToolError {
    /// Command exceeded its configured timeout
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),

    /// Insufficient permissions to execute the command
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// The requested binary/tool is not installed
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Network target is unreachable
    #[error("Target unreachable: {0}")]
    TargetUnreachable(String),

    /// General execution failure
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    /// Task dispatch to executor agent failed
    #[error("Dispatch failed: {0}")]
    DispatchFailed(String),

    /// Command blocked by safety validation
    #[error("Safety validation failed: {0}")]
    SafetyError(#[from] crate::safety::SafetyError),

    /// Memory/database operation failed
    #[error("Memory operation failed: {0}")]
    MemoryError(#[from] crate::memory::MemoryError),
}
