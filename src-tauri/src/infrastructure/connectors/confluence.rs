use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SecretStore, Tool, ToolSpec};

use super::{connector_client, require_token};

/// Create a Confluence Cloud page. The token is stored as
/// "BASE_URL|email:api_token" (Atlassian Cloud uses Basic auth).
pub struct ConfluenceCreatePageTool {
    secrets: Arc<dyn SecretStore>,
}

impl ConfluenceCreatePageTool {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

#[derive(Deserialize)]
struct ConfluenceResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "_links", default)]
    links: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
}

#[async_trait]
impl Tool for ConfluenceCreatePageTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "confluence_create_page".into(),
            description: "Create a Confluence page in a space. `space_key` is the space key \
                          (e.g. ENG). Requires user approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "space_key": {"type": "string", "description": "Confluence space key."},
                    "title": {"type": "string", "description": "Page title."},
                    "content": {"type": "string", "description": "Page body (HTML/storage)."}
                },
                "required": ["space_key", "title"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Create Confluence page \"{}\"",
            arguments.get("title").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let raw = require_token(&self.secrets, "confluence")?;
        let (base_url, basic) = raw.split_once('|').ok_or_else(|| {
            DomainError::ProviderNotConfigured(
                "confluence token must be \"https://you.atlassian.net/wiki|email:api_token\"".into(),
            )
        })?;
        let space_key = arguments
            .get("space_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing space_key".into()))?;
        let title = arguments
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing title".into()))?;
        let content = arguments.get("content").and_then(|v| v.as_str()).unwrap_or("");

        use base64::Engine;
        let auth = base64::engine::general_purpose::STANDARD.encode(basic);
        let url = format!("{}/rest/api/content", base_url.trim_end_matches('/'));
        let response = connector_client()
            .post(url)
            .header("Authorization", format!("Basic {auth}"))
            .json(&json!({
                "type": "page",
                "title": title,
                "space": {"key": space_key},
                "body": {"storage": {"value": content, "representation": "storage"}}
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("confluence request: {e}")))?;
        let status = response.status();
        let parsed: ConfluenceResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("confluence response: {e}")))?;
        if status.is_success() && parsed.id.is_some() {
            let link = parsed
                .links
                .and_then(|l| l.get("base").and_then(|b| b.as_str()).map(String::from))
                .unwrap_or_else(|| format!("id {}", parsed.id.unwrap()));
            Ok(format!("Created Confluence page: {link}"))
        } else {
            Err(DomainError::Provider(format!(
                "confluence error ({status}): {}",
                parsed.message.unwrap_or_default()
            )))
        }
    }
}
