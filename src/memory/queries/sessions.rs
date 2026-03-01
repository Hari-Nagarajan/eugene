use tokio_rusqlite::Connection;

use crate::memory::MemoryError;

/// Load session messages_json for a chat. Returns "[]" if no session exists.
pub async fn load_session(
    conn: &Connection,
    chat_id: String,
) -> Result<String, MemoryError> {
    conn.call(move |conn| {
        match conn.query_row(
            "SELECT messages_json FROM sessions WHERE chat_id = ?1",
            rusqlite::params![chat_id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(json) => Ok(json),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok("[]".to_string()),
            Err(e) => Err(e.into()),
        }
    })
    .await
    .map_err(MemoryError::from)
}

/// Save session messages_json (INSERT OR REPLACE / upsert)
pub async fn save_session(
    conn: &Connection,
    chat_id: String,
    messages_json: String,
) -> Result<(), MemoryError> {
    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO sessions (chat_id, messages_json, updated_at) \
             VALUES (?1, ?2, datetime('now')) \
             ON CONFLICT(chat_id) DO UPDATE SET messages_json = ?2, updated_at = datetime('now')",
            rusqlite::params![chat_id, messages_json],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

/// Clear session messages to "[]" but keep the row
pub async fn clear_session(
    conn: &Connection,
    chat_id: String,
) -> Result<(), MemoryError> {
    conn.call(move |conn| {
        conn.execute(
            "UPDATE sessions SET messages_json = '[]', updated_at = datetime('now') WHERE chat_id = ?1",
            rusqlite::params![chat_id],
        )?;
        Ok(())
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_session_load_empty_db_returns_empty_json() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let result = load_session(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(result, "[]");
    }

    #[tokio::test]
    async fn test_session_save_then_load() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let json = r#"[{"role":"user","content":"hello"}]"#.to_string();
        save_session(&conn, "chat_1".to_string(), json.clone()).await.unwrap();

        let loaded = load_session(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(loaded, json);
    }

    #[tokio::test]
    async fn test_session_save_upserts() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_session(&conn, "chat_1".to_string(), "[1]".to_string()).await.unwrap();
        save_session(&conn, "chat_1".to_string(), "[2]".to_string()).await.unwrap();

        let loaded = load_session(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(loaded, "[2]");
    }

    #[tokio::test]
    async fn test_session_clear() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_session(&conn, "chat_1".to_string(), "[1,2,3]".to_string()).await.unwrap();
        clear_session(&conn, "chat_1".to_string()).await.unwrap();

        let loaded = load_session(&conn, "chat_1".to_string()).await.unwrap();
        assert_eq!(loaded, "[]");
    }
}
