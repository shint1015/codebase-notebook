//! End-to-end tests over the real wiring (SQLite + scanner + secret scanner),
//! with only the network-facing services faked.

use std::sync::Arc;

use async_trait::async_trait;

use codebase_notebook_lib::application::usecases::ask::AskUseCase;
use codebase_notebook_lib::application::usecases::chat::ChatUseCases;
use codebase_notebook_lib::application::usecases::index::IndexWorkspaceUseCase;
use codebase_notebook_lib::application::usecases::search::SearchUseCase;
use codebase_notebook_lib::application::usecases::workspace::WorkspaceUseCases;
use codebase_notebook_lib::domain::entities::provider::{ProviderConfig, ProviderKind};
use codebase_notebook_lib::domain::error::{DomainError, DomainResult};
use codebase_notebook_lib::domain::repositories::ProviderConfigRepository;
use codebase_notebook_lib::domain::services::{
    ChatTurn, EmbeddingProvider, LlmProvider, ProviderRouter,
};
use codebase_notebook_lib::infrastructure::indexing::scanner::FsSourceScanner;
use codebase_notebook_lib::infrastructure::persistence::chat_repo::SqliteChatRepository;
use codebase_notebook_lib::infrastructure::persistence::document_repo::SqliteDocumentRepository;
use codebase_notebook_lib::infrastructure::persistence::provider_repo::SqliteProviderConfigRepository;
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
        assert!(
            system.contains("Answer ONLY from the numbered sources"),
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

struct Harness {
    _tmp: tempdir::TempDir,
    workspace_id: String,
    workspaces: WorkspaceUseCases,
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
    let document_repo = Arc::new(SqliteDocumentRepository::new(db.clone()));
    let chat_repo = Arc::new(SqliteChatRepository::new(db.clone()));
    let provider_repo = Arc::new(SqliteProviderConfigRepository::new(db));
    let embedder = Arc::new(NoEmbedding);
    let search = Arc::new(SearchUseCase::new(document_repo.clone(), embedder.clone()));

    let workspaces = WorkspaceUseCases::new(workspace_repo.clone());
    let workspace = workspaces
        .create("demo", repo_dir.to_str().unwrap())
        .unwrap();

    Harness {
        workspace_id: workspace.id,
        workspaces,
        chats: ChatUseCases::new(chat_repo.clone()),
        index: IndexWorkspaceUseCase::new(
            workspace_repo.clone(),
            document_repo.clone(),
            Arc::new(FsSourceScanner),
            Arc::new(RegexSecretScanner::new()),
            embedder,
        ),
        ask: AskUseCase::new(
            workspace_repo,
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
        .prepare(&h.workspace_id, "Explain auth", ProviderKind::Anthropic)
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
        .prepare(&h.workspace_id, "Explain auth", ProviderKind::Anthropic)
        .await
        .unwrap();
    assert!(!preparation.requires_consent);
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
