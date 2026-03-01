mod errors;
pub use errors::MemoryError;

use std::sync::Arc;
use tokio_rusqlite::Connection;

/// Open a memory store database connection with production pragmas
///
/// Configures the connection with:
/// - WAL mode for better concurrency
/// - NORMAL synchronous mode for performance
/// - 8MB memory-mapped I/O
/// - In-memory temp storage
/// - Foreign key constraints enabled
pub async fn open_memory_store(db_path: &str) -> Result<Arc<Connection>, MemoryError> {
    let conn = Connection::open(db_path)
        .await
        .map_err(|e| MemoryError::Connection(e.to_string()))?;

    // Apply production pragmas in single call
    conn.call(|conn| {
        // WAL mode for better concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // NORMAL sync: less paranoid than FULL, much faster
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // 8MB memory-mapped I/O
        conn.pragma_update(None, "mmap_size", 8388608)?;

        // Temp tables in memory
        conn.pragma_update(None, "temp_store", "MEMORY")?;

        // Enable foreign key constraints
        conn.pragma_update(None, "foreign_keys", "ON")?;

        Ok(())
    })
    .await?;

    Ok(Arc::new(conn))
}

/// Initialize database schema with FTS5 support check
///
/// Creates all tables from schema.sql if they don't exist.
/// Checks for FTS5 availability before creating FTS5 tables.
pub async fn init_schema(conn: &Connection) -> Result<(), MemoryError> {
    // Check if FTS5 is available by attempting to create a probe table
    let has_fts5 = conn
        .call(|conn| {
            match conn.execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS _fts5_probe USING fts5(x)",
                [],
            ) {
                Ok(_) => {
                    // Clean up probe table
                    conn.execute("DROP TABLE IF EXISTS _fts5_probe", [])?;
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        })
        .await?;

    if !has_fts5 {
        return Err(MemoryError::Fts5NotAvailable);
    }

    // Load and execute schema
    let schema = include_str!("schema.sql");

    conn.call(move |conn| {
        conn.execute_batch(schema)?;
        Ok(())
    })
    .await?;

    Ok(())
}
