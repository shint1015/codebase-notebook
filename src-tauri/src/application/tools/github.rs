use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::application::tools::{optional_str, require_str};
use crate::application::usecases::publish::PublishUseCases;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::RepositoryRepository;
use crate::domain::services::{Tool, ToolSpec};

/// Write tool: create a GitHub issue via the user's gh credentials.
pub struct CreateGithubIssueTool {
    publish: Arc<PublishUseCases>,
}

impl CreateGithubIssueTool {
    pub fn new(publish: Arc<PublishUseCases>) -> Self {
        Self { publish }
    }
}

#[async_trait]
impl Tool for CreateGithubIssueTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "create_github_issue".into(),
            description: "Create a GitHub issue in a repository (owner/repo). Uses the user's \
                          authenticated gh CLI. Requires user approval before it runs."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "repo": {"type": "string", "description": "Target repository as owner/repo."},
                    "title": {"type": "string", "description": "Issue title."},
                    "body": {"type": "string", "description": "Issue body in markdown."}
                },
                "required": ["repo", "title"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!(
            "Create GitHub issue \"{}\" in {}",
            optional_str(arguments, "title"),
            optional_str(arguments, "repo")
        )
    }

    async fn execute(
        &self,
        _workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let repo = require_str(arguments, "repo").map_err(DomainError::Validation)?;
        let title = require_str(arguments, "title").map_err(DomainError::Validation)?;
        let body = optional_str(arguments, "body");
        let url = self.publish.create_issue(&repo, &title, &body).await?;
        Ok(format!("Created issue: {url}"))
    }
}

/// Write tool: create or update a page in a cloned wiki of the workspace.
pub struct WriteWikiPageTool {
    publish: Arc<PublishUseCases>,
    repositories: Arc<dyn RepositoryRepository>,
}

impl WriteWikiPageTool {
    pub fn new(
        publish: Arc<PublishUseCases>,
        repositories: Arc<dyn RepositoryRepository>,
    ) -> Self {
        Self {
            publish,
            repositories,
        }
    }
}

#[async_trait]
impl Tool for WriteWikiPageTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_wiki_page".into(),
            description: "Create or update a page in a wiki that has been cloned into this \
                          workspace, then commit and push it. Requires user approval."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Wiki page title."},
                    "content": {"type": "string", "description": "Page content in markdown."}
                },
                "required": ["title", "content"]
            }),
        }
    }

    fn requires_consent(&self) -> bool {
        true
    }

    fn describe_call(&self, arguments: &serde_json::Value) -> String {
        format!("Publish wiki page \"{}\"", optional_str(arguments, "title"))
    }

    async fn execute(
        &self,
        workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String> {
        let title = require_str(arguments, "title").map_err(DomainError::Validation)?;
        let content = require_str(arguments, "content").map_err(DomainError::Validation)?;
        // Target the first wiki repository in the workspace.
        let wiki = self
            .publish
            .wiki_repositories(workspace_id)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                DomainError::Validation(
                    "no wiki has been cloned into this workspace — clone a .wiki.git first".into(),
                )
            })?;
        // Confirm the repo still exists (defensive; the id came from us).
        self.repositories.find_by_id(&wiki.id)?;
        let file = self.publish.write_wiki_page(&wiki.id, &title, &content)?;
        Ok(format!("Published wiki page {file} to {}", wiki.name))
    }
}
