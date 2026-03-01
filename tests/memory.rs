//! Integration tests for DB persistence: schema init, sessions, schedules,
//! findings round-trips, due schedules, advance schedule.

mod common;

use std::sync::Arc;
use tokio_rusqlite::Connection;

// ========== Schema Tests ==========

#[tokio::test]
async fn test_schema_initialization() {
    let db = common::setup_db().await;

    // Verify key tables exist (not exact count — avoids breaking when tables are added)
    for table in ["runs", "findings", "tasks", "memories", "scripts", "scheduled_tasks"] {
        let table = table.to_string();
        let table_clone = table.clone();
        let exists: bool = db
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        table
                    ),
                    [],
                    |row| row.get(0),
                )?;
                Ok(count == 1)
            })
            .await
            .unwrap();
        assert!(exists, "Table '{}' should exist", table_clone);
    }

    // Verify FTS5 virtual tables exist
    for fts_table in ["memories_fts", "scripts_fts"] {
        let fts_table = fts_table.to_string();
        let fts_table_clone = fts_table.clone();
        let exists: bool = db
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        fts_table
                    ),
                    [],
                    |row| row.get(0),
                )?;
                Ok(count == 1)
            })
            .await
            .unwrap();
        assert!(exists, "FTS5 table '{}' should exist", fts_table_clone);
    }
}

// ========== Session Tests ==========

async fn setup_session_db() -> Arc<Connection> {
    common::setup_db().await
}

#[tokio::test]
async fn test_session_save_load_roundtrip() {
    let db = setup_session_db().await;
    let chat_id = "test-chat-1".to_string();
    let messages = r#"[{"role":"user","content":"hello"}]"#.to_string();

    eugene::memory::save_session(&db, chat_id.clone(), messages.clone())
        .await
        .unwrap();

    let loaded = eugene::memory::load_session(&db, chat_id).await.unwrap();
    assert_eq!(loaded, messages);
}

#[tokio::test]
async fn test_session_upsert() {
    let db = setup_session_db().await;
    let chat_id = "test-chat-upsert".to_string();

    eugene::memory::save_session(
        &db,
        chat_id.clone(),
        r#"[{"role":"user","content":"first"}]"#.to_string(),
    )
    .await
    .unwrap();
    eugene::memory::save_session(
        &db,
        chat_id.clone(),
        r#"[{"role":"user","content":"second"}]"#.to_string(),
    )
    .await
    .unwrap();

    let loaded = eugene::memory::load_session(&db, chat_id).await.unwrap();
    assert_eq!(loaded, r#"[{"role":"user","content":"second"}]"#);
}

#[tokio::test]
async fn test_session_clear() {
    let db = setup_session_db().await;
    let chat_id = "test-chat-clear".to_string();

    eugene::memory::save_session(
        &db,
        chat_id.clone(),
        r#"[{"role":"user","content":"data"}]"#.to_string(),
    )
    .await
    .unwrap();
    eugene::memory::clear_session(&db, chat_id.clone())
        .await
        .unwrap();

    let loaded = eugene::memory::load_session(&db, chat_id).await.unwrap();
    assert_eq!(loaded, "[]");
}

#[tokio::test]
async fn test_session_load_nonexistent() {
    let db = setup_session_db().await;
    let loaded = eugene::memory::load_session(&db, "nonexistent-chat".to_string())
        .await
        .unwrap();
    assert_eq!(loaded, "[]");
}

// ========== Schedule Tests ==========

#[tokio::test]
async fn test_schedule_create_and_list() {
    let db = common::setup_db().await;
    let chat_id = "sched-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "scan network".to_string(),
    )
    .await
    .unwrap();

    assert_eq!(id.len(), 36);

    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].id, id);
    assert_eq!(schedules[0].prompt, "scan network");
    assert_eq!(schedules[0].schedule, "0 */6 * * *");
    assert_eq!(schedules[0].status, "active");
}

#[tokio::test]
async fn test_schedule_create_invalid_cron() {
    let db = common::setup_db().await;
    let result = eugene::memory::create_schedule(
        &db,
        "test".to_string(),
        "bad cron".to_string(),
        "prompt".to_string(),
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_schedule_pause_resume() {
    let db = common::setup_db().await;
    let chat_id = "pause-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "scan".to_string(),
    )
    .await
    .unwrap();

    eugene::memory::pause_schedule(&db, id.clone())
        .await
        .unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id.clone())
        .await
        .unwrap();
    assert_eq!(schedules[0].status, "paused");

    eugene::memory::resume_schedule(&db, id.clone())
        .await
        .unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert_eq!(schedules[0].status, "active");
    assert!(schedules[0].next_run > 0);
}

#[tokio::test]
async fn test_schedule_delete() {
    let db = common::setup_db().await;
    let chat_id = "delete-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "scan".to_string(),
    )
    .await
    .unwrap();

    eugene::memory::delete_schedule(&db, id).await.unwrap();

    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert!(schedules.is_empty());
}

#[tokio::test]
async fn test_schedule_crud_count() {
    let db = common::setup_db().await;
    let chat_id = "count-test".to_string();

    let id1 = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "task one".to_string(),
    )
    .await
    .unwrap();

    eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 0 * * *".to_string(),
        "task two".to_string(),
    )
    .await
    .unwrap();

    let schedules = eugene::memory::list_schedules(&db, chat_id.clone())
        .await
        .unwrap();
    assert_eq!(schedules.len(), 2);

    eugene::memory::delete_schedule(&db, id1).await.unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].prompt, "task two");
}

#[tokio::test]
async fn test_get_due_schedules() {
    let db = common::setup_db().await;
    let chat_id = "due-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "due task".to_string(),
    )
    .await
    .unwrap();

    // Manually set next_run to the past so it appears as due
    let id_clone = id.clone();
    db.call(move |conn| {
        conn.execute(
            "UPDATE scheduled_tasks SET next_run = 0 WHERE id = ?1",
            rusqlite::params![id_clone],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let due = eugene::memory::get_due_schedules(&db).await.unwrap();
    assert!(!due.is_empty());
    assert!(due.iter().any(|s| s.id == id));
}

#[tokio::test]
async fn test_advance_schedule() {
    let db = common::setup_db().await;
    let chat_id = "advance-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "advance task".to_string(),
    )
    .await
    .unwrap();

    let schedules = eugene::memory::list_schedules(&db, chat_id.clone())
        .await
        .unwrap();
    assert!(schedules[0].last_run.is_none());
    assert!(schedules[0].last_result.is_none());

    eugene::memory::advance_schedule(
        &db,
        id.clone(),
        "Scan completed: 3 hosts found".to_string(),
    )
    .await
    .unwrap();

    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    let s = &schedules[0];

    assert!(s.last_run.is_some());
    assert_eq!(
        s.last_result.as_deref(),
        Some("Scan completed: 3 hosts found")
    );
    assert!(s.next_run > 0);
    assert_eq!(s.status, "active");
}

// ========== Cron Validation Tests ==========

#[test]
fn test_validate_cron_valid() {
    assert!(eugene::scheduler::cron::validate_cron("0 */6 * * *").is_ok());
    assert!(eugene::scheduler::cron::validate_cron("30 2 * * 1-5").is_ok());
    assert!(eugene::scheduler::cron::validate_cron("0 0 1 * *").is_ok());
}

#[test]
fn test_validate_cron_invalid() {
    assert!(eugene::scheduler::cron::validate_cron("bad").is_err());
    assert!(eugene::scheduler::cron::validate_cron("* * *").is_err());
    assert!(eugene::scheduler::cron::validate_cron("").is_err());
}

// ========== Service Content Test ==========

#[test]
fn test_service_content_format() {
    let content = eugene::service::generate_service_content().unwrap();

    assert!(content.contains("[Unit]"), "Missing [Unit] section");
    assert!(content.contains("[Service]"), "Missing [Service] section");
    assert!(content.contains("[Install]"), "Missing [Install] section");

    assert!(
        content.contains("ExecStart="),
        "Missing ExecStart directive"
    );
    assert!(content.contains("Restart="), "Missing Restart directive");
    assert!(
        content.contains("bot"),
        "ExecStart should run 'bot' subcommand"
    );
    assert!(
        content.contains("EUGENE_DB_PATH="),
        "Missing EUGENE_DB_PATH env"
    );
}
