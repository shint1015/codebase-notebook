use async_trait::async_trait;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::DomainResult;

/// One turn of a chat conversation sent to a model.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
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
    /// Cheap connectivity / credential check.
    async fn test_connection(&self) -> DomainResult<String>;
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
