use rig::tool::Tool;
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio_rusqlite::Connection;

use crate::memory::search_scripts;
use crate::tools::ToolError;

/// Arguments for the search_scripts tool
#[derive(Deserialize)]
pub struct SearchScriptsArgs {
    /// Search query string (searches names, descriptions, and tags)
    pub query: String,
    /// Maximum number of results to return. Defaults to 10 if omitted.
    pub limit: Option<i64>,
}

/// Concise script summary without code content
#[derive(Debug, Serialize)]
pub struct ScriptSummary {
    /// Script name
    pub name: String,
    /// Script description
    pub description: String,
    /// Script language (bash or python)
    pub language: String,
    /// Number of times the script has been executed
    pub use_count: i64,
}

/// Structured result from searching scripts
#[derive(Debug, Serialize)]
pub struct SearchScriptsResult {
    /// Matching scripts (concise view without code)
    pub scripts: Vec<ScriptSummary>,
    /// Number of results returned
    pub count: usize,
}

/// Tool for searching saved scripts by keyword.
///
/// Uses FTS5 full-text search across script names, descriptions, and tags.
/// Returns concise summaries without the full code content.
pub struct SearchScriptsTool {
    memory: Arc<Connection>,
}

impl SearchScriptsTool {
    pub fn new(memory: Arc<Connection>) -> Self {
        Self { memory }
    }
}

impl Tool for SearchScriptsTool {
    const NAME: &'static str = "search_scripts";

    type Error = ToolError;
    type Args = SearchScriptsArgs;
    type Output = SearchScriptsResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_scripts".to_string(),
            description: "Search for saved scripts by keyword. Searches script names, \
                descriptions, and tags."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query string"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 10)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let limit = args.limit.unwrap_or(10);

        let scripts = search_scripts(&self.memory, args.query, limit).await?;

        let summaries: Vec<ScriptSummary> = scripts
            .into_iter()
            .map(|s| ScriptSummary {
                name: s.name,
                description: s.description,
                language: s.language,
                use_count: s.use_count,
            })
            .collect();

        let count = summaries.len();

        Ok(SearchScriptsResult {
            scripts: summaries,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{init_schema, open_memory_store, save_script};

    async fn setup_tool() -> SearchScriptsTool {
        let conn = open_memory_store(":memory:").await.unwrap();
        init_schema(&conn).await.unwrap();
        SearchScriptsTool::new(conn)
    }

    #[tokio::test]
    async fn test_search_scripts_finds_match() {
        let tool = setup_tool().await;

        // Save a script directly
        save_script(
            &tool.memory,
            "nmap_scan.sh".to_string(),
            "Network port scanner using nmap".to_string(),
            "bash".to_string(),
            "[\"network\",\"scan\"]".to_string(),
            "nmap -sS $1".to_string(),
        )
        .await
        .unwrap();

        let result = tool
            .call(SearchScriptsArgs {
                query: "nmap".to_string(),
                limit: Some(5),
            })
            .await
            .unwrap();

        assert_eq!(result.count, 1, "Should find 1 matching script");
        assert_eq!(result.scripts[0].name, "nmap_scan.sh");
        assert_eq!(result.scripts[0].language, "bash");
    }

    #[tokio::test]
    async fn test_search_scripts_empty_query() {
        let tool = setup_tool().await;

        let result = tool
            .call(SearchScriptsArgs {
                query: "".to_string(),
                limit: None,
            })
            .await
            .unwrap();

        assert_eq!(result.count, 0, "Empty query should return no results");
        assert!(result.scripts.is_empty());
    }
}
