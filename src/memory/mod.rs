mod errors;
pub use errors::MemoryError;

mod queries;
pub use queries::{Memory, Finding, RunSummary, create_run, log_finding, log_task, update_task, update_run, get_findings_by_host, get_run_summary, save_memory, search_memories};
pub use queries::{ScoreSummary, ScoreEvent, Script, points_for_action, log_score_event, get_score_summary, weighted_vuln_points, log_weighted_vuln_event, save_script, search_scripts, get_script_by_name, update_script_usage};
pub use queries::{ScheduledTask, load_session, save_session, clear_session, create_schedule, list_schedules, delete_schedule, pause_schedule, resume_schedule, get_due_schedules, advance_schedule};
pub use queries::{get_cached_cves, store_cached_cves, delete_stale_cves};
pub use queries::{insert_wifi_ap, get_wifi_aps, insert_wifi_client, insert_client_probe, get_wifi_clients, get_matched_probes, migrate_wifi_schema, WifiClient, MatchedProbe, WifiCredential, insert_wifi_credential, get_wifi_credentials, update_wps_enabled};
pub use queries::{insert_llm_interaction, get_run_token_summary, RunTokenSummary};

mod decay;
pub use decay::{spawn_decay_task, run_decay_sweep};

use std::sync::Arc;
use tokio_rusqlite::Connection;

/// Open a memory store database connection with production pragmas
///
/// Configures the connection with:
/// - WAL mode for better concurrency
/// - NORMAL synchronous mode for performance
/// - 8MB memory-mapped I/O
/// - In-memory temp storage
/// - Foreign key constraints enabled
pub async fn open_memory_store(db_path: &str) -> Result<Arc<Connection>, MemoryError> {
    let conn = Connection::open(db_path)
        .await
        .map_err(|e| MemoryError::Connection(e.to_string()))?;

    // Apply production pragmas in single call
    conn.call(|conn| {
        // WAL mode for better concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // NORMAL sync: less paranoid than FULL, much faster
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // 8MB memory-mapped I/O
        conn.pragma_update(None, "mmap_size", 8388608)?;

        // Temp tables in memory
        conn.pragma_update(None, "temp_store", "MEMORY")?;

        // Enable foreign key constraints
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(())
    })
    .await?;

    Ok(Arc::new(conn))
}

/// Initialize database schema with FTS5 support check
///
/// Creates all tables from schema.sql if they don't exist.
/// Checks for FTS5 availability before creating FTS5 tables.
pub async fn init_schema(conn: &Connection) -> Result<(), MemoryError> {
    // Check if FTS5 is available by attempting to create a probe table
    let has_fts5 = conn
        .call(|conn| {
            match conn.execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS _fts5_probe USING fts5(x)",
                [],
            ) {
                Ok(_) => {
                    // Clean up probe table
                    conn.execute("DROP TABLE IF EXISTS _fts5_probe", [])?;
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        })
        .await?;

    if !has_fts5 {
        return Err(MemoryError::Fts5NotAvailable);
    }

    // Load and execute schema
    let schema = include_str!("schema.sql");

    conn.call(move |conn| {
        conn.execute_batch(schema)?;
        Ok(())
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> std::sync::Arc<tokio_rusqlite::Connection> {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        conn
    }

    #[tokio::test]
    async fn test_create_run() {
        let conn = setup().await;
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        assert!(run_id > 0, "create_run should return valid ID");
    }

    #[tokio::test]
    async fn test_log_finding() {
        let conn = setup().await;
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        let finding_id = log_finding(
            &conn,
            Some(run_id),
            Some("192.168.1.1".to_string()),
            "open_port".to_string(),
            "port 22 open".to_string(),
        )
        .await
        .unwrap();
        assert!(finding_id > 0, "log_finding should return valid ID");
    }

    #[tokio::test]
    async fn test_save_and_search_memory() {
        let conn = setup().await;
        let mem_id = save_memory(
            &conn,
            "test-chat".to_string(),
            "Found SSH service on target host".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();
        assert!(mem_id > 0, "save_memory should return valid ID");

        let (salience, sector): (f64, String) = conn
            .call(move |conn| {
                let result = conn.query_row(
                    "SELECT salience, sector FROM memories WHERE id = ?1",
                    rusqlite::params![mem_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                Ok(result)
            })
            .await
            .unwrap();
        assert_eq!(salience, 1.0, "New memory should have full salience");
        assert_eq!(sector, "episodic");

        let results = search_memories(&conn, "test-chat".to_string(), "SSH service".to_string(), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "Should find the SSH memory");
        assert_eq!(results[0].id, mem_id);
    }

    #[tokio::test]
    async fn test_fts5_trigger_creates_entry() {
        let conn = setup().await;
        let mem_id = save_memory(
            &conn,
            "test-chat".to_string(),
            "FTS5 trigger test content".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        let fts_count: i64 = conn
            .call(move |conn| {
                let count = conn.query_row(
                    "SELECT COUNT(*) FROM memories_fts WHERE rowid = ?1",
                    rusqlite::params![mem_id],
                    |row| row.get(0),
                )?;
                Ok(count)
            })
            .await
            .unwrap();
        assert_eq!(fts_count, 1, "FTS5 trigger should create entry");
    }

    #[tokio::test]
    async fn test_decay_ignores_fresh_memories() {
        let conn = setup().await;
        let mem_id = save_memory(
            &conn,
            "test-chat".to_string(),
            "Fresh memory content".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        let deleted = run_decay_sweep(&conn).await.unwrap();
        assert_eq!(deleted, 0, "Fresh memories should not be pruned");

        let salience_after: f64 = conn
            .call(move |conn| {
                let salience = conn.query_row(
                    "SELECT salience FROM memories WHERE id = ?1",
                    rusqlite::params![mem_id],
                    |row| row.get(0),
                )?;
                Ok(salience)
            })
            .await
            .unwrap();
        assert_eq!(
            salience_after, 1.0,
            "Fresh memory (< 1 day old) should not decay"
        );
    }

    #[tokio::test]
    async fn test_fts5_special_chars_dont_crash() {
        let conn = setup().await;
        save_memory(
            &conn,
            "test-chat".to_string(),
            "Found SSH service on target host".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        // Search with special chars (should sanitize, not crash)
        let _results = search_memories(
            &conn,
            "test-chat".to_string(),
            "host:192.168".to_string(), // Contains FTS5 special char ":"
            10,
        )
        .await
        .unwrap();
        // Should return results or empty, but not crash
    }

    #[tokio::test]
    async fn test_llm_interactions_table_exists() {
        let conn = setup().await;
        conn.call(|conn| {
            // Insert a minimal row to verify table exists with expected columns
            conn.execute(
                "INSERT INTO llm_interactions (request_id, status, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params!["test-req-id", "success", "2026-01-01T00:00:00Z"],
            )?;
            // Verify we can read it back
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM llm_interactions",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);

            // Verify all expected columns exist by inserting a full row
            conn.execute(
                "INSERT INTO llm_interactions (run_id, request_id, provider, model, caller_context, prompt_text, response_text, input_tokens, output_tokens, total_tokens, latency_ms, status, error_message, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![
                    rusqlite::types::Null, "full-req-id", "openai", "gpt-4", "test_context",
                    "prompt", "response", 10, 20, 30, 150, "success", rusqlite::types::Null, "2026-01-01T00:00:00Z"
                ],
            )?;
            let count2: i64 = conn.query_row(
                "SELECT COUNT(*) FROM llm_interactions",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count2, 2);

            // Verify CHECK constraint on status rejects invalid values
            let bad = conn.execute(
                "INSERT INTO llm_interactions (request_id, status, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params!["bad-req", "invalid_status", "2026-01-01T00:00:00Z"],
            );
            assert!(bad.is_err(), "CHECK constraint should reject invalid status");

            // Verify indexes exist
            let idx_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_llm_interactions_%'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(idx_count, 3, "Should have 3 indexes on llm_interactions");

            Ok(())
        })
        .await
        .unwrap();
    }

    #[test]
    fn test_safety_validation() {
        use crate::safety::{validate_command, sanitize_target};

        // Should block destructive commands
        assert!(validate_command("rm -rf /", None).is_err());
        assert!(validate_command("dd if=/dev/zero of=/dev/sda", None).is_err());
        assert!(validate_command("shutdown -h now", None).is_err());

        // Should allow offensive tools
        assert!(validate_command("nmap -sS 192.168.1.1", None).is_ok());
        assert!(validate_command("hydra -l admin -P pass.txt ssh://target", None).is_ok());

        // Should block shell metacharacters
        assert!(validate_command("cat /etc/passwd; rm -rf /", None).is_err());

        // Should validate targets
        assert!(sanitize_target("192.168.1.1").is_ok());
        assert!(sanitize_target("10.0.0.0/24").is_ok());
        assert!(sanitize_target("example.com").is_ok());
        assert!(sanitize_target("; rm -rf /").is_err());
    }
}
