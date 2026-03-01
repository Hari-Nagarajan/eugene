pub mod cron;

use std::sync::Arc;
use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::ChatAction;
use tokio_rusqlite::Connection;

use crate::bot::formatting::{escape_html, send_chunked};
use crate::config::Config;
use crate::memory::{advance_schedule, get_due_schedules, ScheduledTask};

/// Spawn the background scheduler polling loop.
///
/// Returns a JoinHandle for the spawned task. The scheduler:
/// - Polls every 60 seconds for due scheduled tasks
/// - Spawns each task execution in a separate tokio::spawn (non-blocking)
/// - Sends results to the originating chat_id
/// - Advances next_run regardless of success/failure
pub fn spawn_scheduler(
    db: Arc<Connection>,
    bot: Bot,
    config: Arc<Config>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            match get_due_schedules(&db).await {
                Ok(tasks) => {
                    for task in tasks {
                        // Spawn each task execution separately so the poll loop is never blocked
                        let db = db.clone();
                        let bot = bot.clone();
                        let config = config.clone();
                        tokio::spawn(async move {
                            execute_scheduled_task(db, bot, config, task).await;
                        });
                    }
                }
                Err(e) => {
                    log::error!("Scheduler poll error: {e}");
                }
            }
        }
    })
}

/// Execute a single scheduled task: run campaign, send result, advance schedule.
async fn execute_scheduled_task(
    db: Arc<Connection>,
    bot: Bot,
    config: Arc<Config>,
    task: ScheduledTask,
) {
    let chat_id = ChatId(task.chat_id.parse::<i64>().unwrap_or(0));

    // Send typing indicator
    let _ = bot.send_chat_action(chat_id, ChatAction::Typing).await;

    // Create MiniMax client and run campaign
    let result = {
        let (client, model_name) = crate::agent::client::create_minimax_client();
        let model = rig::prelude::CompletionClient::completion_model(&client, &model_name);
        crate::agent::run_campaign(model, config, db.clone(), &task.prompt).await
    };

    match result {
        Ok(response) => {
            // Send result to originating chat
            let html = format!(
                "<b>Scheduled Task Complete</b>\n\
                 <code>{}</code>\n\n{}",
                escape_html(&task.schedule),
                escape_html(&response),
            );
            let _ = send_chunked(&bot, chat_id, &html).await;

            // Advance schedule with success summary (truncate for last_result)
            let summary = if response.len() > 200 {
                format!("{}...", &response[..200])
            } else {
                response
            };
            let _ = advance_schedule(&db, task.id, summary).await;
        }
        Err(e) => {
            // Send error to originating chat
            let html = format!(
                "<b>Scheduled Task Failed</b>\n\
                 <code>{}</code>\n\nError: {}",
                escape_html(&task.schedule),
                escape_html(&e.to_string()),
            );
            let _ = send_chunked(&bot, chat_id, &html).await;

            // Advance schedule with error (next_run advances regardless per CONTEXT.md)
            let _ = advance_schedule(&db, task.id, format!("ERROR: {}", e)).await;
        }
    }
}
