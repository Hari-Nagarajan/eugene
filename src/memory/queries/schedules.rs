use chrono::Utc;
use tokio_rusqlite::Connection;

use crate::memory::MemoryError;

/// Scheduled task record from scheduled_tasks table
#[derive(Debug)]
pub struct ScheduledTask {
    pub id: String,
    pub chat_id: String,
    pub prompt: String,
    pub schedule: String,
    pub next_run: i64,
    pub last_run: Option<i64>,
    pub last_result: Option<String>,
    pub status: String,
}

/// Create a new scheduled task with cron validation. Returns the UUID.
pub async fn create_schedule(
    conn: &Connection,
    chat_id: String,
    cron_expr: String,
    prompt: String,
) -> Result<String, MemoryError> {
    use croner::Cron;
    use std::str::FromStr;

    let cron = Cron::from_str(&cron_expr)
        .map_err(|e| MemoryError::Query(format!("Invalid cron: {e}")))?;
    let next_run = cron
        .find_next_occurrence(&Utc::now(), false)
        .map_err(|e| MemoryError::Query(format!("No next run: {e}")))?;
    let next_ts = next_run.timestamp();

    let id = uuid::Uuid::new_v4().to_string();
    let id_clone = id.clone();

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO scheduled_tasks (id, chat_id, prompt, schedule, next_run, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'active', unixepoch('now'))",
            rusqlite::params![id_clone, chat_id, prompt, cron_expr, next_ts],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)?;

    Ok(id)
}

/// List all scheduled tasks for a chat_id, ordered by creation time
pub async fn list_schedules(
    conn: &Connection,
    chat_id: String,
) -> Result<Vec<ScheduledTask>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, chat_id, prompt, schedule, next_run, last_run, last_result, status \
             FROM scheduled_tasks WHERE chat_id = ?1 ORDER BY created_at"
        )?;
        let tasks = stmt.query_map(rusqlite::params![chat_id], |row| {
            Ok(ScheduledTask {
                id: row.get(0)?,
                chat_id: row.get(1)?,
                prompt: row.get(2)?,
                schedule: row.get(3)?,
                next_run: row.get(4)?,
                last_run: row.get(5)?,
                last_result: row.get(6)?,
                status: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    })
    .await
    .map_err(MemoryError::from)
}

/// Delete a scheduled task by ID
pub async fn delete_schedule(
    conn: &Connection,
    id: String,
) -> Result<(), MemoryError> {
    conn.call(move |conn| {
        conn.execute(
            "DELETE FROM scheduled_tasks WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

/// Pause a scheduled task (set status to 'paused')
pub async fn pause_schedule(
    conn: &Connection,
    id: String,
) -> Result<(), MemoryError> {
    conn.call(move |conn| {
        conn.execute(
            "UPDATE scheduled_tasks SET status = 'paused' WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

/// Resume a paused schedule (set status to 'active' and recompute next_run)
pub async fn resume_schedule(
    conn: &Connection,
    id: String,
) -> Result<(), MemoryError> {
    use croner::Cron;
    use std::str::FromStr;

    let id_read = id.clone();
    let cron_expr: String = conn
        .call(move |conn| {
            conn.query_row(
                "SELECT schedule FROM scheduled_tasks WHERE id = ?1",
                rusqlite::params![id_read],
                |row| row.get(0),
            )
            .map_err(|e| e.into())
        })
        .await
        .map_err(MemoryError::from)?;

    let cron = Cron::from_str(&cron_expr)
        .map_err(|e| MemoryError::Query(format!("Invalid cron: {e}")))?;
    let next_run = cron
        .find_next_occurrence(&Utc::now(), false)
        .map_err(|e| MemoryError::Query(format!("No next run: {e}")))?;
    let next_ts = next_run.timestamp();

    conn.call(move |conn| {
        conn.execute(
            "UPDATE scheduled_tasks SET status = 'active', next_run = ?1 WHERE id = ?2",
            rusqlite::params![next_ts, id],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

/// Get all active scheduled tasks that are due (next_run <= now)
pub async fn get_due_schedules(
    conn: &Connection,
) -> Result<Vec<ScheduledTask>, MemoryError> {
    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, chat_id, prompt, schedule, next_run, last_run, last_result, status \
             FROM scheduled_tasks WHERE status = 'active' AND next_run <= unixepoch('now')"
        )?;
        let tasks = stmt.query_map([], |row| {
            Ok(ScheduledTask {
                id: row.get(0)?,
                chat_id: row.get(1)?,
                prompt: row.get(2)?,
                schedule: row.get(3)?,
                next_run: row.get(4)?,
                last_run: row.get(5)?,
                last_result: row.get(6)?,
                status: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    })
    .await
    .map_err(MemoryError::from)
}

/// Advance a schedule after execution: update last_run, last_result, compute new next_run
pub async fn advance_schedule(
    conn: &Connection,
    id: String,
    last_result: String,
) -> Result<(), MemoryError> {
    use croner::Cron;
    use std::str::FromStr;

    let id_read = id.clone();
    let cron_expr: String = conn
        .call(move |conn| {
            conn.query_row(
                "SELECT schedule FROM scheduled_tasks WHERE id = ?1",
                rusqlite::params![id_read],
                |row| row.get(0),
            )
            .map_err(|e| e.into())
        })
        .await
        .map_err(MemoryError::from)?;

    let cron = Cron::from_str(&cron_expr)
        .map_err(|e| MemoryError::Query(format!("Invalid cron: {e}")))?;
    let next_run = cron
        .find_next_occurrence(&Utc::now(), false)
        .map_err(|e| MemoryError::Query(format!("No next run: {e}")))?;
    let next_ts = next_run.timestamp();

    conn.call(move |conn| {
        conn.execute(
            "UPDATE scheduled_tasks SET last_run = unixepoch('now'), last_result = ?1, next_run = ?2 WHERE id = ?3",
            rusqlite::params![last_result, next_ts, id],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_schedule_create_returns_uuid() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = create_schedule(&conn, "chat_1".into(), "0 */6 * * *".into(), "scan network".into()).await.unwrap();
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn test_schedule_create_invalid_cron() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let result = create_schedule(&conn, "chat_1".into(), "bad cron".into(), "scan network".into()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schedule_list() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        create_schedule(&conn, "chat_1".into(), "0 */6 * * *".into(), "scan A".into()).await.unwrap();
        create_schedule(&conn, "chat_1".into(), "0 0 * * *".into(), "scan B".into()).await.unwrap();
        create_schedule(&conn, "chat_2".into(), "0 12 * * *".into(), "scan C".into()).await.unwrap();

        let list = list_schedules(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_schedule_delete() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = create_schedule(&conn, "chat_1".into(), "0 */6 * * *".into(), "scan".into()).await.unwrap();
        delete_schedule(&conn, id).await.unwrap();

        let list = list_schedules(&conn, "chat_1".to_string()).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_schedule_pause_and_resume() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = create_schedule(&conn, "chat_1".into(), "0 */6 * * *".into(), "scan".into()).await.unwrap();

        pause_schedule(&conn, id.clone()).await.unwrap();
        let list = list_schedules(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(list[0].status, "paused");

        resume_schedule(&conn, id).await.unwrap();
        let list = list_schedules(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(list[0].status, "active");
    }

    #[tokio::test]
    async fn test_schedule_get_due() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = create_schedule(&conn, "chat_1".into(), "* * * * *".into(), "scan".into()).await.unwrap();

        let id_clone = id.clone();
        conn.call(move |conn| {
            conn.execute("UPDATE scheduled_tasks SET next_run = 0 WHERE id = ?1", rusqlite::params![id_clone])?;
            Ok(())
        }).await.unwrap();

        let due = get_due_schedules(&conn).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id);
    }

    #[tokio::test]
    async fn test_schedule_advance() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = create_schedule(&conn, "chat_1".into(), "0 */6 * * *".into(), "scan".into()).await.unwrap();

        advance_schedule(&conn, id, "scan completed successfully".to_string()).await.unwrap();

        let list = list_schedules(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(list[0].last_result.as_deref(), Some("scan completed successfully"));
        assert!(list[0].last_run.is_some());
    }
}
