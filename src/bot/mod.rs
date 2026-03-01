pub mod commands;
pub mod formatting;
pub mod handlers;
pub mod session;

use std::collections::HashSet;
use std::sync::Arc;

use teloxide::prelude::*;
use tokio_rusqlite::Connection;

use crate::config::Config;

/// Shared state injected into all bot handlers via dptree dependencies.
#[derive(Clone)]
pub struct BotState {
    pub allowed_chats: Arc<HashSet<i64>>,
    pub db: Arc<Connection>,
    pub config: Arc<Config>,
}

/// Start the Telegram bot with dptree dispatch, allow-list filter, and scheduler.
///
/// This is the main entry point for `eugene bot`. It:
/// 1. Creates the Bot from TELEGRAM_BOT_TOKEN (not from env default)
/// 2. Sets default HTML parse mode
/// 3. Builds BotState with allow-list, DB, and config
/// 4. Spawns the background scheduler
/// 5. Runs the dptree dispatcher with allow-list gating
pub async fn start_bot(config: Arc<Config>, db: Arc<Connection>) -> Result<(), anyhow::Error> {
    // Extract token (error if not configured)
    let token = config
        .telegram_bot_token
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TELEGRAM_BOT_TOKEN not set"))?;

    // Create bot with explicit token (NOT Bot::from_env which reads TELOXIDE_TOKEN)
    let bot = Bot::new(token);

    // Build allowed chat_ids set
    let allowed_chats = Arc::new(
        config
            .allowed_chat_ids
            .iter()
            .copied()
            .collect::<HashSet<i64>>(),
    );

    let bot_state = BotState {
        allowed_chats,
        db: db.clone(),
        config: config.clone(),
    };

    // Spawn the scheduler background task (uses raw Bot)
    crate::scheduler::spawn_scheduler(db, bot.clone(), config);

    // Build dptree handler with allow-list filter
    let handler = Update::filter_message()
        .branch(
            dptree::filter(|msg: Message, state: BotState| {
                state.allowed_chats.contains(&msg.chat.id.0)
            })
            .branch(
                dptree::entry()
                    .filter_command::<commands::Command>()
                    .endpoint(commands::handle_command),
            )
            .branch(dptree::entry().endpoint(handlers::handle_message)),
        );

    // Build and dispatch
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![bot_state])
        .build()
        .dispatch()
        .await;

    Ok(())
}
