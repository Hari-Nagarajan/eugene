use chrono::Utc;
use tokio_rusqlite::Connection;

use super::FTS_SANITIZER;
use crate::memory::MemoryError;

/// Memory record structure
#[derive(Debug)]
pub struct Memory {
    pub id: i64,
    pub chat_id: String,
    pub topic_key: Option<String>,
    pub content: String,
    pub sector: String,
    pub salience: f64,
    pub created_at: i64,
    pub accessed_at: i64,
}

/// Save a memory and return its ID
pub async fn save_memory(
    conn: &Connection,
    chat_id: String,
    content: String,
    sector: String,
) -> Result<i64, MemoryError> {
    if sector != "semantic" && sector != "episodic" {
        return Err(MemoryError::Query(format!(
            "Invalid sector '{}', must be 'semantic' or 'episodic'",
            sector
        )));
    }

    let now = Utc::now().timestamp();
    let salience = 1.0;

    conn.call(move |conn| {
        conn.execute(
            "INSERT INTO memories (chat_id, content, sector, salience, created_at, accessed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![chat_id, content, sector, salience, now, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .map_err(MemoryError::from)
}

/// Search memories using FTS5 with query sanitization and salience boosting
pub async fn search_memories(
    conn: &Connection,
    chat_id: String,
    query: String,
    limit: i64,
) -> Result<Vec<Memory>, MemoryError> {
    let safe_query = FTS_SANITIZER.replace_all(&query, " ");
    let words: Vec<&str> = safe_query.split_whitespace().collect();

    if words.is_empty() {
        return Ok(Vec::new());
    }

    let fts_query = words
        .iter()
        .map(|w| format!("{}*", w))
        .collect::<Vec<_>>()
        .join(" OR ");

    let now = Utc::now().timestamp();

    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT m.* FROM memories m
             JOIN memories_fts f ON m.id = f.rowid
             WHERE f.memories_fts MATCH ?1 AND m.chat_id = ?2
             ORDER BY m.salience DESC
             LIMIT ?3"
        )?;

        let memories = stmt.query_map(
            rusqlite::params![fts_query, chat_id, limit],
            |row| {
                Ok(Memory {
                    id: row.get(0)?,
                    chat_id: row.get(1)?,
                    topic_key: row.get(2)?,
                    content: row.get(3)?,
                    sector: row.get(4)?,
                    salience: row.get(5)?,
                    created_at: row.get(6)?,
                    accessed_at: row.get(7)?,
                })
            }
        )?
        .collect::<Result<Vec<_>, _>>()?;

        // Reinforce accessed memories: salience boost (capped at 5.0) and update accessed_at
        if !memories.is_empty() {
            let ids: Vec<i64> = memories.iter().map(|m| m.id).collect();
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let update_query = format!(
                "UPDATE memories SET accessed_at = ?1, salience = MIN(salience + 0.1, 5.0) WHERE id IN ({})",
                placeholders
            );

            let mut params = vec![now];
            params.extend(ids.iter());

            conn.execute(&update_query, rusqlite::params_from_iter(params))?;
        }

        Ok(memories)
    })
    .await
    .map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_crud_operations() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let memory_id = save_memory(&conn, "test_chat".into(), "Test memory content".into(), "semantic".into()).await.unwrap();
        assert!(memory_id > 0);

        let (salience, sector): (f64, String) = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT salience, sector FROM memories WHERE id = ?1", rusqlite::params![memory_id], |row| Ok((row.get(0)?, row.get(1)?)))?)
            }).await.unwrap();
        assert_eq!(salience, 1.0);
        assert_eq!(sector, "semantic");

        let result = save_memory(&conn, "test_chat".into(), "content".into(), "invalid".into()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fts5_search() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_memory(&conn, "test_chat".into(), "Network reconnaissance using nmap".into(), "episodic".into()).await.unwrap();
        save_memory(&conn, "test_chat".into(), "Found open port 22 on target host".into(), "episodic".into()).await.unwrap();
        save_memory(&conn, "test_chat".into(), "SQL injection vulnerability detected".into(), "episodic".into()).await.unwrap();
        save_memory(&conn, "other_chat".into(), "Network reconnaissance data".into(), "episodic".into()).await.unwrap();

        let results = search_memories(&conn, "test_chat".into(), "network".into(), 10).await.unwrap();
        assert_eq!(results.len(), 1);

        let results = search_memories(&conn, "test_chat".into(), "port injection".into(), 10).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = search_memories(&conn, "test_chat".into(), "host:192.168".into(), 10).await.unwrap();
        assert_eq!(results.len(), 1);

        let results = search_memories(&conn, "test_chat".into(), "".into(), 10).await.unwrap();
        assert_eq!(results.len(), 0);

        let results = search_memories(&conn, "other_chat".into(), "reconnaissance".into(), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        let first_salience = results[0].salience;

        let results = search_memories(&conn, "other_chat".into(), "reconnaissance".into(), 10).await.unwrap();
        assert!(results[0].salience > first_salience);
        assert!(results[0].salience <= 5.0);

        let results = search_memories(&conn, "test_chat".into(), "test".into(), 1).await.unwrap();
        assert!(results.len() <= 1);
    }
}
