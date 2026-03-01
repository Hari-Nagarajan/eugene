use teloxide::prelude::*;
use teloxide::types::ParseMode;

use super::formatting::{escape_html, format_schedule_list, send_chunked};
use super::BotState;

use crate::memory::{
    create_schedule, delete_schedule, list_schedules, pause_schedule, resume_schedule,
};
use crate::scheduler::cron::validate_cron;

/// Handle free-text messages (non-command).
///
/// Parses /schedule subcommands from message text:
///   /schedule create <cron> <prompt>
///   /schedule list
///   /schedule delete <id>
///   /schedule pause <id>
///   /schedule resume <id>
///
/// Any other text is treated as a conversational prompt to the agent.
pub async fn handle_message(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let text = match msg.text() {
        Some(t) => t.to_string(),
        None => return Ok(()),
    };

    let chat_id = msg.chat.id;
    let chat_id_str = chat_id.0.to_string();

    // Check if this is a /schedule command (not caught by BotCommands because
    // schedule subcommands use free-text argument parsing)
    if text.starts_with("/schedule") {
        return handle_schedule_command(&bot, chat_id, &chat_id_str, &text, &state).await;
    }

    // Otherwise, treat as conversational chat with the agent
    handle_chat_message(&bot, chat_id, &chat_id_str, &text, &state).await
}

/// Handle /schedule subcommands
async fn handle_schedule_command(
    bot: &Bot,
    chat_id: ChatId,
    chat_id_str: &str,
    text: &str,
    state: &BotState,
) -> ResponseResult<()> {
    let parts: Vec<&str> = text.splitn(3, ' ').collect();
    let subcommand = parts.get(1).unwrap_or(&"help").trim();

    let id_arg = parts.get(2).unwrap_or(&"").trim();

    match subcommand {
        "create" => handle_schedule_create(bot, chat_id, chat_id_str, id_arg, state).await,
        "list" => handle_schedule_list(bot, chat_id, chat_id_str, state).await,
        "delete" | "pause" | "resume" => {
            handle_schedule_mutate(bot, chat_id, subcommand, id_arg, &state.db).await
        }
        _ => {
            bot.send_message(
                chat_id,
                "Schedule commands:\n\
                 /schedule create &lt;cron&gt; &lt;prompt&gt;\n\
                 /schedule list\n\
                 /schedule delete &lt;id&gt;\n\
                 /schedule pause &lt;id&gt;\n\
                 /schedule resume &lt;id&gt;",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            Ok(())
        }
    }
}

async fn handle_schedule_create(
    bot: &Bot,
    chat_id: ChatId,
    chat_id_str: &str,
    rest: &str,
    state: &BotState,
) -> ResponseResult<()> {
    if rest.is_empty() {
        bot.send_message(
            chat_id,
            "Usage: /schedule create &lt;cron&gt; &lt;prompt&gt;\n\
             Example: /schedule create 0 */6 * * * scan the network",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let tokens: Vec<&str> = rest.splitn(6, ' ').collect();
    if tokens.len() < 6 {
        bot.send_message(
            chat_id,
            "Need 5 cron fields + prompt.\n\
             Example: /schedule create 0 */6 * * * scan the network",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let cron_expr = format!(
        "{} {} {} {} {}",
        tokens[0], tokens[1], tokens[2], tokens[3], tokens[4]
    );
    let prompt = tokens[5].to_string();

    if let Err(e) = validate_cron(&cron_expr) {
        bot.send_message(
            chat_id,
            format!("Invalid cron expression: {}", escape_html(&e)),
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    match create_schedule(&state.db, chat_id_str.to_string(), cron_expr.clone(), prompt).await {
        Ok(id) => {
            bot.send_message(
                chat_id,
                format!(
                    "Schedule created: <code>{}</code>\nCron: <code>{}</code>",
                    escape_html(&id[..8.min(id.len())]),
                    escape_html(&cron_expr)
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            bot.send_message(
                chat_id,
                format!("Error creating schedule: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }
    Ok(())
}

async fn handle_schedule_list(
    bot: &Bot,
    chat_id: ChatId,
    chat_id_str: &str,
    state: &BotState,
) -> ResponseResult<()> {
    match list_schedules(&state.db, chat_id_str.to_string()).await {
        Ok(schedules) => {
            let html = format_schedule_list(&schedules);
            send_chunked(bot, chat_id, &html).await?;
        }
        Err(e) => {
            bot.send_message(
                chat_id,
                format!("Error listing schedules: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }
    Ok(())
}

/// Handle delete/pause/resume — all follow the same id + action pattern.
async fn handle_schedule_mutate(
    bot: &Bot,
    chat_id: ChatId,
    action: &str,
    id: &str,
    db: &tokio_rusqlite::Connection,
) -> ResponseResult<()> {
    if id.is_empty() {
        bot.send_message(chat_id, format!("Usage: /schedule {} &lt;id&gt;", action))
            .parse_mode(ParseMode::Html)
            .await?;
        return Ok(());
    }
    let id = id.to_string();
    let result = match action {
        "delete" => delete_schedule(db, id).await,
        "pause" => pause_schedule(db, id).await,
        _ => resume_schedule(db, id).await,
    };
    let past_tense = match action {
        "pause" => "paused",
        "resume" => "resumed",
        _ => "deleted",
    };
    match result {
        Ok(_) => {
            bot.send_message(chat_id, format!("Schedule {}.", past_tense))
                .await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("Error: {}", escape_html(&e.to_string())))
                .await?;
        }
    }
    Ok(())
}

/// Handle free-text conversational messages by running them through the agent
async fn handle_chat_message(
    bot: &Bot,
    chat_id: ChatId,
    chat_id_str: &str,
    text: &str,
    state: &BotState,
) -> ResponseResult<()> {
    use std::time::Duration;
    use teloxide::types::ChatAction;

    let db = state.db.clone();
    let config = state.config.clone();

    // Load session history
    let history = super::session::load_chat_history(&db, chat_id_str).await;

    // Typing indicator
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    let bot_clone = bot.clone();
    let typing_chat_id = chat_id;
    let typing_handle = tokio::spawn(async move {
        loop {
            let _ = bot_clone
                .send_chat_action(typing_chat_id, ChatAction::Typing)
                .await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                _ = cancel_rx.changed() => break,
            }
        }
    });

    // Create MiniMax client and run
    let text_owned = text.to_string();
    let result = match crate::agent::client::create_minimax_client() {
        Ok((client, model_name)) => {
            let model = rig::prelude::CompletionClient::completion_model(&client, &model_name);
            crate::agent::run_campaign(model, config, db.clone(), &text_owned).await
        }
        Err(e) => Err(e),
    };

    // Stop typing
    let _ = cancel_tx.send(true);
    typing_handle.abort();

    match result {
        Ok(response) => {
            super::session::save_chat_history(&db, chat_id_str, text, &response, &history).await;
            super::formatting::send_chunked(
                bot,
                chat_id,
                &super::formatting::escape_html(&response),
            )
            .await?;
        }
        Err(e) => {
            bot.send_message(
                chat_id,
                format!(
                    "<b>Error:</b> {}",
                    super::formatting::escape_html(&e.to_string())
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
    }

    Ok(())
}
