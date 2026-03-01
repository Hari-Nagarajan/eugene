//! Phase 6 integration tests: session persistence, schedule CRUD lifecycle,
//! cron validation, and systemd service content generation.

use std::sync::Arc;
use tokio_rusqlite::Connection;

/// Set up an in-memory database with full schema for test isolation.
async fn setup() -> Arc<Connection> {
    let db = eugene::memory::open_memory_store(":memory:").await.unwrap();
    eugene::memory::init_schema(&db).await.unwrap();
    db
}

// ========== Session Tests ==========

#[tokio::test]
async fn test_session_save_load_roundtrip() {
    let db = setup().await;
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
    let db = setup().await;
    let chat_id = "test-chat-upsert".to_string();

    eugene::memory::save_session(&db, chat_id.clone(), r#"[{"role":"user","content":"first"}]"#.to_string())
        .await
        .unwrap();
    eugene::memory::save_session(&db, chat_id.clone(), r#"[{"role":"user","content":"second"}]"#.to_string())
        .await
        .unwrap();

    let loaded = eugene::memory::load_session(&db, chat_id).await.unwrap();
    assert_eq!(loaded, r#"[{"role":"user","content":"second"}]"#);
}

#[tokio::test]
async fn test_session_clear() {
    let db = setup().await;
    let chat_id = "test-chat-clear".to_string();

    eugene::memory::save_session(&db, chat_id.clone(), r#"[{"role":"user","content":"data"}]"#.to_string())
        .await
        .unwrap();
    eugene::memory::clear_session(&db, chat_id.clone()).await.unwrap();

    let loaded = eugene::memory::load_session(&db, chat_id).await.unwrap();
    assert_eq!(loaded, "[]");
}

#[tokio::test]
async fn test_session_load_nonexistent() {
    let db = setup().await;
    let loaded = eugene::memory::load_session(&db, "nonexistent-chat".to_string())
        .await
        .unwrap();
    assert_eq!(loaded, "[]");
}

// ========== Schedule Tests ==========

#[tokio::test]
async fn test_schedule_create_and_list() {
    let db = setup().await;
    let chat_id = "sched-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "scan network".to_string(),
    )
    .await
    .unwrap();

    // UUID should be 36 chars (8-4-4-4-12 format)
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
    let db = setup().await;
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
    let db = setup().await;
    let chat_id = "pause-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "scan".to_string(),
    )
    .await
    .unwrap();

    // Pause
    eugene::memory::pause_schedule(&db, id.clone()).await.unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id.clone()).await.unwrap();
    assert_eq!(schedules[0].status, "paused");

    // Resume
    eugene::memory::resume_schedule(&db, id.clone()).await.unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert_eq!(schedules[0].status, "active");
    // next_run should be recomputed (> 0)
    assert!(schedules[0].next_run > 0);
}

#[tokio::test]
async fn test_schedule_delete() {
    let db = setup().await;
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
    let db = setup().await;
    let chat_id = "count-test".to_string();

    // Create two schedules
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

    let schedules = eugene::memory::list_schedules(&db, chat_id.clone()).await.unwrap();
    assert_eq!(schedules.len(), 2);

    // Delete one
    eugene::memory::delete_schedule(&db, id1).await.unwrap();
    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].prompt, "task two");
}

#[tokio::test]
async fn test_get_due_schedules() {
    let db = setup().await;
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
    let db = setup().await;
    let chat_id = "advance-test".to_string();

    let id = eugene::memory::create_schedule(
        &db,
        chat_id.clone(),
        "0 */6 * * *".to_string(),
        "advance task".to_string(),
    )
    .await
    .unwrap();

    // Record the original next_run
    let schedules = eugene::memory::list_schedules(&db, chat_id.clone()).await.unwrap();
    let _original_next_run = schedules[0].next_run;
    assert!(schedules[0].last_run.is_none());
    assert!(schedules[0].last_result.is_none());

    // Advance schedule
    eugene::memory::advance_schedule(&db, id.clone(), "Scan completed: 3 hosts found".to_string())
        .await
        .unwrap();

    let schedules = eugene::memory::list_schedules(&db, chat_id).await.unwrap();
    let s = &schedules[0];

    // last_run should now be set
    assert!(s.last_run.is_some());
    // last_result should contain the result string
    assert_eq!(s.last_result.as_deref(), Some("Scan completed: 3 hosts found"));
    // next_run should be recomputed (may differ from original depending on timing)
    assert!(s.next_run > 0);
    // The schedule should still be valid
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

    // Must contain all three systemd sections
    assert!(content.contains("[Unit]"), "Missing [Unit] section");
    assert!(content.contains("[Service]"), "Missing [Service] section");
    assert!(content.contains("[Install]"), "Missing [Install] section");

    // Must contain key service directives
    assert!(content.contains("ExecStart="), "Missing ExecStart directive");
    assert!(content.contains("Restart="), "Missing Restart directive");
    assert!(content.contains("bot"), "ExecStart should run 'bot' subcommand");
    assert!(content.contains("EUGENE_DB_PATH="), "Missing EUGENE_DB_PATH env");
}
