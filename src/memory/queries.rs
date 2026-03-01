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

/// Run summary statistics
#[derive(Debug, serde::Serialize)]
pub struct RunSummary {
    pub task_count: i64,
    pub finding_count: i64,
    pub completed_task_count: i64,
    pub failed_task_count: i64,
    // Phase 5 scoring fields
    pub total_score: i64,
    pub detection_count: i64,
    pub last_score_event: Option<String>,
}

/// Score event record
#[derive(Debug, serde::Serialize)]
pub struct ScoreEvent {
    pub action: String,
    pub points: i64,
    pub risk_level: String,
    pub detected: bool,
    pub timestamp: String,
}

/// Score summary for a run
#[derive(Debug, serde::Serialize)]
pub struct ScoreSummary {
    pub total_score: i64,
    pub detection_count: i64,
    pub recent_events: Vec<ScoreEvent>,
}

/// Script record from scripts table
#[derive(Debug, serde::Serialize)]
pub struct Script {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub language: String,
    pub tags: String,
    pub code: String,
    pub use_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
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

/// Log a task to the tasks table and return its ID
pub async fn log_task(
    conn: &Connection,
    run_id: i64,
    name: &str,
    description: &str,
) -> Result<i64, MemoryError> {
    let name = name.to_string();
    let description = description.to_string();
    let created_at = Utc::now().to_rfc3339();

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO tasks (run_id, name, description, status, created_at) VALUES (?1, ?2, ?3, 'running', ?4)",
            rusqlite::params![run_id, name, description, created_at],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Update task status and result
pub async fn update_task(
    conn: &Connection,
    task_id: i64,
    status: &str,
    result: &str,
) -> Result<(), MemoryError> {
    let status = status.to_string();
    let result = result[..result.len().min(2000)].to_string();
    let completed_at = Utc::now().to_rfc3339();

    let err_result = conn.call(move |conn| {
        conn.execute(
            "UPDATE tasks SET status = ?1, result = ?2, completed_at = ?3 WHERE id = ?4",
            rusqlite::params![status, result, completed_at, task_id],
        )?;
        Ok(())
    })
    .await;
    err_result.map_err(MemoryError::from)
}

/// Update run status and set completed_at
pub async fn update_run(
    conn: &Connection,
    run_id: i64,
    status: &str,
) -> Result<(), MemoryError> {
    let status = status.to_string();
    let completed_at = Utc::now().to_rfc3339();

    let err_result = conn.call(move |conn| {
        conn.execute(
            "UPDATE runs SET status = ?1, completed_at = ?2 WHERE id = ?3",
            rusqlite::params![status, completed_at, run_id],
        )?;
        Ok(())
    })
    .await;
    err_result.map_err(MemoryError::from)
}

/// Get all findings for a specific host, ordered by timestamp
pub async fn get_findings_by_host(
    conn: &Connection,
    host: String,
) -> Result<Vec<Finding>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, run_id, host, finding_type, data, timestamp FROM findings WHERE host = ?1 ORDER BY timestamp"
        )?;
        let findings = stmt.query_map(rusqlite::params![host], |row| {
            Ok(Finding {
                id: row.get(0)?,
                run_id: row.get(1)?,
                host: row.get(2)?,
                finding_type: row.get(3)?,
                data: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(findings)
    })
    .await
    .map_err(MemoryError::from)
}

/// Get a summary of tasks and findings for a run
pub async fn get_run_summary(
    conn: &Connection,
    run_id: i64,
) -> Result<RunSummary, MemoryError> {
    conn.call(move |conn| {
        let (task_count, completed_task_count, failed_task_count): (i64, i64, i64) = conn.query_row(
            "SELECT \
                COUNT(*), \
                COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0), \
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) \
             FROM tasks WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let finding_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM findings WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| row.get(0),
        )?;

        // Stub score fields (RED phase - will be implemented in GREEN)
        Ok(RunSummary {
            task_count,
            finding_count,
            completed_task_count,
            failed_task_count,
            total_score: 0,
            detection_count: 0,
            last_score_event: None,
        })
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

/// Look up point value for a scoring action
pub fn points_for_action(action: &str) -> Option<i64> {
    // STUB: returns None for all actions (RED phase)
    let _ = action;
    None
}

/// Log a score event and return its ID
pub async fn log_score_event(
    conn: &Connection,
    run_id: Option<i64>,
    action: String,
    risk_level: String,
    detected: bool,
) -> Result<i64, MemoryError> {
    // STUB: always returns error (RED phase)
    let _ = (conn, run_id, action, risk_level, detected);
    Err(MemoryError::Query("not implemented".to_string()))
}

/// Get score summary for a run
pub async fn get_score_summary(
    conn: &Connection,
    run_id: i64,
) -> Result<ScoreSummary, MemoryError> {
    // STUB: returns empty summary (RED phase)
    let _ = (conn, run_id);
    Ok(ScoreSummary {
        total_score: -999,
        detection_count: -999,
        recent_events: vec![],
    })
}

/// Save a script (insert or upsert on name conflict)
pub async fn save_script(
    conn: &Connection,
    name: String,
    description: String,
    language: String,
    tags: String,
    code: String,
) -> Result<i64, MemoryError> {
    // STUB: always returns error (RED phase)
    let _ = (conn, name, description, language, tags, code);
    Err(MemoryError::Query("not implemented".to_string()))
}

/// Search scripts using FTS5
pub async fn search_scripts(
    conn: &Connection,
    query: String,
    limit: i64,
) -> Result<Vec<Script>, MemoryError> {
    // STUB: always returns error (RED phase)
    let _ = (conn, query, limit);
    Err(MemoryError::Query("not implemented".to_string()))
}

/// Get a script by its unique name
pub async fn get_script_by_name(
    conn: &Connection,
    name: String,
) -> Result<Option<Script>, MemoryError> {
    // STUB: always returns error (RED phase)
    let _ = (conn, name);
    Err(MemoryError::Query("not implemented".to_string()))
}

/// Increment script use_count and set last_run_at
pub async fn update_script_usage(
    conn: &Connection,
    script_id: i64,
) -> Result<(), MemoryError> {
    // STUB: always returns error (RED phase)
    let _ = (conn, script_id);
    Err(MemoryError::Query("not implemented".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_crud_operations() {
        // Setup: Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Test create_run
        let run_id = create_run(
            &conn,
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
            &conn,
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
            &conn,
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
            &conn,
            "test_chat".to_string(),
            "content".to_string(),
            "invalid".to_string(),
        )
        .await;
        assert!(result.is_err(), "save_memory should reject invalid sector");
    }

    #[tokio::test]
    async fn test_log_task_returns_valid_id() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let task_id = log_task(&conn, run_id, "arp_sweep", "Sweep the local subnet").await.unwrap();
        assert!(task_id > 0, "log_task should return valid ID");

        // Verify the task was inserted with status='running'
        let status: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(status, "running");
    }

    #[tokio::test]
    async fn test_update_task_sets_status_and_result() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        let task_id = log_task(&conn, run_id, "scan", "scan it").await.unwrap();

        update_task(&conn, task_id, "completed", "result text").await.unwrap();

        let (status, result, completed_at): (String, String, Option<String>) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status, result, completed_at FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(status, "completed");
        assert_eq!(result, "result text");
        assert!(completed_at.is_some(), "completed_at should be set");
    }

    #[tokio::test]
    async fn test_update_task_truncates_result_to_2000_chars() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();
        let task_id = log_task(&conn, run_id, "scan", "scan it").await.unwrap();

        let long_result = "x".repeat(5000);
        update_task(&conn, task_id, "completed", &long_result).await.unwrap();

        let result: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT result FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(result.len(), 2000, "result should be truncated to 2000 chars");
    }

    #[tokio::test]
    async fn test_update_run_sets_status_and_completed_at() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        update_run(&conn, run_id, "completed").await.unwrap();

        let (status, completed_at): (String, Option<String>) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status, completed_at FROM runs WHERE id = ?1",
                    rusqlite::params![run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(status, "completed");
        assert!(completed_at.is_some(), "completed_at should be set");
    }

    #[tokio::test]
    async fn test_get_findings_by_host_returns_ordered_findings() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        log_finding(&conn, Some(run_id), Some("192.168.1.1".to_string()), "port_scan".to_string(), "port 22 open".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("192.168.1.1".to_string()), "service".to_string(), "SSH on 22".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("10.0.0.1".to_string()), "port_scan".to_string(), "port 80 open".to_string()).await.unwrap();

        let findings = get_findings_by_host(&conn, "192.168.1.1".to_string()).await.unwrap();
        assert_eq!(findings.len(), 2, "Should find 2 findings for 192.168.1.1");
        assert_eq!(findings[0].finding_type, "port_scan");
        assert_eq!(findings[1].finding_type, "service");
    }

    #[tokio::test]
    async fn test_get_findings_by_host_returns_empty_for_unknown() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let findings = get_findings_by_host(&conn, "unknown.host".to_string()).await.unwrap();
        assert!(findings.is_empty(), "Should return empty vec for unknown host");
    }

    #[tokio::test]
    async fn test_get_run_summary_counts() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        // Create 3 tasks
        let t1 = log_task(&conn, run_id, "task1", "desc1").await.unwrap();
        let t2 = log_task(&conn, run_id, "task2", "desc2").await.unwrap();
        let t3 = log_task(&conn, run_id, "task3", "desc3").await.unwrap();

        // Complete 2, fail 1
        update_task(&conn, t1, "completed", "ok").await.unwrap();
        update_task(&conn, t2, "completed", "ok").await.unwrap();
        update_task(&conn, t3, "failed", "err").await.unwrap();

        // Log 2 findings
        log_finding(&conn, Some(run_id), Some("host".to_string()), "port".to_string(), "data".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("host".to_string()), "svc".to_string(), "data".to_string()).await.unwrap();

        let summary = get_run_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.task_count, 3);
        assert_eq!(summary.finding_count, 2);
        assert_eq!(summary.completed_task_count, 2);
        assert_eq!(summary.failed_task_count, 1);
    }

    #[tokio::test]
    async fn test_fts5_search() {
        // Setup: Create in-memory database
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Insert test memories
        save_memory(
            &conn,
            "test_chat".to_string(),
            "Network reconnaissance using nmap".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &conn,
            "test_chat".to_string(),
            "Found open port 22 on target host".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &conn,
            "test_chat".to_string(),
            "SQL injection vulnerability detected".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        save_memory(
            &conn,
            "other_chat".to_string(),
            "Network reconnaissance data".to_string(),
            "episodic".to_string(),
        )
        .await
        .unwrap();

        // Test 1: Basic search with single word
        let results = search_memories(
            &conn,
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
            &conn,
            "test_chat".to_string(),
            "port injection".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 2, "Should find 2 memories with 'port' OR 'injection'");

        // Test 3: Search with special characters (sanitization test)
        let results = search_memories(
            &conn,
            "test_chat".to_string(),
            "host:192.168".to_string(), // Contains ':' which would break FTS5
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1, "Should sanitize and find 'host'");

        // Test 4: Empty query returns empty results
        let results = search_memories(
            &conn,
            "test_chat".to_string(),
            "".to_string(),
            10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 0, "Empty query should return no results");

        // Test 5: Chat ID filtering
        let results = search_memories(
            &conn,
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
            &conn,
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
            &conn,
            "test_chat".to_string(),
            "test".to_string(),
            1,
        )
        .await
        .unwrap();
        assert!(results.len() <= 1, "Should respect limit parameter");
    }

    // ========== Phase 5: Score & Script Tests ==========

    #[test]
    fn test_points_for_action_known_actions() {
        assert_eq!(points_for_action("host_discovered"), Some(10));
        assert_eq!(points_for_action("port_found"), Some(5));
        assert_eq!(points_for_action("service_identified"), Some(15));
        assert_eq!(points_for_action("os_fingerprinted"), Some(20));
        assert_eq!(points_for_action("vuln_detected"), Some(25));
        assert_eq!(points_for_action("credential_captured"), Some(50));
        assert_eq!(points_for_action("successful_login"), Some(75));
        assert_eq!(points_for_action("privilege_escalation"), Some(150));
        assert_eq!(points_for_action("rce_achieved"), Some(200));
        assert_eq!(points_for_action("data_exfiltrated"), Some(100));
        assert_eq!(points_for_action("detection"), Some(-100));
    }

    #[test]
    fn test_points_for_action_unknown_returns_none() {
        assert_eq!(points_for_action("bogus_action"), None);
        assert_eq!(points_for_action(""), None);
        assert_eq!(points_for_action("HOST_DISCOVERED"), None); // case sensitive
    }

    #[tokio::test]
    async fn test_log_score_event_inserts_and_returns_id() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let event_id = log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        assert!(event_id > 0, "log_score_event should return valid ID");

        // Verify the correct points were stored
        let points: i64 = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT points FROM score_events WHERE id = ?1",
                    rusqlite::params![event_id],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(points, 10, "host_discovered should have 10 points");
    }

    #[tokio::test]
    async fn test_log_score_event_rejects_unknown_action() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let result = log_score_event(&conn, None, "bogus_action".to_string(), "low".to_string(), false).await;
        assert!(result.is_err(), "Should reject unknown action");
    }

    #[tokio::test]
    async fn test_get_score_summary_empty_run() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let summary = get_score_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_score, 0, "Empty run should have total_score=0");
        assert_eq!(summary.detection_count, 0, "Empty run should have detection_count=0");
        assert!(summary.recent_events.is_empty(), "Empty run should have no events");
    }

    #[tokio::test]
    async fn test_get_score_summary_with_events() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "port_found".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "detection".to_string(), "high".to_string(), true).await.unwrap();

        let summary = get_score_summary(&conn, run_id).await.unwrap();
        // 10 + 5 + (-100) = -85
        assert_eq!(summary.total_score, -85, "Total score should be 10+5-100=-85");
        assert_eq!(summary.detection_count, 1, "Should count 1 detection");
        assert_eq!(summary.recent_events.len(), 3, "Should have 3 recent events");
    }

    #[tokio::test]
    async fn test_save_script_insert_and_upsert() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = save_script(
            &conn,
            "sweep.sh".to_string(),
            "ARP sweep script".to_string(),
            "bash".to_string(),
            "[\"network\"]".to_string(),
            "arp-scan --localnet".to_string(),
        ).await.unwrap();
        assert!(id > 0, "save_script should return valid ID");

        // Upsert: same name, different code -- should not error
        let id2 = save_script(
            &conn,
            "sweep.sh".to_string(),
            "Updated ARP sweep".to_string(),
            "bash".to_string(),
            "[\"network\"]".to_string(),
            "arp-scan -I eth0 --localnet".to_string(),
        ).await.unwrap();
        assert!(id2 > 0, "upsert should return valid ID");

        // Verify code was updated
        let code: String = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT code FROM scripts WHERE name = 'sweep.sh'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert!(code.contains("eth0"), "Code should be updated after upsert");
    }

    #[tokio::test]
    async fn test_search_scripts_fts5() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_script(
            &conn,
            "nmap_scan.sh".to_string(),
            "Network port scanner using nmap".to_string(),
            "bash".to_string(),
            "[\"network\",\"scan\"]".to_string(),
            "nmap -sS $1".to_string(),
        ).await.unwrap();

        save_script(
            &conn,
            "hydra_brute.sh".to_string(),
            "SSH brute force with hydra".to_string(),
            "bash".to_string(),
            "[\"brute\",\"ssh\"]".to_string(),
            "hydra -l admin -P pass.txt ssh://$1".to_string(),
        ).await.unwrap();

        let results = search_scripts(&conn, "nmap".to_string(), 10).await.unwrap();
        assert_eq!(results.len(), 1, "Should find 1 script matching 'nmap'");
        assert_eq!(results[0].name, "nmap_scan.sh");
    }

    #[tokio::test]
    async fn test_search_scripts_empty_query() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let results = search_scripts(&conn, "".to_string(), 10).await.unwrap();
        assert!(results.is_empty(), "Empty query should return empty vec");
    }

    #[tokio::test]
    async fn test_search_scripts_special_chars() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        // Should not crash with FTS5 special characters
        let results = search_scripts(&conn, "test:foo*bar".to_string(), 10).await.unwrap();
        assert!(results.is_empty() || !results.is_empty(), "Should not crash with special chars");
    }

    #[tokio::test]
    async fn test_get_script_by_name_found_and_not_found() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_script(
            &conn,
            "sweep.sh".to_string(),
            "ARP sweep".to_string(),
            "bash".to_string(),
            "[]".to_string(),
            "arp-scan --localnet".to_string(),
        ).await.unwrap();

        let found = get_script_by_name(&conn, "sweep.sh".to_string()).await.unwrap();
        assert!(found.is_some(), "Should find existing script");
        assert_eq!(found.unwrap().name, "sweep.sh");

        let not_found = get_script_by_name(&conn, "nonexistent.sh".to_string()).await.unwrap();
        assert!(not_found.is_none(), "Should return None for missing script");
    }

    #[tokio::test]
    async fn test_update_script_usage_increments() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = save_script(
            &conn,
            "sweep.sh".to_string(),
            "ARP sweep".to_string(),
            "bash".to_string(),
            "[]".to_string(),
            "arp-scan --localnet".to_string(),
        ).await.unwrap();

        update_script_usage(&conn, id).await.unwrap();
        update_script_usage(&conn, id).await.unwrap();

        let (use_count, last_run_at): (i64, Option<String>) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT use_count, last_run_at FROM scripts WHERE id = ?1",
                    rusqlite::params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(use_count, 2, "use_count should be 2 after two updates");
        assert!(last_run_at.is_some(), "last_run_at should be set");
    }

    #[tokio::test]
    async fn test_run_summary_includes_score_fields() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        // Create tasks and findings
        let t1 = log_task(&conn, run_id, "task1", "desc1").await.unwrap();
        update_task(&conn, t1, "completed", "ok").await.unwrap();
        log_finding(&conn, Some(run_id), Some("host".to_string()), "port".to_string(), "data".to_string()).await.unwrap();

        // Create score events
        log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "port_found".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "detection".to_string(), "high".to_string(), true).await.unwrap();

        let summary = get_run_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.task_count, 1);
        assert_eq!(summary.finding_count, 1);
        assert_eq!(summary.completed_task_count, 1);
        assert_eq!(summary.failed_task_count, 0);
        assert_eq!(summary.total_score, -85, "total_score = 10+5-100 = -85");
        assert_eq!(summary.detection_count, 1);
        assert!(summary.last_score_event.is_some(), "last_score_event should be set");
        assert_eq!(summary.last_score_event.unwrap(), "detection");
    }
}
