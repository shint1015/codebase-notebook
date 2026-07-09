//! End-to-end tests over the real wiring (SQLite + scanner + secret scanner),
//! with only the network-facing services faked.

use std::sync::Arc;

use async_trait::async_trait;

use codebase_notebook_lib::application::usecases::ask::AskUseCase;
use codebase_notebook_lib::application::usecases::chat::ChatUseCases;
use codebase_notebook_lib::application::usecases::index::IndexWorkspaceUseCase;
use codebase_notebook_lib::application::usecases::repository::RepositoryUseCases;
use codebase_notebook_lib::application::usecases::search::SearchUseCase;
use codebase_notebook_lib::application::usecases::workspace::WorkspaceUseCases;
use codebase_notebook_lib::domain::entities::provider::{ProviderConfig, ProviderKind};
use codebase_notebook_lib::domain::error::{DomainError, DomainResult};
use codebase_notebook_lib::domain::repositories::ProviderConfigRepository;
use codebase_notebook_lib::domain::services::{
    ChatTurn, EmbeddingProvider, IssueDoc, IssueFetcher, LlmProvider, ProviderRouter,
};
use codebase_notebook_lib::infrastructure::indexing::ast_chunker::SmartChunker;
use codebase_notebook_lib::infrastructure::indexing::git::GitCliCloner;
use codebase_notebook_lib::infrastructure::indexing::scanner::FsSourceScanner;
use codebase_notebook_lib::infrastructure::persistence::chat_repo::SqliteChatRepository;
use codebase_notebook_lib::infrastructure::persistence::document_repo::SqliteDocumentRepository;
use codebase_notebook_lib::infrastructure::persistence::provider_repo::SqliteProviderConfigRepository;
use codebase_notebook_lib::infrastructure::persistence::repository_repo::SqliteRepositoryRepository;
use codebase_notebook_lib::infrastructure::persistence::settings_repo::SqliteSettingsRepository;
use codebase_notebook_lib::infrastructure::persistence::workspace_repo::SqliteWorkspaceRepository;
use codebase_notebook_lib::infrastructure::persistence::Db;
use codebase_notebook_lib::infrastructure::secrets::scanner::RegexSecretScanner;

struct NoEmbedding;

#[async_trait]
impl EmbeddingProvider for NoEmbedding {
    async fn embed(&self, _texts: &[String]) -> DomainResult<Vec<Vec<f32>>> {
        Err(DomainError::Provider("offline".into()))
    }
    async fn is_available(&self) -> bool {
        false
    }
}

/// Fake LLM that always cites source [1].
struct CitingLlm(ProviderKind);

#[async_trait]
impl LlmProvider for CitingLlm {
    fn kind(&self) -> ProviderKind {
        self.0
    }
    async fn chat(&self, _model: &str, system: &str, _turns: &[ChatTurn]) -> DomainResult<String> {
        // Query-rewrite calls get a plain standalone query back.
        if system.starts_with("Rewrite") {
            return Ok("token validation in auth".to_string());
        }
        assert!(
            system.contains("Answer ONLY from the workspace overview and the numbered sources"),
            "grounding instruction must be present"
        );
        Ok("The session token is validated in `validate_token` [1].".to_string())
    }
    async fn test_connection(&self) -> DomainResult<String> {
        Ok("ok".into())
    }
}

struct FakeRouter;

impl ProviderRouter for FakeRouter {
    fn resolve(&self, kind: ProviderKind) -> DomainResult<Arc<dyn LlmProvider>> {
        Ok(Arc::new(CitingLlm(kind)))
    }
}

/// Two fake issues, no network.
struct FakeIssueFetcher;

#[async_trait]
impl IssueFetcher for FakeIssueFetcher {
    async fn fetch_issues(&self, spec: &str) -> DomainResult<Vec<IssueDoc>> {
        Ok(vec![
            IssueDoc {
                number: 1,
                title: "Crash on startup".into(),
                state: "open".into(),
                author: "alice".into(),
                labels: vec!["bug".into()],
                body: format!("The app from {spec} crashes when the config is missing."),
                url: format!("https://github.com/{spec}/issues/1"),
                created_at: "2026-01-01T00:00:00Z".into(),
            },
            IssueDoc {
                number: 2,
                title: "Add dark mode".into(),
                state: "closed".into(),
                author: "bob".into(),
                labels: vec![],
                body: "Please support a dark theme.".into(),
                url: format!("https://github.com/{spec}/issues/2"),
                created_at: "2026-01-02T00:00:00Z".into(),
            },
        ])
    }
}

struct Harness {
    _tmp: tempdir::TempDir,
    workspace_id: String,
    workspaces: WorkspaceUseCases,
    repositories: RepositoryUseCases,
    chats: ChatUseCases,
    index: IndexWorkspaceUseCase,
    search: Arc<SearchUseCase>,
    ask: AskUseCase,
    providers: Arc<SqliteProviderConfigRepository>,
}

mod tempdir {
    pub struct TempDir(pub std::path::PathBuf);
    impl TempDir {
        pub fn new(prefix: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        pub fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
}

async fn setup() -> Harness {
    let tmp = tempdir::TempDir::new("cbnb-it");
    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(repo_dir.join("src")).unwrap();
    std::fs::write(
        repo_dir.join("README.md"),
        "# Demo\n\nThis service handles session authentication.",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("src/auth.rs"),
        "// session auth\nfn validate_token(token: &str) -> bool {\n    let api_key = \"AKIAIOSFODNN7EXAMPLE\";\n    !token.is_empty()\n}\n",
    )
    .unwrap();

    let db = Db::open(&tmp.path().join("test.sqlite")).unwrap();
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let repository_repo = Arc::new(SqliteRepositoryRepository::new(db.clone()));
    let document_repo = Arc::new(SqliteDocumentRepository::new(db.clone()));
    let chat_repo = Arc::new(SqliteChatRepository::new(db.clone()));
    let settings_repo = Arc::new(SqliteSettingsRepository::new(db.clone()));
    let provider_repo = Arc::new(SqliteProviderConfigRepository::new(db));
    let embedder = Arc::new(NoEmbedding);
    let search = Arc::new(SearchUseCase::new(
        document_repo.clone(),
        embedder.clone(),
        Arc::new(FakeRouter),
        provider_repo.clone(),
        settings_repo,
    ));

    let workspaces = WorkspaceUseCases::new(workspace_repo.clone());
    let workspace = workspaces.create("demo").unwrap();

    let repositories = RepositoryUseCases::new(
        workspace_repo.clone(),
        repository_repo.clone(),
        document_repo.clone(),
        Arc::new(GitCliCloner),
        Arc::new(GitCliCloner),
        Arc::new(FakeIssueFetcher),
        tmp.path().join("clones"),
    );
    repositories
        .add_local(&workspace.id, repo_dir.to_str().unwrap())
        .unwrap();

    Harness {
        workspace_id: workspace.id,
        workspaces,
        repositories,
        chats: ChatUseCases::new(chat_repo.clone()),
        index: IndexWorkspaceUseCase::new(
            workspace_repo.clone(),
            repository_repo.clone(),
            document_repo.clone(),
            Arc::new(FsSourceScanner),
            Arc::new(RegexSecretScanner::new()),
            Arc::new(SmartChunker),
            embedder,
        ),
        ask: AskUseCase::new(
            workspace_repo,
            repository_repo,
            document_repo.clone(),
            chat_repo,
            provider_repo.clone(),
            Arc::new(FakeRouter),
            search.clone(),
        ),
        search,
        providers: provider_repo,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn index_search_and_grounded_ask_flow() {
    let h = setup().await;

    // Index: both files picked up, the AWS key redacted before storage.
    let report = h.index.execute(&h.workspace_id).await.unwrap();
    assert_eq!(report.files_indexed, 2);
    assert_eq!(report.files_with_secrets_redacted, 1);
    assert!(!report.embedding_available);

    // Re-index without changes is a no-op.
    let second = h.index.execute(&h.workspace_id).await.unwrap();
    assert_eq!(second.files_indexed, 0);
    assert_eq!(second.files_unchanged, 2);

    // Keyword search finds the auth chunk; the secret never reached the index.
    let hits = h
        .search
        .execute(&h.workspace_id, "validate_token session", 10)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    let all_content: String = hits.iter().map(|hit| hit.chunk.content.clone()).collect();
    assert!(!all_content.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(all_content.contains("[REDACTED:"));

    // Grounded ask on the local provider: no consent needed, citation mapped.
    let session = h.chats.create_session(&h.workspace_id, "auth").unwrap();
    let answer = h
        .ask
        .execute(
            &session.id,
            &h.workspace_id,
            "How is the token validated?",
            ProviderKind::Ollama,
            false,
        )
        .await
        .unwrap();
    assert_eq!(answer.citations.len(), 1);
    assert_eq!(answer.citations[0].marker, 1);
    let messages = h.chats.list_messages(&session.id).unwrap();
    assert_eq!(messages.len(), 2, "user + assistant persisted");
}

#[tokio::test]
async fn external_provider_requires_consent() {
    let h = setup().await;
    h.index.execute(&h.workspace_id).await.unwrap();

    // Enable Anthropic with code sending allowed.
    let mut config = ProviderConfig::default_for(ProviderKind::Anthropic);
    config.enabled = true;
    config.allow_send_code = true;
    h.providers.upsert(&config).unwrap();

    let session = h.chats.create_session(&h.workspace_id, "ext").unwrap();

    // Without consent → blocked.
    let denied = h
        .ask
        .execute(
            &session.id,
            &h.workspace_id,
            "Explain auth",
            ProviderKind::Anthropic,
            false,
        )
        .await;
    assert!(matches!(denied, Err(DomainError::ConsentRequired)));

    // prepare() reports consent is required and lists the exact sources.
    let preparation = h
        .ask
        .prepare(&h.workspace_id, "Explain auth", ProviderKind::Anthropic, None)
        .await
        .unwrap();
    assert!(preparation.is_external);
    assert!(preparation.requires_consent);
    assert!(!preparation.sources.is_empty());

    // With one-time consent → allowed.
    let allowed = h
        .ask
        .execute(
            &session.id,
            &h.workspace_id,
            "Explain auth",
            ProviderKind::Anthropic,
            true,
        )
        .await;
    assert!(allowed.is_ok());

    // Workspace-level allow_external also unlocks it without per-call consent.
    h.workspaces
        .set_allow_external(&h.workspace_id, true)
        .unwrap();
    let preparation = h
        .ask
        .prepare(&h.workspace_id, "Explain auth", ProviderKind::Anthropic, None)
        .await
        .unwrap();
    assert!(!preparation.requires_consent);
}

#[tokio::test]
async fn asking_before_indexing_returns_guidance_error() {
    let h = setup().await;
    // Repository added but never indexed.
    let session = h.chats.create_session(&h.workspace_id, "early").unwrap();

    let prepared = h
        .ask
        .prepare(&h.workspace_id, "What is this?", ProviderKind::Ollama, None)
        .await;
    assert!(matches!(prepared, Err(DomainError::Validation(_))));

    let asked = h
        .ask
        .execute(
            &session.id,
            &h.workspace_id,
            "What is this?",
            ProviderKind::Ollama,
            false,
        )
        .await;
    match asked {
        Err(DomainError::Validation(message)) => {
            assert!(message.contains("no indexed sources"));
        }
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn multiple_repositories_index_and_remove_independently() {
    let h = setup().await;

    // Second repository in the same workspace.
    let second_dir = h._tmp.path().join("second");
    std::fs::create_dir_all(&second_dir).unwrap();
    std::fs::write(
        second_dir.join("billing.md"),
        "# Billing\n\nInvoices are generated nightly by the billing_cron job.",
    )
    .unwrap();
    let second = h
        .repositories
        .add_local(&h.workspace_id, second_dir.to_str().unwrap())
        .unwrap();

    let report = h.index.execute(&h.workspace_id).await.unwrap();
    assert_eq!(report.files_indexed, 3, "both repositories are indexed");

    // Paths are prefixed with the repository name.
    let hits = h
        .search
        .execute(&h.workspace_id, "billing_cron invoices", 10)
        .await
        .unwrap();
    assert!(hits.iter().any(|hit| hit.rel_path == "second/billing.md"));

    // Duplicate repository names in one workspace are rejected.
    let duplicate = h
        .repositories
        .add_local(&h.workspace_id, second_dir.to_str().unwrap());
    assert!(matches!(duplicate, Err(DomainError::Validation(_))));

    // Removing the repository removes its indexed data, not the other repo's.
    h.repositories.remove(&second.id).unwrap();
    let hits = h
        .search
        .execute(&h.workspace_id, "billing_cron invoices", 10)
        .await
        .unwrap();
    assert!(hits.iter().all(|hit| !hit.rel_path.starts_with("second/")));
    let still = h
        .search
        .execute(&h.workspace_id, "validate_token", 10)
        .await
        .unwrap();
    assert!(!still.is_empty());
}

#[tokio::test]
async fn single_file_and_github_issues_sources() {
    let h = setup().await;

    // Single file as a source.
    let notes = h._tmp.path().join("meeting-notes.md");
    std::fs::write(&notes, "# Meeting\n\nDecided to migrate the queue to NATS.").unwrap();
    let file_repo = h
        .repositories
        .add_local(&h.workspace_id, notes.to_str().unwrap())
        .unwrap();
    assert_eq!(file_repo.name, "meeting-notes.md");

    // GitHub issues (via fake fetcher) materialized as markdown.
    let issues_repo = h
        .repositories
        .add_github_issues(&h.workspace_id, "https://github.com/acme/app/issues")
        .await
        .unwrap();
    assert_eq!(issues_repo.name, "app-issues");
    assert!(issues_repo.remote_url.as_deref() == Some("https://github.com/acme/app/issues"));

    let report = h.index.execute(&h.workspace_id).await.unwrap();
    // 2 code files + 1 note file + 2 issue files
    assert_eq!(report.files_indexed, 5);

    // Both new sources are searchable with their repo-name prefixes.
    let hits = h
        .search
        .execute(&h.workspace_id, "migrate queue NATS", 10)
        .await
        .unwrap();
    assert!(hits.iter().any(|hit| hit.rel_path == "meeting-notes.md"));

    let hits = h
        .search
        .execute(&h.workspace_id, "crash startup config missing", 10)
        .await
        .unwrap();
    assert!(hits
        .iter()
        .any(|hit| hit.rel_path == "app-issues/issue-00001.md"));

    // Sync re-materializes the issue files (stale ones replaced).
    let issue_file = std::path::Path::new(&issues_repo.root_path).join("issue-00001.md");
    std::fs::remove_file(&issue_file).unwrap();
    h.repositories.sync(&issues_repo.id).await.unwrap();
    assert!(issue_file.exists(), "sync must restore issue files");

    // Removing the issues repo also removes the materialized files.
    let dir = issues_repo.root_path.clone();
    assert!(std::path::Path::new(&dir).exists());
    h.repositories.remove(&issues_repo.id).unwrap();
    assert!(!std::path::Path::new(&dir).exists());
}

#[tokio::test]
async fn disabled_provider_is_rejected() {
    let h = setup().await;
    h.index.execute(&h.workspace_id).await.unwrap();
    let session = h.chats.create_session(&h.workspace_id, "off").unwrap();
    // OpenAI is disabled by default.
    let result = h
        .ask
        .execute(
            &session.id,
            &h.workspace_id,
            "anything",
            ProviderKind::OpenAi,
            true,
        )
        .await;
    assert!(matches!(
        result,
        Err(DomainError::ProviderNotConfigured(_))
    ));
}
