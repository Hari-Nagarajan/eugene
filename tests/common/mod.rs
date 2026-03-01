use std::sync::Arc;

use eugene::config::Config;
use eugene::memory::{create_run, init_schema, open_memory_store};
use tokio_rusqlite::Connection;

/// In-memory database with schema initialized.
#[allow(dead_code)]
pub async fn setup_db() -> Arc<Connection> {
    let db = open_memory_store(":memory:").await.unwrap();
    init_schema(&db).await.unwrap();
    db
}

/// In-memory database + default config.
#[allow(dead_code)]
pub async fn setup_env() -> (Arc<Config>, Arc<Connection>) {
    let db = setup_db().await;
    let config = Arc::new(Config::default());
    (config, db)
}

/// In-memory database + default config + a run record.
#[allow(dead_code)]
pub async fn setup_with_run() -> (Arc<Config>, Arc<Connection>, i64) {
    let (config, db) = setup_env().await;
    let run_id = create_run(&db, "test".to_string(), None).await.unwrap();
    (config, db, run_id)
}
