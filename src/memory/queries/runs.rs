use chrono::Utc;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;

/// Run summary statistics
#[derive(Debug, serde::Serialize)]
pub struct RunSummary {
    pub task_count: i64,
    pub finding_count: i64,
    pub completed_task_count: i64,
    pub failed_task_count: i64,
    pub total_score: i64,
    pub detection_count: i64,
    pub last_score_event: Option<String>,
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

        let (total_score, detection_count): (i64, i64) = conn.query_row(
            "SELECT COALESCE(SUM(points), 0), COALESCE(SUM(CASE WHEN detected = 1 THEN 1 ELSE 0 END), 0) FROM score_events WHERE run_id = ?1",
            rusqlite::params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let last_score_event: Option<String> = match conn.query_row(
            "SELECT action FROM score_events WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
            rusqlite::params![run_id],
            |row| row.get(0),
        ) {
            Ok(action) => Some(action),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };

        Ok(RunSummary {
            task_count,
            finding_count,
            completed_task_count,
            failed_task_count,
            total_score,
            detection_count,
            last_score_event,
        })
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};
    use crate::memory::log_score_event;

    #[tokio::test]
    async fn test_crud_operations() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let run_id = create_run(&conn, "manual".to_string(), Some("test trigger".to_string()))
            .await.unwrap();
        assert!(run_id > 0);

        let run_status: String = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT status FROM runs WHERE id = ?1", rusqlite::params![run_id], |row| row.get(0))?)
            }).await.unwrap();
        assert_eq!(run_status, "running");

        let finding_id = log_finding(&conn, Some(run_id), Some("192.168.1.1".to_string()), "open_port".to_string(), "port 22 open".to_string())
            .await.unwrap();
        assert!(finding_id > 0);
    }

    #[tokio::test]
    async fn test_log_task_returns_valid_id() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let task_id = log_task(&conn, run_id, "arp_sweep", "Sweep the local subnet").await.unwrap();
        assert!(task_id > 0);

        let status: String = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT status FROM tasks WHERE id = ?1", rusqlite::params![task_id], |row| row.get(0))?)
            }).await.unwrap();
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
            }).await.unwrap();
        assert_eq!(status, "completed");
        assert_eq!(result, "result text");
        assert!(completed_at.is_some());
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
                Ok(conn.query_row("SELECT result FROM tasks WHERE id = ?1", rusqlite::params![task_id], |row| row.get(0))?)
            }).await.unwrap();
        assert_eq!(result.len(), 2000);
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
            }).await.unwrap();
        assert_eq!(status, "completed");
        assert!(completed_at.is_some());
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
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].finding_type, "port_scan");
        assert_eq!(findings[1].finding_type, "service");
    }

    #[tokio::test]
    async fn test_get_findings_by_host_returns_empty_for_unknown() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let findings = get_findings_by_host(&conn, "unknown.host".to_string()).await.unwrap();
        assert!(findings.is_empty());
    }

    #[tokio::test]
    async fn test_get_run_summary_counts() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let t1 = log_task(&conn, run_id, "task1", "desc1").await.unwrap();
        let t2 = log_task(&conn, run_id, "task2", "desc2").await.unwrap();
        let t3 = log_task(&conn, run_id, "task3", "desc3").await.unwrap();
        update_task(&conn, t1, "completed", "ok").await.unwrap();
        update_task(&conn, t2, "completed", "ok").await.unwrap();
        update_task(&conn, t3, "failed", "err").await.unwrap();

        log_finding(&conn, Some(run_id), Some("host".to_string()), "port".to_string(), "data".to_string()).await.unwrap();
        log_finding(&conn, Some(run_id), Some("host".to_string()), "svc".to_string(), "data".to_string()).await.unwrap();

        let summary = get_run_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.task_count, 3);
        assert_eq!(summary.finding_count, 2);
        assert_eq!(summary.completed_task_count, 2);
        assert_eq!(summary.failed_task_count, 1);
    }

    #[tokio::test]
    async fn test_run_summary_includes_score_fields() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let t1 = log_task(&conn, run_id, "task1", "desc1").await.unwrap();
        update_task(&conn, t1, "completed", "ok").await.unwrap();
        log_finding(&conn, Some(run_id), Some("host".to_string()), "port".to_string(), "data".to_string()).await.unwrap();

        log_score_event(&conn, Some(run_id), "host_discovered".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "port_found".to_string(), "low".to_string(), false).await.unwrap();
        log_score_event(&conn, Some(run_id), "detection".to_string(), "high".to_string(), true).await.unwrap();

        let summary = get_run_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_score, -85);
        assert_eq!(summary.detection_count, 1);
        assert_eq!(summary.last_score_event.unwrap(), "detection");
    }
}
