//! Systemd user service file generator for `eugene service` command.
//!
//! Generates a systemd user service file at `~/.config/systemd/user/eugene.service`
//! that runs `eugene bot` as a long-running service with auto-restart.

use std::path::PathBuf;

/// Generate systemd service file content without writing to disk.
///
/// Useful for testing. Returns the service file content as a string.
pub fn generate_service_content() -> Result<String, anyhow::Error> {
    let binary_path = std::env::current_exe()?;
    let binary_path_str = binary_path.display();

    let db_path = std::env::var("EUGENE_DB_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/eugene.db")
    });

    let content = format!(
        r#"[Unit]
Description=Eugene Autonomous Recon Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={binary_path_str} bot
Restart=on-failure
RestartSec=10
Environment=EUGENE_DB_PATH={db_path}
# Add your secrets in an override file:
# systemctl --user edit eugene
# [Service]
# Environment=TELEGRAM_BOT_TOKEN=your_token
# Environment=MINIMAX_API_KEY=your_key
# Environment=ALLOWED_CHAT_IDS=123456789

[Install]
WantedBy=default.target
"#
    );

    Ok(content)
}

/// Generate and write systemd user service file, then print instructions.
///
/// Writes to `~/.config/systemd/user/eugene.service` and prints
/// enable/start instructions for the user.
pub fn generate_service() -> Result<(), anyhow::Error> {
    let content = generate_service_content()?;

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let service_dir = PathBuf::from(&home).join(".config/systemd/user");
    std::fs::create_dir_all(&service_dir)?;

    let service_path = service_dir.join("eugene.service");
    std::fs::write(&service_path, &content)?;

    println!("Service file written to {}", service_path.display());
    println!();
    println!("To install and start:");
    println!("  systemctl --user daemon-reload");
    println!("  systemctl --user enable eugene");
    println!("  systemctl --user start eugene");
    println!();
    println!("To configure secrets:");
    println!("  systemctl --user edit eugene");
    println!("  # Add Environment= lines for TELEGRAM_BOT_TOKEN, MINIMAX_API_KEY, ALLOWED_CHAT_IDS");
    println!();
    println!("To check status:");
    println!("  systemctl --user status eugene");
    println!("  journalctl --user -u eugene -f");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_service_content_has_unit_section() {
        let content = generate_service_content().unwrap();
        assert!(content.contains("[Unit]"));
        assert!(content.contains("Description=Eugene Autonomous Recon Agent"));
    }

    #[test]
    fn test_generate_service_content_has_service_section() {
        let content = generate_service_content().unwrap();
        assert!(content.contains("[Service]"));
        assert!(content.contains("ExecStart="));
        assert!(content.contains("bot"));
        assert!(content.contains("Restart=on-failure"));
        assert!(content.contains("RestartSec=10"));
    }

    #[test]
    fn test_generate_service_content_has_install_section() {
        let content = generate_service_content().unwrap();
        assert!(content.contains("[Install]"));
        assert!(content.contains("WantedBy=default.target"));
    }

    #[test]
    fn test_generate_service_content_has_db_path() {
        let content = generate_service_content().unwrap();
        assert!(content.contains("EUGENE_DB_PATH="));
    }
}
