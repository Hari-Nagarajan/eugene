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
}
