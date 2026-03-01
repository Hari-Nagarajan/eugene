mod errors;
pub use errors::MemoryError;

mod queries;
pub use queries::{Memory, Finding, create_run, log_finding, save_memory, search_memories};

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

    #[tokio::test]
    async fn test_phase1_integration() {
        // Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Test run creation
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        assert!(run_id > 0, "create_run should return valid ID");

        // Test finding logging
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

        // Test memory storage
        let mem_id = save_memory(
            &conn,
            "test-chat".to_string(),
            "Found SSH service on target host".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();
        assert!(mem_id > 0, "save_memory should return valid ID");

        // Verify memory was stored with correct defaults
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

        // Verify FTS5 trigger created entry
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

        // Test salience decay
        let deleted = run_decay_sweep(&conn).await.unwrap();
        // Fresh memory shouldn't be deleted yet (created just now)
        assert_eq!(deleted, 0, "Fresh memories should not be pruned");

        // Verify memory still exists with unchanged salience
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

        // Test FTS5 search with special chars doesn't crash
        let results = search_memories(&conn, "test-chat".to_string(), "SSH service".to_string(), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1, "Should find the SSH memory");
        assert_eq!(results[0].id, mem_id);

        // Test search with special chars (should sanitize, not crash)
        let _results2 = search_memories(
            &conn,
            "test-chat".to_string(),
            "host:192.168".to_string(), // Contains FTS5 special char ":"
            10,
        )
        .await
        .unwrap();
        // Should return results or empty, but not crash
    }

    #[test]
    fn test_safety_validation() {
        use crate::safety::{validate_command, sanitize_target};

        // Should block destructive commands
        assert!(validate_command("rm -rf /").is_err());
        assert!(validate_command("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(validate_command("shutdown -h now").is_err());

        // Should allow offensive tools
        assert!(validate_command("nmap -sS 192.168.1.1").is_ok());
        assert!(validate_command("hydra -l admin -P pass.txt ssh://target").is_ok());

        // Should block shell metacharacters
        assert!(validate_command("cat /etc/passwd; rm -rf /").is_err());

        // Should validate targets
        assert!(sanitize_target("192.168.1.1").is_ok());
        assert!(sanitize_target("10.0.0.0/24").is_ok());
        assert!(sanitize_target("example.com").is_ok());
        assert!(sanitize_target("; rm -rf /").is_err());
    }
}
