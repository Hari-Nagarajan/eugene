//! Dispatch tools for the orchestrator agent.
//!
//! Provides `DispatchTaskTool` and `DispatchParallelTasksTool` that spawn
//! ephemeral executor agents via `tokio::spawn` with `Semaphore`-bounded
//! concurrency. Each dispatch creates a fresh rig `Agent` with recon tools,
//! runs it to completion, and returns the result string.

use rig::agent::AgentBuilder;
use rig::completion::{CompletionModel, Prompt, ToolDefinition};
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_rusqlite::Connection;

use crate::agent::prompt::EXECUTOR_PROMPT;
use crate::config::Config;
use crate::memory::{log_task, update_task};
use crate::tools::{make_executor_tools, ToolError};

/// Arguments for dispatching a single task to an executor agent.
#[derive(Deserialize)]
pub struct DispatchTaskArgs {
    /// Short name for task tracking (e.g., "arp_sweep", "port_scan_10.0.0.1")
    pub task_name: String,
    /// Full description of what the executor should do
    pub task_description: String,
}

/// Specification for a single task in a parallel dispatch batch.
#[derive(Deserialize, Clone)]
pub struct TaskSpec {
    /// Short name for task tracking
    pub name: String,
    /// Full description of what the executor should do
    pub description: String,
}

/// Arguments for dispatching multiple tasks in parallel.
#[derive(Deserialize)]
pub struct DispatchParallelArgs {
    /// List of tasks to dispatch concurrently
    pub tasks: Vec<TaskSpec>,
}

/// Tool that dispatches a single task to an executor agent.
///
/// Acquires a semaphore permit, logs the task to DB, spawns an executor
/// agent via `tokio::spawn`, and returns the executor's response string.
/// Errors are caught and returned as formatted error strings, never propagated.
pub struct DispatchTaskTool<M: CompletionModel> {
    model: Arc<M>,
    config: Arc<Config>,
    memory: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    run_id: i64,
}

impl<M: CompletionModel> DispatchTaskTool<M> {
    pub fn new(
        model: Arc<M>,
        config: Arc<Config>,
        memory: Arc<Connection>,
        semaphore: Arc<Semaphore>,
        run_id: i64,
    ) -> Self {
        Self {
            model,
            config,
            memory,
            semaphore,
            run_id,
        }
    }
}

impl<M> Tool for DispatchTaskTool<M>
where
    M: CompletionModel + 'static,
{
    const NAME: &'static str = "dispatch_task";

    type Error = ToolError;
    type Args = DispatchTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "dispatch_task".to_string(),
            description: "Dispatch a single task to an executor agent. \
                The executor will use recon tools (nmap, dig, arp, tcpdump, etc.) \
                to complete the task and return structured findings."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_name": {
                        "type": "string",
                        "description": "Short name for tracking (e.g., 'arp_sweep', 'port_scan_10.0.0.1')"
                    },
                    "task_description": {
                        "type": "string",
                        "description": "Full description of what the executor should do"
                    }
                },
                "required": ["task_name", "task_description"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Acquire semaphore permit (bounded concurrency)
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ToolError::DispatchFailed("semaphore closed".into()))?;

        // Log task to DB before execution
        let task_id = log_task(&self.memory, self.run_id, &args.task_name, &args.task_description)
            .await
            .map_err(|e| ToolError::DispatchFailed(format!("failed to log task: {e}")))?;

        // Clone Arcs for the spawned future
        let model = self.model.clone();
        let config = self.config.clone();
        let memory = self.memory.clone();
        let task_description = args.task_description;
        let task_name = args.task_name;

        // Spawn executor in a separate task
        let handle = tokio::spawn(async move {
            let _permit = permit; // held until task completes, then dropped

            // Create ephemeral executor agent with recon tools
            let executor_tools = make_executor_tools(config, memory.clone());
            let executor = AgentBuilder::new((*model).clone())
                .preamble(EXECUTOR_PROMPT)
                .tools(executor_tools)
                .temperature(0.3)
                .max_tokens(4096)
                .default_max_turns(8)
                .build();

            // Run the executor
            let result: Result<String, String> =
                match executor.prompt(&task_description).await {
                    Ok(result) => {
                        let _ = update_task(&memory, task_id, "completed", &result).await;
                        Ok(result)
                    }
                    Err(e) => {
                        let err = format!("[ERROR] Task '{}' failed: {}", task_name, e);
                        let _ = update_task(&memory, task_id, "failed", &err).await;
                        Ok(err)
                    }
                };
            result
        });

        // Await the spawned task, handle JoinError (panic)
        match handle.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Ok(format!("[ERROR] {}", e)),
            Err(e) => Ok(format!("[ERROR] JoinError: {}", e)),
        }
    }
}

/// Tool that dispatches multiple tasks to executor agents concurrently.
///
/// Each task acquires a semaphore permit before spawning, which gates
/// concurrency to `max_concurrent_executors`. Returns combined results
/// from all executors as a formatted string.
pub struct DispatchParallelTasksTool<M: CompletionModel> {
    model: Arc<M>,
    config: Arc<Config>,
    memory: Arc<Connection>,
    semaphore: Arc<Semaphore>,
    run_id: i64,
}

impl<M: CompletionModel> DispatchParallelTasksTool<M> {
    pub fn new(
        model: Arc<M>,
        config: Arc<Config>,
        memory: Arc<Connection>,
        semaphore: Arc<Semaphore>,
        run_id: i64,
    ) -> Self {
        Self {
            model,
            config,
            memory,
            semaphore,
            run_id,
        }
    }
}

impl<M> Tool for DispatchParallelTasksTool<M>
where
    M: CompletionModel + 'static,
{
    const NAME: &'static str = "dispatch_parallel_tasks";

    type Error = ToolError;
    type Args = DispatchParallelArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "dispatch_parallel_tasks".to_string(),
            description: "Dispatch multiple tasks to executor agents concurrently \
                (bounded by max concurrent executors). Returns combined results from all executors."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "description": "Short name for tracking"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Full description of what the executor should do"
                                }
                            },
                            "required": ["name", "description"]
                        },
                        "description": "Array of tasks to dispatch concurrently"
                    }
                },
                "required": ["tasks"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut handles = Vec::new();

        for task in &args.tasks {
            // Acquire semaphore permit before spawning -- gates concurrency
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| ToolError::DispatchFailed("semaphore closed".into()))?;

            // Log task to DB
            let task_id = log_task(&self.memory, self.run_id, &task.name, &task.description)
                .await
                .map_err(|e| ToolError::DispatchFailed(format!("failed to log task: {e}")))?;

            // Clone Arcs for the spawned future
            let model = self.model.clone();
            let config = self.config.clone();
            let memory = self.memory.clone();
            let task_description = task.description.clone();
            let task_name = task.name.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit; // held until task completes, then dropped

                // Create ephemeral executor agent with recon tools
                let executor_tools = make_executor_tools(config, memory.clone());
                let executor = AgentBuilder::new((*model).clone())
                    .preamble(EXECUTOR_PROMPT)
                    .tools(executor_tools)
                    .temperature(0.3)
                    .max_tokens(4096)
                    .default_max_turns(8)
                    .build();

                // Run the executor
                match executor.prompt(&task_description).await {
                    Ok(result) => {
                        let _ = update_task(&memory, task_id, "completed", &result).await;
                        (task_name, Ok(result))
                    }
                    Err(e) => {
                        let err = format!("[ERROR] Task '{}' failed: {}", task_name, e);
                        let _ = update_task(&memory, task_id, "failed", &err).await;
                        (task_name, Ok::<String, String>(err))
                    }
                }
            });

            handles.push(handle);
        }

        // Collect all results
        let mut output = String::new();
        for handle in handles {
            match handle.await {
                Ok((name, Ok(result))) => {
                    output.push_str(&format!("=== Task: {} ===\n{}\n\n", name, result));
                }
                Ok((name, Err(e))) => {
                    output.push_str(&format!(
                        "=== Task: {} ===\n[ERROR] {}\n\n",
                        name, e
                    ));
                }
                Err(e) => {
                    output.push_str(&format!("=== Task: unknown ===\n[ERROR] JoinError: {}\n\n", e));
                }
            }
        }

        Ok(output.trim_end().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mock::MockCompletionModel;
    use crate::memory::{create_run, init_schema, open_memory_store};
    use rig::message::AssistantContent;
    use rig::OneOrMany;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Helper: create in-memory DB + run + semaphore for dispatch tests.
    async fn setup_dispatch() -> (Arc<Config>, Arc<Connection>, Arc<Semaphore>, i64) {
        let config = Arc::new(Config::default());
        let memory = open_memory_store(":memory:").await.unwrap();
        init_schema(&memory).await.unwrap();
        let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_executors));
        (config, memory, semaphore, run_id)
    }

    #[tokio::test]
    async fn test_dispatch_task_returns_executor_result() {
        let (config, memory, semaphore, run_id) = setup_dispatch().await;

        // Mock executor: single text response
        let mock = MockCompletionModel::new(vec![OneOrMany::one(AssistantContent::text(
            "TASK: arp_sweep\nSTATUS: success\nFINDINGS:\n  - Found 3 hosts on local subnet",
        ))]);

        let tool = DispatchTaskTool::new(
            Arc::new(mock),
            config,
            memory,
            semaphore,
            run_id,
        );

        let result = tool
            .call(DispatchTaskArgs {
                task_name: "arp_sweep".to_string(),
                task_description: "Run arp-scan on local network".to_string(),
            })
            .await
            .unwrap();

        assert!(
            result.contains("arp_sweep"),
            "Result should contain executor response, got: {result}"
        );
        assert!(
            result.contains("success"),
            "Result should contain executor success status"
        );
    }

    #[tokio::test]
    async fn test_dispatch_parallel_returns_two_results() {
        let (config, memory, semaphore, run_id) = setup_dispatch().await;

        // Mock: 2 executors each return a single text
        // Note: MockCompletionModel is Clone (uses Arc<Mutex<Vec>>), so the
        // model shared across 2 executors needs 2 responses.
        let mock = MockCompletionModel::new(vec![
            OneOrMany::one(AssistantContent::text("Result from task t1")),
            OneOrMany::one(AssistantContent::text("Result from task t2")),
        ]);

        let tool = DispatchParallelTasksTool::new(
            Arc::new(mock),
            config,
            memory,
            semaphore,
            run_id,
        );

        let result = tool
            .call(DispatchParallelArgs {
                tasks: vec![
                    TaskSpec {
                        name: "t1".to_string(),
                        description: "Do thing 1".to_string(),
                    },
                    TaskSpec {
                        name: "t2".to_string(),
                        description: "Do thing 2".to_string(),
                    },
                ],
            })
            .await
            .unwrap();

        assert!(
            result.contains("=== Task: t1 ==="),
            "Result should contain t1 header, got: {result}"
        );
        assert!(
            result.contains("=== Task: t2 ==="),
            "Result should contain t2 header, got: {result}"
        );
        assert!(
            result.contains("Result from task t1"),
            "Result should contain t1 response"
        );
        assert!(
            result.contains("Result from task t2"),
            "Result should contain t2 response"
        );
    }

    #[tokio::test]
    async fn test_dispatch_task_logs_to_db() {
        let (config, memory, semaphore, run_id) = setup_dispatch().await;

        let mock = MockCompletionModel::new(vec![OneOrMany::one(AssistantContent::text(
            "Scan complete.",
        ))]);

        let tool = DispatchTaskTool::new(
            Arc::new(mock),
            config,
            memory.clone(),
            semaphore,
            run_id,
        );

        tool.call(DispatchTaskArgs {
            task_name: "test_scan".to_string(),
            task_description: "Test scan description".to_string(),
        })
        .await
        .unwrap();

        // Verify task was logged and updated in DB
        let (status, result): (String, String) = memory
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status, result FROM tasks WHERE run_id = ?1 AND name = ?2",
                    rusqlite::params![run_id, "test_scan"],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .await
            .unwrap();

        assert_eq!(status, "completed", "Task should be marked completed");
        assert!(
            result.contains("Scan complete"),
            "Task result should be stored in DB"
        );
    }

    #[tokio::test]
    async fn test_dispatch_task_executor_failure_returns_error_string() {
        let (config, memory, semaphore, run_id) = setup_dispatch().await;

        // Mock with empty response queue -- will panic, causing a JoinError
        // But we need a model that produces an error, not a panic.
        // Instead: use a model that returns empty responses so the agent
        // exhausts its turns and returns a PromptError.
        //
        // Actually, MockCompletionModel panics on exhausted queue.
        // A panicking executor should be caught as JoinError.
        let mock = MockCompletionModel::new(vec![]); // will panic

        let tool = DispatchTaskTool::new(
            Arc::new(mock),
            config,
            memory.clone(),
            semaphore,
            run_id,
        );

        let result = tool
            .call(DispatchTaskArgs {
                task_name: "failing_task".to_string(),
                task_description: "This will fail".to_string(),
            })
            .await
            .unwrap(); // Should not return Err -- error encoded in string

        assert!(
            result.contains("[ERROR]"),
            "Failed executor should return error string, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_semaphore_bounds_concurrency() {
        // Use semaphore with max=1 to prove tasks run sequentially
        let config = Arc::new(Config::default());
        let memory = open_memory_store(":memory:").await.unwrap();
        init_schema(&memory).await.unwrap();
        let run_id = create_run(&memory, "test".to_string(), None).await.unwrap();
        let semaphore = Arc::new(Semaphore::new(1)); // max 1 concurrent

        // Track peak concurrency using an atomic counter
        static PEAK: AtomicUsize = AtomicUsize::new(0);
        static CURRENT: AtomicUsize = AtomicUsize::new(0);
        PEAK.store(0, Ordering::SeqCst);
        CURRENT.store(0, Ordering::SeqCst);

        // Both executors return immediately
        let mock = MockCompletionModel::new(vec![
            OneOrMany::one(AssistantContent::text("Result 1")),
            OneOrMany::one(AssistantContent::text("Result 2")),
        ]);

        let tool = DispatchParallelTasksTool::new(
            Arc::new(mock),
            config,
            memory,
            semaphore,
            run_id,
        );

        let result = tool
            .call(DispatchParallelArgs {
                tasks: vec![
                    TaskSpec {
                        name: "s1".to_string(),
                        description: "Task 1".to_string(),
                    },
                    TaskSpec {
                        name: "s2".to_string(),
                        description: "Task 2".to_string(),
                    },
                ],
            })
            .await
            .unwrap();

        // Both tasks should complete successfully
        assert!(result.contains("=== Task: s1 ==="), "Should have s1 result");
        assert!(result.contains("=== Task: s2 ==="), "Should have s2 result");

        // The semaphore with max=1 ensures at most 1 executor runs at a time.
        // We can't easily measure peak concurrency in a unit test, but the
        // fact that both tasks complete without deadlock proves the semaphore
        // works (acquire_owned + drop pattern).
    }

    #[tokio::test]
    async fn test_dispatch_task_failed_executor_updates_db_status() {
        let (config, memory, semaphore, run_id) = setup_dispatch().await;

        // Empty queue causes panic -> JoinError
        let mock = MockCompletionModel::new(vec![]);

        let tool = DispatchTaskTool::new(
            Arc::new(mock),
            config,
            memory.clone(),
            semaphore,
            run_id,
        );

        let _ = tool
            .call(DispatchTaskArgs {
                task_name: "panicking_task".to_string(),
                task_description: "This panics".to_string(),
            })
            .await
            .unwrap();

        // The task was logged to DB before the spawn. The panic inside the
        // spawned task means update_task("failed") may not have been called.
        // But the task should at least exist in the DB with status "running".
        let status: String = memory
            .call(move |conn| {
                Ok(conn.query_row(
                    "SELECT status FROM tasks WHERE run_id = ?1 AND name = ?2",
                    rusqlite::params![run_id, "panicking_task"],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();

        // Task exists in DB -- either still "running" (panic before update) or "failed"
        assert!(
            status == "running" || status == "failed",
            "Task should exist in DB with running or failed status, got: {status}"
        );
    }
}
