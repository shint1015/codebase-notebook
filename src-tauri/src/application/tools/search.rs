use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::application::tools::require_str;
use crate::application::usecases::search::SearchUseCase;
use crate::domain::error::DomainResult;
use crate::domain::services::{Tool, ToolSpec};

/// Read tool: let the model search the workspace's indexed sources itself.
pub struct SearchSourcesTool {
    search: Arc<SearchUseCase>,
}

impl SearchSourcesTool {
    pub fn new(search: Arc<SearchUseCase>) -> Self {
        Self { search }
    }
}

#[async_trait]
impl Tool for SearchSourcesTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_sources".into(),
            description: "Search the current workspace's indexed code, docs and issues. \
                          Returns matching snippets with their file paths and line ranges. \
                          Use this to ground answers before replying."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for (keywords or a natural-language query)."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        false
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Search sources for \"{}\"",
            arguments.get("query").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let query = require_str(arguments, "query").map_err(crate::domain::error::DomainError::Validation)?;
        let hits = self.search.execute(workspace_id, &query, 8).await?;
        if hits.is_empty() {
            return Ok("No matching sources found.".into());
        }
        let mut out = String::new();
        for (i, hit) in hits.iter().enumerate() {
            let snippet: String = hit.chunk.content.chars().take(500).collect();
            out.push_str(&format!(
                "[{}] {} (lines {}-{})\n{}\n\n",
                i + 1,
                hit.rel_path,
                hit.chunk.start_line,
                hit.chunk.end_line,
                snippet
            ));
        }
        Ok(out)
    }
}
