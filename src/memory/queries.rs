use tokio_rusqlite::Connection;
use crate::memory::MemoryError;
use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

/// FTS5 query sanitization regex - strips non-alphanumeric/non-space characters
static FTS_SANITIZER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^\w\s]").unwrap()
});

/// Memory record structure
#[derive(Debug)]
pub struct Memory {
    pub id: i64,
    pub chat_id: String,
    pub topic_key: Option<String>,
    pub content: String,
    pub sector: String,
    pub salience: f64,
    pub created_at: i64,
    pub accessed_at: i64,
}

/// Finding record structure
#[derive(Debug)]
pub struct Finding {
    pub id: i64,
    pub run_id: Option<i64>,
    pub host: Option<String>,
    pub finding_type: String,
    pub data: String,
    pub timestamp: String,
}

/// Create a new run record and return its ID
pub async fn create_run(
    conn: &Connection,
    trigger_type: String,
    trigger_data: Option<String>,
) -> Result<i64, MemoryError> {
    let started_at = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO runs (trigger_type, trigger_data, status, started_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![trigger_type, trigger_data, "running", started_at],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Log a finding and return its ID
pub async fn log_finding(
    conn: &Connection,
    run_id: Option<i64>,
    host: Option<String>,
    finding_type: String,
    data: String,
) -> Result<i64, MemoryError> {
    let timestamp = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO findings (run_id, host, finding_type, data, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![run_id, host, finding_type, data, timestamp],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Save a memory and return its ID
pub async fn save_memory(
    conn: &Connection,
    chat_id: String,
    content: String,
    sector: String,
) -> Result<i64, MemoryError> {
    // Validate sector
    if sector != "semantic" && sector != "episodic" {
        return Err(MemoryError::Query(format!(
            "Invalid sector '{}', must be 'semantic' or 'episodic'",
            sector
        )));
    }

    let now = Utc::now().timestamp();
    let salience = 1.0;

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![chat_id, content, sector, salience, now, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Search memories using FTS5 with query sanitization and salience boosting
pub async fn search_memories(
    conn: &Connection,
    chat_id: String,
    query: String,
    limit: i64,
) -> Result<Vec<Memory>, MemoryError> {
    // Sanitize query: remove FTS5 special chars
    let safe_query = FTS_SANITIZER.replace_all(&query, " ");
    let words: Vec<&str> = safe_query.split_whitespace().collect();

    if words.is_empty() {
        return Ok(Vec::new());
    }

    // Build FTS5 query: "word1* OR word2* OR word3*" for prefix matching
    let fts_query = words
        .iter()
        .map(|w| format!("{}*", w))
        .collect::<Vec<_>>()
        .join(" OR ");

    let now = Utc::now().timestamp();

    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT m.* FROM memories m
             JOIN memories_fts f ON m.id = f.rowid
             WHERE f.memories_fts MATCH ?1 AND m.chat_id = ?2
             ORDER BY m.salience DESC
             LIMIT ?3"
        )?;

        let memories = stmt.query_map(
            rusqlite::params![fts_query, chat_id, limit],
            |row| {
                Ok(Memory {
                    id: row.get(0)?,
                    chat_id: row.get(1)?,
                    topic_key: row.get(2)?,
                    content: row.get(3)?,
                    sector: row.get(4)?,
                    salience: row.get(5)?,
                    created_at: row.get(6)?,
                    accessed_at: row.get(7)?,
                })
            }
        )?
        .collect::<Result<Vec<_>, _>>()?;

        // Reinforce accessed memories: salience boost (capped at 5.0) and update accessed_at
        if !memories.is_empty() {
            let ids: Vec<i64> = memories.iter().map(|m| m.id).collect();
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let update_query = format!(
                "UPDATE memories SET accessed_at = ?1, salience = MIN(salience + 0.1, 5.0) WHERE id IN ({})",
                placeholders
            );

            let mut params = vec![now];
            params.extend(ids.iter());

            conn.execute(&update_query, rusqlite::params_from_iter(params))?;
        }

        Ok(memories)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_crud_operations() {
        // Setup: Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&*conn).await.unwrap();

        // Test create_run
        let run_id = create_run(
            &*conn,
            "manual".to_string(),
            Some("test trigger".to_string()),
        )
        .await
        .unwrap();
        assert!(run_id > 0, "create_run should return valid ID");

        // Verify run was inserted
        let run_status: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status FROM runs WHERE id = ?1",
                    rusqlite::params![run_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(run_status, "running");

        // Test log_finding
        let finding_id = log_finding(
            &*conn,
            Some(run_id),
            Some("192.168.1.1".to_string()),
            "open_port".to_string(),
            "port 22 open".to_string(),
        )
        .await
        .unwrap();
        assert!(finding_id > 0, "log_finding should return valid ID");

        // Verify finding was inserted
        let finding_type: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT finding_type FROM findings WHERE id = ?1",
                    rusqlite::params![finding_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(finding_type, "open_port");

        // Test save_memory
        let memory_id = save_memory(
            &*conn,
            "test_chat".to_string(),
            "Test memory content".to_string(),
            "semantic".to_string(),
        )
        .await
        .unwrap();
        assert!(memory_id > 0, "save_memory should return valid ID");

        // Verify memory was inserted with correct defaults
        let (salience, sector): (f64, String) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT salience, sector FROM memories WHERE id = ?1",
                    rusqlite::params![memory_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(salience, 1.0);
        assert_eq!(sector, "semantic");

        // Verify FTS5 trigger created entry
        let fts_count: i64 = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM memories_fts WHERE rowid = ?1",
                    rusqlite::params![memory_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(fts_count, 1, "FTS5 trigger should create entry");

        // Test invalid sector
        let result = save_memory(
            &*conn,
            "test_chat".to_string(),
            "content".to_string(),
            "invalid".to_string(),
        )
        .await;
        assert!(result.is_err(), "save_memory should reject invalid sector");
    }

    #[tokio::test]
    async fn test_fts5_search() {
        // Setup: Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&*conn).await.unwrap();

        // Insert test memories
        save_memory(
            &*conn,
            "test_chat".to_string(),
            "Network reconnaissance using nmap".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &*conn,
            "test_chat".to_string(),
            "Found open port 22 on target host".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &*conn,
            "test_chat".to_string(),
            "SQL injection vulnerability detected".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &*conn,
            "other_chat".to_string(),
            "Network reconnaissance data".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        // Test 1: Basic search with single word
        let results = search_memories(
            &*conn,
            "test_chat".to_string(),
            "network".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1, "Should find 1 memory with 'network'");
        assert!(results[0].content.contains("reconnaissance"));

        // Test 2: Search with multiple words (OR query)
        let results = search_memories(
            &*conn,
            "test_chat".to_string(),
            "port injection".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 2, "Should find 2 memories with 'port' OR 'injection'");

        // Test 3: Search with special characters (sanitization test)
        let results = search_memories(
            &*conn,
            "test_chat".to_string(),
            "host:192.168".to_string(), // Contains ':' which would break FTS5
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1, "Should sanitize and find 'host'");

        // Test 4: Empty query returns empty results
        let results = search_memories(
            &*conn,
            "test_chat".to_string(),
            "".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 0, "Empty query should return no results");

        // Test 5: Chat ID filtering
        let results = search_memories(
            &*conn,
            "other_chat".to_string(),
            "reconnaissance".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1, "Should only return memories for other_chat");

        // Test 6: Salience boosting
        let first_salience = results[0].salience;

        // Search again for the same memory
        let results = search_memories(
            &*conn,
            "other_chat".to_string(),
            "reconnaissance".to_string(),
            10,
        )
        .await
        .unwrap();

        assert!(results[0].salience > first_salience, "Salience should increase on access");
        assert!(results[0].salience <= 5.0, "Salience should be capped at 5.0");

        // Test 7: Limit enforcement
        let results = search_memories(
            &*conn,
            "test_chat".to_string(),
            "test".to_string(),
            1,
        )
        .await
        .unwrap();
        assert!(results.len() <= 1, "Should respect limit parameter");
    }
}
