use async_trait::async_trait;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::DomainResult;

/// One turn of a chat conversation sent to a model. Assistant turns may carry
/// tool-call requests; tool-result turns carry `tool_call_id`.
#[derive(Debug, Clone, Default)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            ..Default::default()
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            ..Default::default()
        }
    }
}

/// A callable capability advertised to the model (JSON-schema parameters).
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool invocation the model asked for.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// One step of an agent conversation: either a final answer or tool requests.
#[derive(Debug, Clone)]
pub enum AgentStep {
    Message(String),
    ToolCalls(Vec<ToolCall>),
}

/// Receives incremental answer fragments during generation.
pub type TokenSink = dyn Fn(&str) + Send + Sync;

/// An LLM chat backend. One implementation per provider (adapter pattern).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String>;
    /// Streaming variant: forwards fragments to `on_token` as they arrive and
    /// returns the full text. Providers without streaming support fall back
    /// to one final chunk.
    async fn chat_stream(
        &self,
        model: &str,
        system: &str,
        turns: &[ChatTurn],
        on_token: &TokenSink,
    ) -> DomainResult<String> {
        let text = self.chat(model, system, turns).await?;
        on_token(&text);
        Ok(text)
    }
    /// Tool-calling variant. Providers without tool support return
    /// `AgentStep::Message` from a plain chat (tools are then unavailable).
    async fn chat_with_tools(
        &self,
        model: &str,
        system: &str,
        turns: &[ChatTurn],
        _tools: &[ToolSpec],
    ) -> DomainResult<AgentStep> {
        Ok(AgentStep::Message(self.chat(model, system, turns).await?))
    }
    /// Cheap connectivity / credential check.
    async fn test_connection(&self) -> DomainResult<String>;
}

/// A capability the agent can invoke on the user's behalf (search sources,
/// create an issue, post to Slack, ...). External writes set
/// `requires_consent` so the agent gates them behind explicit approval.
#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    /// True for actions that change external state and need user approval.
    fn requires_consent(&self) -> bool;
    /// Short human-readable summary of what a call would do (for the approval
    /// prompt), e.g. "Create issue \"Fix login\" in acme/app".
    fn describe_call(&self, arguments: &serde_json::Value) -> String;
    async fn execute(
        &self,
        workspace_id: &str,
        arguments: &serde_json::Value,
    ) -> DomainResult<String>;
}

/// Text embedding backend (local by default).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> DomainResult<Vec<Vec<f32>>>;
    async fn is_available(&self) -> bool;
}

/// Secure storage for provider API keys (OS keychain — never the DB).
pub trait SecretStore: Send + Sync {
    fn set_api_key(&self, kind: ProviderKind, api_key: &str) -> DomainResult<()>;
    fn get_api_key(&self, kind: ProviderKind) -> DomainResult<Option<String>>;
    fn delete_api_key(&self, kind: ProviderKind) -> DomainResult<()>;
    /// Generic keychain slot for connector tokens (Slack, Notion, ...),
    /// addressed by an arbitrary key such as "connector-slack".
    fn set_secret(&self, key: &str, value: &str) -> DomainResult<()>;
    fn get_secret(&self, key: &str) -> DomainResult<Option<String>>;
    fn delete_secret(&self, key: &str) -> DomainResult<()>;
}

/// A source file discovered under a workspace root.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub rel_path: String,
    pub language: String,
    pub content: String,
    pub content_hash: String,
}

/// Walks a workspace root and yields indexable source files.
pub trait SourceScanner: Send + Sync {
    fn scan(&self, root_path: &str) -> DomainResult<Vec<SourceFile>>;
}

/// Splits file content into retrievable chunks. Implementations may be
/// syntax-aware (AST-based) per language, falling back to plain line
/// windows.
pub trait Chunker: Send + Sync {
    fn chunk(&self, content: &str, language: &str) -> Vec<crate::domain::chunking::TextChunk>;
}

/// Key-value application settings (embedding model, rerank toggle, ...).
pub trait SettingsRepository: Send + Sync {
    fn get(&self, key: &str) -> DomainResult<Option<String>>;
    fn set(&self, key: &str, value: &str) -> DomainResult<()>;
}

/// Clones remote git repositories into app-managed directories and removes
/// them again. Implementation shells out to the user's git.
pub trait RepoCloner: Send + Sync {
    fn clone_repo(&self, url: &str, dest: &str) -> DomainResult<()>;
    fn remove_clone(&self, path: &str) -> DomainResult<()>;
}

/// One issue fetched from a tracker, ready to be materialized as markdown.
#[derive(Debug, Clone)]
pub struct IssueDoc {
    pub number: i64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub body: String,
    pub url: String,
    pub created_at: String,
}

/// Fetches issues from a remote tracker (e.g. GitHub) so they can be indexed
/// locally. Network access happens only on the explicit "fetch" action.
#[async_trait]
pub trait IssueFetcher: Send + Sync {
    /// `spec` is "owner/repo".
    async fn fetch_issues(&self, spec: &str) -> DomainResult<Vec<IssueDoc>>;
}

/// Creates issues on a remote tracker using the user's own credentials.
/// Only invoked by an explicit user action.
#[async_trait]
pub trait IssuePublisher: Send + Sync {
    /// Returns the URL of the created issue.
    async fn create_issue(&self, spec: &str, title: &str, body: &str) -> DomainResult<String>;
}

/// Git working-tree synchronization for app-managed clones (wiki publishing).
pub trait GitSync: Send + Sync {
    fn pull(&self, repo_path: &str) -> DomainResult<()>;
    fn commit_and_push(&self, repo_path: &str, message: &str) -> DomainResult<()>;
}

/// Resolves the concrete LLM adapter for a provider kind (model router).
pub trait ProviderRouter: Send + Sync {
    fn resolve(&self, kind: ProviderKind) -> DomainResult<std::sync::Arc<dyn LlmProvider>>;
}

/// A secret found in source content during indexing.
#[derive(Debug, Clone)]
pub struct SecretFinding {
    pub rule: String,
    pub line: usize,
}

/// Detects credentials/API keys/private keys in content before it is indexed.
pub trait SecretScanner: Send + Sync {
    fn scan(&self, content: &str) -> Vec<SecretFinding>;
    /// Redact found secrets, returning sanitized content.
    fn redact(&self, content: &str) -> String;
}
