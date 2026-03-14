use tokio_rusqlite::Connection;

use crate::memory::MemoryError;

/// Insert a new LLM interaction record into the llm_interactions table.
///
/// All string parameters are cloned to owned values for the async closure.
/// Returns the row ID of the inserted record.
pub async fn insert_llm_interaction(
    conn: &Connection,
    run_id: Option<i64>,
    request_id: &str,
    provider: &str,
    model: &str,
    caller_context: Option<&str>,
    prompt_text: Option<&str>,
    response_text: Option<&str>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    latency_ms: Option<i64>,
    status: &str,
    error_message: Option<&str>,
    created_at: &str,
) -> Result<i64, MemoryError> {
    let run_id = run_id;
    let request_id = request_id.to_string();
    let provider = provider.to_string();
    let model = model.to_string();
    let caller_context = caller_context.map(String::from);
    let prompt_text = prompt_text.map(String::from);
    let response_text = response_text.map(String::from);
    let error_message = error_message.map(String::from);
    let status = status.to_string();
    let created_at = created_at.to_string();

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO llm_interactions (run_id, request_id, provider, model, caller_context, prompt_text, response_text, input_tokens, output_tokens, total_tokens, latency_ms, status, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                run_id, request_id, provider, model, caller_context,
                prompt_text, response_text,
                input_tokens, output_tokens, total_tokens,
                latency_ms, status, error_message, created_at
            ],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Aggregated token usage summary for a specific run.
pub struct RunTokenSummary {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

/// Get aggregated token usage for a run, counting only successful interactions.
///
/// Returns zero values if no matching rows exist.
pub async fn get_run_token_summary(
    conn: &Connection,
    run_id: i64,
) -> Result<RunTokenSummary, MemoryError> {
    conn.call(move |conn| {
        Ok(conn.query_row(
            "SELECT COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COUNT(*)
             FROM llm_interactions
             WHERE run_id = ?1 AND status = 'success'",
            rusqlite::params![run_id],
            |row| {
                Ok(RunTokenSummary {
                    total_input_tokens: row.get(0)?,
                    total_output_tokens: row.get(1)?,
                    total_tokens: row.get(2)?,
                    call_count: row.get(3)?,
                })
            },
        )?)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, create_run};

    #[tokio::test]
    async fn test_insert_all_fields_populated() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        let rowid = insert_llm_interaction(
            &conn,
            Some(run_id),
            "req-001",
            "openai",
            "gpt-4",
            Some("test_context"),
            Some("Hello, world!"),
            Some("Hi there!"),
            Some(10),
            Some(20),
            Some(30),
            Some(150),
            "success",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(rowid > 0);
    }

    #[tokio::test]
    async fn test_insert_nullable_fields_as_none() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let rowid = insert_llm_interaction(
            &conn,
            None,
            "req-002",
            "minimax",
            "MiniMax-M2.5",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            "success",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(rowid > 0);
    }

    #[tokio::test]
    async fn test_insert_error_status_with_message() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let rowid = insert_llm_interaction(
            &conn,
            None,
            "req-003",
            "openai",
            "gpt-4",
            None,
            Some("prompt text"),
            None,
            None,
            None,
            None,
            None,
            "error",
            Some("Connection timeout"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();

        assert!(rowid > 0);

        // Verify the error fields were stored correctly
        let (status, error_msg): (String, Option<String>) = conn
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status, error_message FROM llm_interactions WHERE id = ?1",
                    rusqlite::params![rowid],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(status, "error");
        assert_eq!(error_msg, Some("Connection timeout".to_string()));
    }

    /// Helper to insert a success interaction with known token values.
    async fn insert_success_row(
        conn: &std::sync::Arc<Connection>,
        run_id: i64,
        input: i64,
        output: i64,
        total: i64,
    ) {
        insert_llm_interaction(
            conn,
            Some(run_id),
            &uuid::Uuid::new_v4().to_string(),
            "openai",
            "gpt-4",
            Some("test"),
            None,
            None,
            Some(input),
            Some(output),
            Some(total),
            Some(100),
            "success",
            None,
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
    }

    /// Helper to insert an error interaction.
    async fn insert_error_row(conn: &std::sync::Arc<Connection>, run_id: i64) {
        insert_llm_interaction(
            conn,
            Some(run_id),
            &uuid::Uuid::new_v4().to_string(),
            "openai",
            "gpt-4",
            Some("test"),
            None,
            None,
            Some(50),
            Some(50),
            Some(100),
            Some(100),
            "error",
            Some("test error"),
            "2026-01-01T00:00:00Z",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_run_token_summary_single_row() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        insert_success_row(&conn, run_id, 10, 20, 30).await;

        let summary = get_run_token_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_input_tokens, 10);
        assert_eq!(summary.total_output_tokens, 20);
        assert_eq!(summary.total_tokens, 30);
        assert_eq!(summary.call_count, 1);
    }

    #[tokio::test]
    async fn test_run_token_summary_multiple_rows() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        insert_success_row(&conn, run_id, 10, 20, 30).await;
        insert_success_row(&conn, run_id, 100, 200, 300).await;
        insert_success_row(&conn, run_id, 50, 80, 130).await;

        let summary = get_run_token_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_input_tokens, 160);
        assert_eq!(summary.total_output_tokens, 300);
        assert_eq!(summary.total_tokens, 460);
        assert_eq!(summary.call_count, 3);
    }

    #[tokio::test]
    async fn test_run_token_summary_no_rows() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let summary = get_run_token_summary(&conn, 99999).await.unwrap();
        assert_eq!(summary.total_input_tokens, 0);
        assert_eq!(summary.total_output_tokens, 0);
        assert_eq!(summary.total_tokens, 0);
        assert_eq!(summary.call_count, 0);
    }

    #[tokio::test]
    async fn test_run_token_summary_excludes_errors() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = create_run(&conn, "test".to_string(), None).await.unwrap();

        insert_success_row(&conn, run_id, 10, 20, 30).await;
        insert_success_row(&conn, run_id, 40, 50, 90).await;
        insert_error_row(&conn, run_id).await;

        let summary = get_run_token_summary(&conn, run_id).await.unwrap();
        assert_eq!(summary.total_input_tokens, 50);
        assert_eq!(summary.total_output_tokens, 70);
        assert_eq!(summary.total_tokens, 120);
        assert_eq!(summary.call_count, 2, "Error rows should be excluded");
    }
}
