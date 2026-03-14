//! LLM interaction logger implementing rig's PromptHook trait.
//!
//! Provides structured `[llm]` log lines and async DB persistence for every
//! LLM completion call. Attach to any agent via `AgentBuilder::hook(logger)`.
//!
//! Log levels (controlled by `Config::llm_log_level`):
//! - **Off**: No logging, no DB writes
//! - **Summary**: `[llm] provider=X model=Y input_tokens=N output_tokens=N total_tokens=N latency_ms=N status=success`
//! - **Full**: Summary fields plus `prompt="..." response="..."`

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;
use tokio_rusqlite::Connection;

use rig::agent::{HookAction, PromptHook};
use rig::completion::{AssistantContent, CompletionModel, CompletionResponse, Message};
use rig::completion::message::UserContent;
use rig::OneOrMany;

use crate::config::{Config, LlmLogLevel};

/// Logger that intercepts LLM calls via PromptHook and emits structured logs + DB writes.
///
/// Clone is required by PromptHook. All internal state is Arc-wrapped for shared ownership.
#[derive(Clone)]
pub struct LlmLogger {
    config: Arc<Config>,
    db: Arc<Connection>,
    run_id: Option<i64>,
    caller_context: String,
    call_start: Arc<Mutex<Option<Instant>>>,
    prompt_text: Arc<Mutex<Option<String>>>,
}

impl LlmLogger {
    /// Create a new LlmLogger.
    ///
    /// The logger reads `config.llm_log_level` to determine verbosity and
    /// `config.provider`/`config.model` for structured log fields.
    /// `run_id` ties interactions to a specific campaign run.
    /// `caller_context` identifies the agent (e.g. "orchestrator", "executor:nmap_scan").
    pub fn new(config: Arc<Config>, db: Arc<Connection>, run_id: Option<i64>, caller_context: impl Into<String>) -> Self {
        Self {
            config,
            db,
            run_id,
            caller_context: caller_context.into(),
            call_start: Arc::new(Mutex::new(None)),
            prompt_text: Arc::new(Mutex::new(None)),
        }
    }
}

/// Extract text content from a Message (User or Assistant).
///
/// Filters for Text variants only; tool calls, images, etc. are ignored.
fn extract_message_text(msg: &Message) -> String {
    match msg {
        Message::User { content } => content
            .iter()
            .filter_map(|c| {
                if let UserContent::Text(t) = c {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|c| {
                if let AssistantContent::Text(t) = c {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Extract text from a CompletionResponse's choice field.
///
/// Filters for AssistantContent::Text variants; tool calls are ignored.
fn extract_choice_text(choice: &OneOrMany<AssistantContent>) -> String {
    choice
        .iter()
        .filter_map(|c| {
            if let AssistantContent::Text(t) = c {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl<M: CompletionModel> PromptHook<M> for LlmLogger {
    async fn on_completion_call(
        &self,
        prompt: &Message,
        _history: &[Message],
    ) -> HookAction {
        // Record start time for latency measurement
        *self.call_start.lock().await = Some(Instant::now());

        // Capture prompt text for potential Full-level logging and DB persistence
        let text = extract_message_text(prompt);
        *self.prompt_text.lock().await = Some(text);

        HookAction::cont()
    }

    async fn on_completion_response(
        &self,
        _prompt: &Message,
        response: &CompletionResponse<M::Response>,
    ) -> HookAction {
        let latency_ms = self
            .call_start
            .lock()
            .await
            .take()
            .map(|start| start.elapsed().as_millis() as i64)
            .unwrap_or(0);

        let prompt_text = self.prompt_text.lock().await.take();
        let response_text = extract_choice_text(&response.choice);

        let usage = &response.usage;
        let provider = self.config.provider.clone().unwrap_or_default();
        let model = self.config.model.clone().unwrap_or_default();
        let level = self.config.llm_log_level;

        // Structured logging
        match level {
            LlmLogLevel::Off => {}
            LlmLogLevel::Summary => {
                log::info!(
                    "[llm] agent={} provider={} model={} input_tokens={} output_tokens={} total_tokens={} latency_ms={} status=success",
                    self.caller_context, provider, model, usage.input_tokens, usage.output_tokens, usage.total_tokens, latency_ms
                );
            }
            LlmLogLevel::Full => {
                log::info!(
                    "[llm] agent={} provider={} model={} input_tokens={} output_tokens={} total_tokens={} latency_ms={} status=success prompt=\"{}\" response=\"{}\"",
                    self.caller_context, provider, model, usage.input_tokens, usage.output_tokens, usage.total_tokens, latency_ms,
                    prompt_text.as_deref().unwrap_or(""),
                    &response_text
                );
            }
        }

        // Fire-and-forget DB write (skip for Off level)
        if level != LlmLogLevel::Off {
            let db = self.db.clone();
            let run_id = self.run_id;
            let caller_context = self.caller_context.clone();
            let request_id = uuid::Uuid::new_v4().to_string();
            let created_at = chrono::Utc::now().to_rfc3339();
            let store_prompt = if level == LlmLogLevel::Full {
                prompt_text
            } else {
                None
            };
            let store_response = if level == LlmLogLevel::Full {
                Some(response_text)
            } else {
                None
            };
            let input_tokens = usage.input_tokens as i64;
            let output_tokens = usage.output_tokens as i64;
            let total_tokens = usage.total_tokens as i64;

            tokio::spawn(async move {
                if let Err(e) = crate::memory::insert_llm_interaction(
                    &db,
                    run_id,
                    &request_id,
                    &provider,
                    &model,
                    Some(&caller_context),
                    store_prompt.as_deref(),
                    store_response.as_deref(),
                    Some(input_tokens),
                    Some(output_tokens),
                    Some(total_tokens),
                    Some(latency_ms),
                    "success",
                    None,
                    &created_at,
                )
                .await
                {
                    log::warn!("[llm] failed to persist interaction: {}", e);
                }
            });
        }

        HookAction::cont()
    }
}

/// Log an LLM call failure with structured [llm] prefix.
///
/// Call from error handling paths where the PromptHook won't fire
/// (e.g., network errors, API errors returned before completion).
pub fn log_llm_error(config: &Config, caller_context: &str, error: &dyn std::fmt::Display) {
    if config.llm_log_level == LlmLogLevel::Off {
        return;
    }
    let provider = config.provider.as_deref().unwrap_or("");
    let model = config.model.as_deref().unwrap_or("");
    log::error!(
        "[llm] agent={} provider={} model={} status=error error=\"{}\"",
        caller_context,
        provider,
        model,
        error
    );
}

/// Log an LLM error and persist it to the DB via fire-and-forget spawn.
///
/// Combines `log_llm_error` with async DB persistence of the error record.
/// `run_id` ties the error to a specific campaign run.
/// `caller_context` identifies the agent (e.g. "orchestrator", "executor:scan").
pub fn log_llm_error_with_persist(
    config: Arc<Config>,
    db: Arc<Connection>,
    run_id: Option<i64>,
    caller_context: &str,
    error: &dyn std::fmt::Display,
) {
    let error_str = error.to_string();
    log_llm_error(&config, caller_context, &error_str);

    if config.llm_log_level != LlmLogLevel::Off {
        let request_id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let provider = config.provider.clone().unwrap_or_default();
        let model = config.model.clone().unwrap_or_default();
        let caller_ctx = caller_context.to_string();

        tokio::spawn(async move {
            let _ = crate::memory::insert_llm_interaction(
                &db,
                run_id,
                &request_id,
                &provider,
                &model,
                Some(&caller_ctx),
                None,
                None,
                None,
                None,
                None,
                None,
                "error",
                Some(&error_str),
                &created_at,
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mock::{MockCompletionModel, MockRawResponse};
    use crate::memory::{init_schema, open_memory_store};
    use rig::completion::message::Text;
    use rig::completion::Usage;

    /// Helper to create a test Config with specified log level and provider/model.
    fn test_config(level: LlmLogLevel) -> Arc<Config> {
        Arc::new(Config {
            llm_log_level: level,
            provider: Some("test-provider".to_string()),
            model: Some("test-model".to_string()),
            ..Config::default()
        })
    }

    /// Helper to create a CompletionResponse with given usage and response text.
    fn make_response(
        response_text: &str,
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
    ) -> CompletionResponse<MockRawResponse> {
        CompletionResponse {
            choice: OneOrMany::one(AssistantContent::Text(Text {
                text: response_text.to_string(),
            })),
            usage: Usage {
                input_tokens,
                output_tokens,
                total_tokens,
                cached_input_tokens: 0,
            },
            raw_response: MockRawResponse,
            message_id: None,
        }
    }

    /// Helper to create a User message with text content.
    fn user_message(text: &str) -> Message {
        Message::User {
            content: OneOrMany::one(UserContent::Text(Text {
                text: text.to_string(),
            })),
        }
    }

    #[test]
    fn test_extract_message_text_user() {
        let msg = user_message("Hello, world!");
        assert_eq!(extract_message_text(&msg), "Hello, world!");
    }

    #[test]
    fn test_extract_choice_text() {
        let choice = OneOrMany::one(AssistantContent::Text(Text {
            text: "Response text".to_string(),
        }));
        assert_eq!(extract_choice_text(&choice), "Response text");
    }

    #[tokio::test]
    async fn test_summary_level_db_no_prompt_response_text() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = test_config(LlmLogLevel::Summary);
        let logger = LlmLogger::new(config, conn.clone(), None, "test");

        let prompt = user_message("test prompt");
        let response = make_response("test response", 10, 20, 30);

        // Call both hook methods
        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        // Allow tokio::spawn to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify DB row exists with NO prompt_text or response_text
        let (prompt_text, response_text): (Option<String>, Option<String>) = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT prompt_text, response_text FROM llm_interactions LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert!(
            prompt_text.is_none(),
            "Summary level should NOT persist prompt_text"
        );
        assert!(
            response_text.is_none(),
            "Summary level should NOT persist response_text"
        );
    }

    #[tokio::test]
    async fn test_full_level_db_has_prompt_response_text() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = test_config(LlmLogLevel::Full);
        let logger = LlmLogger::new(config, conn.clone(), None, "test");

        let prompt = user_message("my prompt");
        let response = make_response("my response", 5, 15, 20);

        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (prompt_text, response_text): (Option<String>, Option<String>) = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT prompt_text, response_text FROM llm_interactions LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(prompt_text, Some("my prompt".to_string()));
        assert_eq!(response_text, Some("my response".to_string()));
    }

    #[tokio::test]
    async fn test_off_level_no_db_writes() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = test_config(LlmLogLevel::Off);
        let logger = LlmLogger::new(config, conn.clone(), None, "test");

        let prompt = user_message("test");
        let response = make_response("test", 1, 2, 3);

        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let count: i64 = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM llm_interactions",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(count, 0, "Off level should produce zero DB writes");
    }

    #[tokio::test]
    async fn test_summary_level_db_has_token_counts_and_latency() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = test_config(LlmLogLevel::Summary);
        let logger = LlmLogger::new(config, conn.clone(), None, "test");

        let prompt = user_message("test");
        let response = make_response("response", 100, 200, 300);

        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (provider, model, input_tok, output_tok, total_tok, latency, status): (
            String,
            String,
            i64,
            i64,
            i64,
            i64,
            String,
        ) = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT provider, model, input_tokens, output_tokens, total_tokens, latency_ms, status FROM llm_interactions LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(provider, "test-provider");
        assert_eq!(model, "test-model");
        assert_eq!(input_tok, 100);
        assert_eq!(output_tok, 200);
        assert_eq!(total_tok, 300);
        assert!(latency >= 0, "latency should be non-negative");
        assert_eq!(status, "success");
    }

    #[tokio::test]
    async fn test_async_db_write_row_appears() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let config = test_config(LlmLogLevel::Full);
        let logger = LlmLogger::new(config, conn.clone(), None, "test");

        let prompt = user_message("async test prompt");
        let response = make_response("async test response", 42, 84, 126);

        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        // Wait for tokio::spawn to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let count: i64 = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM llm_interactions",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(count, 1, "Async DB write should create exactly one row");
    }

    #[tokio::test]
    async fn test_caller_context_stored_in_db() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = crate::memory::create_run(&conn, "test".to_string(), None)
            .await
            .unwrap();
        let config = test_config(LlmLogLevel::Summary);
        let logger = LlmLogger::new(config, conn.clone(), Some(run_id), "orchestrator");

        let prompt = user_message("context test prompt");
        let response = make_response("context test response", 10, 20, 30);

        PromptHook::<MockCompletionModel>::on_completion_call(&logger, &prompt, &[]).await;
        PromptHook::<MockCompletionModel>::on_completion_response(&logger, &prompt, &response)
            .await;

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (db_caller_context, db_run_id): (Option<String>, Option<i64>) = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT caller_context, run_id FROM llm_interactions LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(
            db_caller_context,
            Some("orchestrator".to_string()),
            "caller_context should be stored in DB"
        );
        assert_eq!(
            db_run_id,
            Some(run_id),
            "run_id should be stored in DB"
        );
    }

    #[tokio::test]
    async fn test_error_persist_with_run_id() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        let run_id = crate::memory::create_run(&conn, "test".to_string(), None)
            .await
            .unwrap();
        let config = test_config(LlmLogLevel::Summary);

        log_llm_error_with_persist(
            config,
            conn.clone(),
            Some(run_id),
            "executor:scan",
            &"test error message",
        );

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let (db_run_id, db_caller_context, db_status): (Option<i64>, Option<String>, String) = conn
            .call(|conn| {
                Ok(conn.query_row(
                    "SELECT run_id, caller_context, status FROM llm_interactions LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(db_run_id, Some(run_id), "run_id should be stored for error persist");
        assert_eq!(
            db_caller_context,
            Some("executor:scan".to_string()),
            "caller_context should be stored for error persist"
        );
        assert_eq!(db_status, "error");
    }
}
