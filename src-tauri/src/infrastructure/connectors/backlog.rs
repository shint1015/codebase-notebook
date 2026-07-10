use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SecretStore, Tool, ToolSpec};

use super::{connector_client, require_token};

/// Create a Backlog issue. The token is stored as "SPACE_URL|API_KEY" because
/// Backlog is per-space (e.g. "https://your.backlog.com|abcdef...").
pub struct BacklogCreateIssueTool {
    secrets: Arc<dyn SecretStore>,
}

impl BacklogCreateIssueTool {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

#[derive(Deserialize)]
struct BacklogResponse {
    #[serde(default)]
    id: Option<i64>,
    #[serde(rename = "issueKey", default)]
    issue_key: Option<String>,
    #[serde(default)]
    errors: Option<serde_json::Value>,
}

#[async_trait]
impl Tool for BacklogCreateIssueTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "backlog_create_issue".into(),
            description: "Create a Backlog issue. `project_id` and `issue_type_id` are numeric \
                          Backlog ids; `priority_id` defaults to 3 (normal). Requires approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "project_id": {"type": "integer", "description": "Backlog project id."},
                    "issue_type_id": {"type": "integer", "description": "Issue type id."},
                    "summary": {"type": "string", "description": "Issue summary."},
                    "description": {"type": "string", "description": "Issue description."},
                    "priority_id": {"type": "integer", "description": "Priority id (default 3)."}
                },
                "required": ["project_id", "issue_type_id", "summary"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Create Backlog issue \"{}\"",
            arguments.get("summary").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let raw = require_token(&self.secrets, "backlog")?;
        let (space_url, api_key) = raw.split_once('|').ok_or_else(|| {
            DomainError::ProviderNotConfigured(
                "backlog token must be \"https://your.backlog.com|API_KEY\"".into(),
            )
        })?;
        let project_id = arguments
            .get("project_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::Validation("missing project_id".into()))?;
        let issue_type_id = arguments
            .get("issue_type_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| DomainError::Validation("missing issue_type_id".into()))?;
        let summary = arguments
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing summary".into()))?;
        let description = arguments.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let priority_id = arguments.get("priority_id").and_then(|v| v.as_i64()).unwrap_or(3);

        let url = format!(
            "{}/api/v2/issues?apiKey={}",
            space_url.trim_end_matches('/'),
            api_key
        );
        let response = connector_client()
            .post(url)
            .form(&[
                ("projectId", project_id.to_string()),
                ("issueTypeId", issue_type_id.to_string()),
                ("priorityId", priority_id.to_string()),
                ("summary", summary.to_string()),
                ("description", description.to_string()),
            ])
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("backlog request: {e}")))?;
        let status = response.status();
        let parsed: BacklogResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("backlog response: {e}")))?;
        if status.is_success() && parsed.id.is_some() {
            Ok(format!(
                "Created Backlog issue {}",
                parsed.issue_key.unwrap_or_else(|| parsed.id.unwrap().to_string())
            ))
        } else {
            Err(DomainError::Provider(format!(
                "backlog error ({status}): {}",
                parsed.errors.map(|e| e.to_string()).unwrap_or_default()
            )))
        }
    }
}
