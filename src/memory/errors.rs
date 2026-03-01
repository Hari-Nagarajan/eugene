use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("FTS5 not available")]
    Fts5NotAvailable,

    #[error("Query error: {0}")]
    Query(String),
}
