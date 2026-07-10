use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SecretStore, Tool, ToolSpec};

use super::{connector_client, require_token};

const NOTION_VERSION: &str = "2022-06-28";

/// Create a page under a parent Notion page. The integration token must be
/// shared with the parent page. Stored in the keychain.
pub struct NotionCreatePageTool {
    secrets: Arc<dyn SecretStore>,
}

impl NotionCreatePageTool {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

#[derive(Deserialize)]
struct NotionResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[async_trait]
impl Tool for NotionCreatePageTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "notion_create_page".into(),
            description: "Create a Notion page under a parent page. `parent_page_id` is the \
                          32-char id of a page shared with the integration. Requires approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "parent_page_id": {"type": "string", "description": "Parent page id."},
                    "title": {"type": "string", "description": "New page title."},
                    "content": {"type": "string", "description": "Body text (plain/markdown)."}
                },
                "required": ["parent_page_id", "title"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Create Notion page \"{}\"",
            arguments.get("title").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let token = require_token(&self.secrets, "notion")?;
        let parent = arguments
            .get("parent_page_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing parent_page_id".into()))?;
        let title = arguments
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing title".into()))?;
        let content = arguments.get("content").and_then(|v| v.as_str()).unwrap_or("");

        let mut children = Vec::new();
        if !content.is_empty() {
            children.push(json!({
                "object": "block",
                "type": "paragraph",
                "paragraph": {
                    "rich_text": [{"type": "text", "text": {"content": content}}]
                }
            }));
        }
        let response = connector_client()
            .post("https://api.notion.com/v1/pages")
            .bearer_auth(token)
            .header("Notion-Version", NOTION_VERSION)
            .json(&json!({
                "parent": {"page_id": parent},
                "properties": {
                    "title": {"title": [{"type": "text", "text": {"content": title}}]}
                },
                "children": children,
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("notion request: {e}")))?;
        let status = response.status();
        let parsed: NotionResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("notion response: {e}")))?;
        if status.is_success() && parsed.id.is_some() {
            Ok(format!(
                "Created Notion page: {}",
                parsed.url.unwrap_or_else(|| parsed.id.unwrap_or_default())
            ))
        } else {
            Err(DomainError::Provider(format!(
                "notion error: {}",
                parsed.message.unwrap_or_else(|| status.to_string())
            )))
        }
    }
}
