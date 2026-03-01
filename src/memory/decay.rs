use crate::memory::{Connection, MemoryError};
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Spawn a background task that periodically decays salience of old memories
///
/// The task runs every 24 hours and:
/// - Applies 2% decay to memories older than 1 day
/// - Deletes memories with salience below 0.1
pub fn spawn_decay_task(conn: Arc<Connection>) {
    tokio::spawn(async move {
        // Run daily decay sweep
        let mut ticker = interval(Duration::from_secs(86400)); // 24 hours

        loop {
            ticker.tick().await;

            match run_decay_sweep(&conn).await {
                Ok(deleted_count) => {
                    println!("[INFO] Salience decay sweep completed: {} memories pruned", deleted_count);
                }
                Err(e) => {
                    println!("[ERROR] Salience decay sweep failed: {}", e);
                }
            }
        }
    });
}

/// Run a single decay sweep: apply decay to old memories and prune low-salience ones
///
/// Returns the count of memories deleted.
pub async fn run_decay_sweep(conn: &Connection) -> Result<usize, MemoryError> {
    let cutoff = chrono::Utc::now().timestamp() - 86400; // 1 day ago

    conn.call(move |conn| {
        // Decay memories older than 1 day by 2%
        conn.execute(
            "UPDATE memories SET salience = salience * 0.98 WHERE created_at < ?1",
            rusqlite::params![cutoff],
        )?;

        // Prune memories with salience below 0.1
        let deleted = conn.execute(
            "DELETE FROM memories WHERE salience < 0.1",
            [],
        )?;

        Ok(deleted)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_salience_decay() {
        // Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Insert test memories with different ages and salience values
        let now = chrono::Utc::now().timestamp();
        let two_days_ago = now - 172800; // 2 days ago
        let _yesterday = now - 86400; // 1 day ago

        conn.call(move |conn| {
            // Memory 1: old with high salience (should decay but not be deleted)
            conn.execute(
                "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "test-chat",
                    "Old memory with high salience",
                    "episodic",
                    0.5,
                    two_days_ago,
                    two_days_ago
                ],
            )?;

            // Memory 2: old with low salience (should decay and be deleted)
            conn.execute(
                "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "test-chat",
                    "Old memory with low salience",
                    "episodic",
                    0.11,
                    two_days_ago,
                    two_days_ago
                ],
            )?;

            // Memory 3: recent (should not decay)
            conn.execute(
                "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "test-chat",
                    "Recent memory",
                    "episodic",
                    1.0,
                    now,
                    now
                ],
            )?;

            Ok(())
        })
        .await
        .unwrap();

        // Run decay sweep
        let deleted = run_decay_sweep(&conn).await.unwrap();

        // Verify results
        let results: Vec<(i64, f64)> = conn
            .call(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, salience FROM memories ORDER BY id",
                )?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .unwrap();

        // Memory 1 should have decayed salience (0.5 * 0.98 = 0.49)
        assert!(results.len() >= 1);
        assert!((results[0].1 - 0.49).abs() < 0.01, "Memory 1 salience should be ~0.49");

        // Memory 2 should be deleted (0.11 * 0.98 = 0.1078, still above 0.1)
        // Actually this will decay to 0.1078 which is still above 0.1, so won't be deleted
        // Let me adjust test expectations

        // Memory 3 should be unchanged (1.0)
        let last = results.last().unwrap();
        assert!((last.1 - 1.0).abs() < 0.01, "Recent memory should be unchanged");

        // At least one memory should have been deleted eventually
        // (Memory 2 after enough decay cycles would go below 0.1)
        // For now, let's verify the sweep runs without error
        assert_eq!(deleted, 0, "No memories below 0.1 threshold yet");
    }

    #[tokio::test]
    async fn test_salience_decay_prunes_below_threshold() {
        // Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Insert memory that's already below threshold
        let two_days_ago = chrono::Utc::now().timestamp() - 172800;

        conn.call(move |conn| {
            conn.execute(
                "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "test-chat",
                    "Low salience memory",
                    "episodic",
                    0.05,  // Below 0.1 threshold
                    two_days_ago,
                    two_days_ago
                ],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        // Run decay sweep
        let deleted = run_decay_sweep(&conn).await.unwrap();

        // Verify memory was deleted
        assert_eq!(deleted, 1, "One memory should be deleted");

        let count: i64 = conn
            .call(|conn| {
                let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
                Ok(count)
            })
            .await
            .unwrap();

        assert_eq!(count, 0, "No memories should remain");
    }
}
