use thiserror::Error;

#[derive(Error, Debug)]
pub enum SafetyError {
    #[error("Blocked: shell metacharacters detected in command: {0}")]
    ShellMetacharacters(String),

    #[error("Blocked: '{0}' could destroy the Pi's filesystem or shut it down")]
    DestructiveBinary(String),

    #[error("Blocked: empty command")]
    EmptyCommand,

    #[error("Blocked: invalid characters in target: {0}")]
    InvalidTarget(String),

    #[error("Blocked: wifi command targets protected interface '{0}' (C2 channel)")]
    ProtectedInterface(String),

    #[error("Blocked: {0}")]
    BlockedWifiCommand(String),
}
