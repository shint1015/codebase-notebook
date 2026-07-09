use async_trait::async_trait;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::DomainResult;

/// One turn of a chat conversation sent to a model.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

/// An LLM chat backend. One implementation per provider (adapter pattern).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String>;
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
