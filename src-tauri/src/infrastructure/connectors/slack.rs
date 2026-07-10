use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SecretStore, Tool, ToolSpec};

use super::{connector_client, require_token};

/// Post a message to a Slack channel via the Web API (chat.postMessage).
/// Needs a bot token (xoxb-…) with chat:write, stored in the keychain.
pub struct SlackPostMessageTool {
    secrets: Arc<dyn SecretStore>,
}

impl SlackPostMessageTool {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    channel: Option<String>,
}

#[async_trait]
impl Tool for SlackPostMessageTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "slack_post_message".into(),
            description: "Post a message to a Slack channel. `channel` is a channel ID \
                          (e.g. C0123) or name (e.g. #general). Requires user approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "channel": {"type": "string", "description": "Channel ID or #name."},
                    "text": {"type": "string", "description": "Message text (markdown-ish)."}
                },
                "required": ["channel", "text"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Post to Slack {}",
            arguments.get("channel").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let token = require_token(&self.secrets, "slack")?;
        let channel = arguments
            .get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing channel".into()))?;
        let text = arguments
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing text".into()))?;
        let response = connector_client()
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(token)
            .json(&json!({"channel": channel, "text": text}))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("slack request: {e}")))?
            .json::<SlackResponse>()
            .await
            .map_err(|e| DomainError::Provider(format!("slack response: {e}")))?;
        if response.ok {
            Ok(format!(
                "Posted to Slack channel {} (ts {}).",
                response.channel.unwrap_or_default(),
                response.ts.unwrap_or_default()
            ))
        } else {
            Err(DomainError::Provider(format!(
                "slack error: {}",
                response.error.unwrap_or_else(|| "unknown".into())
            )))
        }
    }
}
