use std::future::Future;
use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::ChatAction;
use teloxide::utils::command::BotCommands;

use super::formatting::{escape_html, format_findings, format_status, send_chunked};
use super::session;
use super::BotState;

use crate::agent::client::create_minimax_client;
use crate::agent::run_campaign;
use crate::memory::{clear_session, get_findings_by_host, get_run_summary};

/// Bot command enum derived with teloxide BotCommands
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    #[command(description = "Show help")]
    Start,
    #[command(description = "Run recon campaign")]
    Run(String),
    #[command(description = "Show last run status")]
    Status,
    #[command(description = "Query findings")]
    Findings(String),
    #[command(description = "Clear conversation history")]
    Newchat,
}

/// Run an async future while sending typing indicators every 5 seconds.
///
/// Spawns a background task that re-sends ChatAction::Typing every 5s
/// (Telegram typing status expires after 5s). Cancels when the future completes.
async fn run_with_typing<F, T>(bot: &Bot, chat_id: ChatId, future: F) -> T
where
    F: Future<Output = T>,
{
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    let bot_clone = bot.clone();
    let typing_handle = tokio::spawn(async move {
        loop {
            let _ = bot_clone
                .send_chat_action(chat_id, ChatAction::Typing)
                .await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                _ = cancel_rx.changed() => break,
            }
        }
    });
    let result = future.await;
    let _ = cancel_tx.send(true);
    typing_handle.abort();
    result
}

/// Handle all bot commands
pub async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: BotState,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;
    match cmd {
        Command::Start => handle_start(&bot, chat_id).await,
        Command::Run(prompt) => handle_run(&bot, chat_id, &state, prompt).await,
        Command::Status => handle_status(&bot, chat_id, &state).await,
        Command::Findings(host) => handle_findings(&bot, chat_id, &state, &host).await,
        Command::Newchat => handle_newchat(&bot, chat_id, &state).await,
    }
}

async fn handle_start(bot: &Bot, chat_id: ChatId) -> ResponseResult<()> {
    let help = Command::descriptions().to_string();
    bot.send_message(chat_id, format!("<b>Eugene Recon Agent</b>\n\n{}", escape_html(&help)))
        .parse_mode(teloxide::types::ParseMode::Html)
        .await?;
    Ok(())
}

async fn handle_run(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    prompt: String,
) -> ResponseResult<()> {
    let prompt = if prompt.trim().is_empty() {
        "scan the local network".to_string()
    } else {
        prompt
    };

    let chat_id_str = chat_id.0.to_string();
    let db = state.db.clone();
    let config = state.config.clone();

    let history = session::load_chat_history(&db, &chat_id_str).await;

    let prompt_clone = prompt.clone();
    let db_clone = db.clone();
    let result = run_with_typing(bot, chat_id, async move {
        let (client, model_name) = create_minimax_client()?;
        let model = rig::prelude::CompletionClient::completion_model(&client, &model_name);
        run_campaign(model, config, db_clone, Some(&prompt_clone)).await
    })
    .await;

    match result {
        Ok(response) => {
            session::save_chat_history(&db, &chat_id_str, &prompt, &response, &history).await;
            send_chunked(bot, chat_id, &escape_html(&response)).await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("<b>Error:</b> {}", escape_html(&e.to_string())))
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
    }
    Ok(())
}

async fn handle_status(bot: &Bot, chat_id: ChatId, state: &BotState) -> ResponseResult<()> {
    let db = state.db.clone();

    let run_id_result = db
        .call(|conn| {
            match conn.query_row(
                "SELECT id FROM runs ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            ) {
                Ok(id) => Ok(Some(id)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await;

    match run_id_result {
        Ok(Some(run_id)) => match get_run_summary(&db, run_id).await {
            Ok(summary) => {
                let html = format_status(&summary);
                send_chunked(bot, chat_id, &html).await?;
            }
            Err(e) => {
                bot.send_message(chat_id, format!("Error: {}", escape_html(&e.to_string())))
                    .await?;
            }
        },
        Ok(None) => {
            bot.send_message(chat_id, "No runs found.").await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("Error: {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

async fn handle_findings(
    bot: &Bot,
    chat_id: ChatId,
    state: &BotState,
    host: &str,
) -> ResponseResult<()> {
    let db = state.db.clone();

    let findings = if host.trim().is_empty() {
        db.call(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, run_id, host, finding_type, data, timestamp \
                 FROM findings ORDER BY timestamp DESC LIMIT 20",
            )?;
            let findings = stmt
                .query_map([], |row| {
                    Ok(crate::memory::Finding {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        host: row.get(2)?,
                        finding_type: row.get(3)?,
                        data: row.get(4)?,
                        timestamp: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(findings)
        })
        .await
    } else {
        get_findings_by_host(&db, host.trim().to_string())
            .await
            .map_err(|e| tokio_rusqlite::Error::Other(e.into()))
    };

    match findings {
        Ok(findings) => {
            let html = format_findings(&findings);
            send_chunked(bot, chat_id, &html).await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("Error: {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

async fn handle_newchat(bot: &Bot, chat_id: ChatId, state: &BotState) -> ResponseResult<()> {
    let chat_id_str = chat_id.0.to_string();
    match clear_session(&state.db, chat_id_str).await {
        Ok(_) => {
            bot.send_message(chat_id, "Conversation history cleared. Data preserved.")
                .await?;
        }
        Err(e) => {
            bot.send_message(
                chat_id,
                format!("Error clearing session: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }
    Ok(())
}
