mod errors;
pub use errors::MemoryError;

use std::sync::Arc;
use tokio_rusqlite::Connection;

// Module will contain MemoryStore struct and functions
