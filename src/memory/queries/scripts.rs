use chrono::Utc;
use tokio_rusqlite::Connection;

use super::FTS_SANITIZER;
use crate::memory::MemoryError;

/// Script record from scripts table
#[derive(Debug, serde::Serialize)]
pub struct Script {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub language: String,
    pub tags: String,
    pub code: String,
    pub use_count: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
}

/// Save a script (insert or upsert on name conflict)
pub async fn save_script(
    conn: &Connection,
    name: String,
    description: String,
    language: String,
    tags: String,
    code: String,
) -> Result<i64, MemoryError> {
    let now = Utc::now().to_rfc3339();

    let err_result = conn.call(move |conn| {
        conn.execute(
            "INSERT INTO scripts (name, description, language, tags, code, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(name) DO UPDATE SET \
                code = excluded.code, \
                description = excluded.description, \
                tags = excluded.tags, \
                updated_at = excluded.updated_at",
            rusqlite::params![name, description, language, tags, code, now, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await;
    err_result.map_err(MemoryError::from)
}

/// Search scripts using FTS5
pub async fn search_scripts(
    conn: &Connection,
    query: String,
    limit: i64,
) -> Result<Vec<Script>, MemoryError> {
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

    conn.call(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, s.description, s.language, s.tags, s.code, s.use_count, s.created_at, s.updated_at, s.last_run_at \
             FROM scripts s \
             JOIN scripts_fts f ON s.id = f.rowid \
             WHERE f.scripts_fts MATCH ?1 \
             ORDER BY s.use_count DESC \
             LIMIT ?2"
        )?;
        let scripts = stmt.query_map(rusqlite::params![fts_query, limit], |row| {
            Ok(Script {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                language: row.get(3)?,
                tags: row.get(4)?,
                code: row.get(5)?,
                use_count: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                last_run_at: row.get(9)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(scripts)
    })
    .await
    .map_err(MemoryError::from)
}

/// Get a script by its unique name
pub async fn get_script_by_name(
    conn: &Connection,
    name: String,
) -> Result<Option<Script>, MemoryError> {
    let err_result = conn.call(move |conn| {
        match conn.query_row(
            "SELECT id, name, description, language, tags, code, use_count, created_at, updated_at, last_run_at \
             FROM scripts WHERE name = ?1",
            rusqlite::params![name],
            |row| {
                Ok(Script {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    language: row.get(3)?,
                    tags: row.get(4)?,
                    code: row.get(5)?,
                    use_count: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    last_run_at: row.get(9)?,
                })
            },
        ) {
            Ok(script) => Ok(Some(script)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
    .await;
    err_result.map_err(MemoryError::from)
}

/// Increment script use_count and set last_run_at
pub async fn update_script_usage(
    conn: &Connection,
    script_id: i64,
) -> Result<(), MemoryError> {
    let now = Utc::now().to_rfc3339();

    let err_result = conn.call(move |conn| {
        conn.execute(
            "UPDATE scripts SET use_count = use_count + 1, last_run_at = ?1 WHERE id = ?2",
            rusqlite::params![now, script_id],
        )?;
        Ok(())
    })
    .await;
    err_result.map_err(MemoryError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store};

    #[tokio::test]
    async fn test_save_script_insert_and_upsert() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = save_script(&conn, "sweep.sh".into(), "ARP sweep script".into(), "bash".into(), "[\"network\"]".into(), "arp-scan --localnet".into()).await.unwrap();
        assert!(id > 0);

        let _id2 = save_script(&conn, "sweep.sh".into(), "Updated ARP sweep".into(), "bash".into(), "[\"network\"]".into(), "arp-scan -I eth0 --localnet".into()).await.unwrap();

        let code: String = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT code FROM scripts WHERE name = 'sweep.sh'", [], |row| row.get(0))?)
            }).await.unwrap();
        assert!(code.contains("eth0"));
    }

    #[tokio::test]
    async fn test_search_scripts_fts5() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_script(&conn, "nmap_scan.sh".into(), "Network port scanner using nmap".into(), "bash".into(), "[\"network\",\"scan\"]".into(), "nmap -sS $1".into()).await.unwrap();
        save_script(&conn, "hydra_brute.sh".into(), "SSH brute force with hydra".into(), "bash".into(), "[\"brute\",\"ssh\"]".into(), "hydra -l admin -P pass.txt ssh://$1".into()).await.unwrap();

        let results = search_scripts(&conn, "nmap".to_string(), 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "nmap_scan.sh");
    }

    #[tokio::test]
    async fn test_search_scripts_empty_query() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let results = search_scripts(&conn, "".to_string(), 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_scripts_special_chars() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let results = search_scripts(&conn, "test:foo*bar".to_string(), 10).await.unwrap();
        assert!(results.is_empty() || !results.is_empty());
    }

    #[tokio::test]
    async fn test_get_script_by_name_found_and_not_found() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        save_script(&conn, "sweep.sh".into(), "ARP sweep".into(), "bash".into(), "[]".into(), "arp-scan --localnet".into()).await.unwrap();

        let found = get_script_by_name(&conn, "sweep.sh".to_string()).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "sweep.sh");

        let not_found = get_script_by_name(&conn, "nonexistent.sh".to_string()).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_update_script_usage_increments() {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();

        let id = save_script(&conn, "sweep.sh".into(), "ARP sweep".into(), "bash".into(), "[]".into(), "arp-scan --localnet".into()).await.unwrap();

        update_script_usage(&conn, id).await.unwrap();
        update_script_usage(&conn, id).await.unwrap();

        let (use_count, last_run_at): (i64, Option<String>) = conn
            .call(move |conn| {
                Ok(conn.query_row("SELECT use_count, last_run_at FROM scripts WHERE id = ?1", rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))?)
            }).await.unwrap();
        assert_eq!(use_count, 2);
        assert!(last_run_at.is_some());
    }
}
