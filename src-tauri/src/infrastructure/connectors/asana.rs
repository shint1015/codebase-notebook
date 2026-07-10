use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{SecretStore, Tool, ToolSpec};

use super::{connector_client, require_token};

/// Create an Asana task in a project. Uses a personal access token.
pub struct AsanaCreateTaskTool {
    secrets: Arc<dyn SecretStore>,
}

impl AsanaCreateTaskTool {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

#[derive(Deserialize)]
struct AsanaResponse {
    #[serde(default)]
    data: Option<AsanaData>,
    #[serde(default)]
    errors: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AsanaData {
    #[serde(default)]
    gid: Option<String>,
    #[serde(default)]
    permalink_url: Option<String>,
}

#[async_trait]
impl Tool for AsanaCreateTaskTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "asana_create_task".into(),
            description: "Create an Asana task in a project. `project_gid` is the project id. \
                          Requires user approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "project_gid": {"type": "string", "description": "Asana project gid."},
                    "name": {"type": "string", "description": "Task name."},
                    "notes": {"type": "string", "description": "Task description."}
                },
                "required": ["project_gid", "name"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Create Asana task \"{}\"",
            arguments.get("name").and_then(|v| v.as_str()).unwrap_or("")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let token = require_token(&self.secrets, "asana")?;
        let project = arguments
            .get("project_gid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing project_gid".into()))?;
        let name = arguments
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DomainError::Validation("missing name".into()))?;
        let notes = arguments.get("notes").and_then(|v| v.as_str()).unwrap_or("");

        let response = connector_client()
            .post("https://app.asana.com/api/1.0/tasks")
            .bearer_auth(token)
            .json(&json!({
                "data": {"name": name, "notes": notes, "projects": [project]}
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("asana request: {e}")))?;
        let status = response.status();
        let parsed: AsanaResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("asana response: {e}")))?;
        match parsed.data.and_then(|d| d.permalink_url.or(d.gid)) {
            Some(reference) if status.is_success() => Ok(format!("Created Asana task: {reference}")),
            _ => Err(DomainError::Provider(format!(
                "asana error ({status}): {}",
                parsed.errors.map(|e| e.to_string()).unwrap_or_default()
            ))),
        }
    }
}
