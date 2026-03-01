use rig::completion::Message;
use tokio_rusqlite::Connection;

/// Maximum number of messages to keep in session history
const MAX_HISTORY_MESSAGES: usize = 50;

/// Load chat history from the sessions table, deserializing JSON to rig Messages.
///
/// Returns an empty vec if:
/// - No session exists for this chat_id
/// - The stored JSON is malformed
///
/// History is capped at MAX_HISTORY_MESSAGES to prevent unbounded growth.
pub async fn load_chat_history(db: &Connection, chat_id: &str) -> Vec<Message> {
    let json = match crate::memory::load_session(db, chat_id.to_string()).await {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };

    let messages: Vec<Message> = serde_json::from_str(&json).unwrap_or_default();

    // Cap at last MAX_HISTORY_MESSAGES
    if messages.len() > MAX_HISTORY_MESSAGES {
        messages[messages.len() - MAX_HISTORY_MESSAGES..].to_vec()
    } else {
        messages
    }
}

/// Save chat history after a conversation turn.
///
/// Appends the user message and assistant response to the existing history,
/// trims to MAX_HISTORY_MESSAGES, and persists the JSON.
///
/// Note: rig's chat() takes history by value and the prompt separately.
/// After calling chat(), we manually append user + assistant messages
/// (chat() does not return updated history to the caller).
pub async fn save_chat_history(
    db: &Connection,
    chat_id: &str,
    user_msg: &str,
    assistant_msg: &str,
    existing_history: &[Message],
) {
    let mut history = existing_history.to_vec();
    history.push(Message::user(user_msg));
    history.push(Message::assistant(assistant_msg));

    // Trim to last MAX_HISTORY_MESSAGES if exceeds limit
    if history.len() > MAX_HISTORY_MESSAGES {
        history = history[history.len() - MAX_HISTORY_MESSAGES..].to_vec();
    }

    let json = serde_json::to_string(&history).unwrap_or_else(|_| "[]".to_string());
    let _ = crate::memory::save_session(db, chat_id.to_string(), json).await;
}
