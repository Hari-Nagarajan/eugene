use eugene::memory::{init_schema, open_memory_store, MemoryError};
use std::fs;

#[tokio::test]
async fn test_schema_initialization() -> Result<(), MemoryError> {
    // Create test database
    let test_db = "test_schema.db";

    // Clean up if exists
    let _ = fs::remove_file(test_db);
    let _ = fs::remove_file(format!("{}-shm", test_db));
    let _ = fs::remove_file(format!("{}-wal", test_db));

    // Open connection and initialize schema
    let conn = open_memory_store(test_db).await?;
    init_schema(&conn).await?;

    // Verify journal mode is WAL
    let journal_mode: String = conn
        .call(|conn| {
            let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
            Ok(mode)
        })
        .await?;
    assert_eq!(journal_mode.to_lowercase(), "wal");

    // Verify 10 tables exist
    let table_count: i64 = conn
        .call(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_fts5_%' AND name NOT LIKE 'memories_fts%'",
                [],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await?;
    assert_eq!(table_count, 10, "Expected 10 tables");

    // Verify memories_fts virtual table exists
    let fts_exists: i64 = conn
        .call(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
                [],
                |row| row.get(0),
            )?;
            Ok(exists)
        })
        .await?;
    assert_eq!(fts_exists, 1, "memories_fts virtual table should exist");

    // Verify 3 triggers exist for memories
    let trigger_count: i64 = conn
        .call(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'memories_a%'",
                [],
                |row| row.get(0),
            )?;
            Ok(count)
        })
        .await?;
    assert_eq!(trigger_count, 3, "Expected 3 FTS5 sync triggers");

    // Clean up
    drop(conn);
    let _ = fs::remove_file(test_db);
    let _ = fs::remove_file(format!("{}-shm", test_db));
    let _ = fs::remove_file(format!("{}-wal", test_db));

    Ok(())
}
