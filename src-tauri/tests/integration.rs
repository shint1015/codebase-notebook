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
use codebase_notebook_lib::domain::repositories::{ChatRepository, ProviderConfigRepository};
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
use codebase_notebook_lib::infrastructure::persistence::usage_repo::SqliteUsageRepository;
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
    let usage_repo = Arc::new(SqliteUsageRepository::new(db.clone()));
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
            usage_repo,
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
async fn agent_runs_tools_and_gates_writes() {
    use codebase_notebook_lib::application::usecases::agent::AgentUseCase;
    use codebase_notebook_lib::domain::services::{AgentStep, Tool, ToolCall, ToolSpec};
    use std::sync::Mutex;

    let h = setup().await;
    h.index.execute(&h.workspace_id).await.unwrap();

    struct RecordingWriteTool {
        executed: Arc<Mutex<bool>>,
    }
    #[async_trait]
    impl Tool for RecordingWriteTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "create_issue".into(),
                description: "create an issue".into(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            }
        }
        fn requires_consent(&self) -> bool {
            true
        }
        fn describe_call(&self, _a: &serde_json::Value) -> String {
            "create an issue".into()
        }
        async fn execute(&self, _w: &str, _a: &serde_json::Value) -> DomainResult<String> {
            *self.executed.lock().unwrap() = true;
            Ok("created issue #1".into())
        }
    }

    struct ScriptedLlm {
        round: Mutex<usize>,
    }
    #[async_trait]
    impl LlmProvider for ScriptedLlm {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Ollama
        }
        async fn chat(&self, _m: &str, _s: &str, _t: &[ChatTurn]) -> DomainResult<String> {
            Ok("ok".into())
        }
        async fn chat_with_tools(
            &self,
            _m: &str,
            _s: &str,
            _t: &[ChatTurn],
            _tools: &[ToolSpec],
        ) -> DomainResult<AgentStep> {
            let mut round = self.round.lock().unwrap();
            *round += 1;
            if *round == 1 {
                Ok(AgentStep::ToolCalls(vec![ToolCall {
                    id: "c1".into(),
                    name: "create_issue".into(),
                    arguments: serde_json::json!({}),
                }]))
            } else {
                Ok(AgentStep::Message("Done.".into()))
            }
        }
        async fn test_connection(&self) -> DomainResult<String> {
            Ok("ok".into())
        }
    }
    struct ScriptedRouter;
    impl ProviderRouter for ScriptedRouter {
        fn resolve(&self, _k: ProviderKind) -> DomainResult<Arc<dyn LlmProvider>> {
            Ok(Arc::new(ScriptedLlm {
                round: Mutex::new(0),
            }))
        }
    }

    let build_agent = |executed: Arc<Mutex<bool>>| {
        let db = Db::open(&h._tmp.path().join("test.sqlite")).unwrap();
        AgentUseCase::new(
            Arc::new(SqliteWorkspaceRepository::new(db.clone())),
            Arc::new(SqliteChatRepository::new(db.clone())),
            Arc::new(SqliteProviderConfigRepository::new(db)),
            Arc::new(ScriptedRouter),
            vec![Arc::new(RecordingWriteTool { executed })],
        )
    };

    let mut config = ProviderConfig::default_for(ProviderKind::Ollama);
    config.enabled = true;
    h.providers.upsert(&config).unwrap();
    let session = h.chats.create_session(&h.workspace_id, "agent").unwrap();

    // Writes NOT allowed → tool is blocked, never executes.
    let executed = Arc::new(Mutex::new(false));
    let outcome = build_agent(executed.clone())
        .run(&session.id, &h.workspace_id, "file an issue", ProviderKind::Ollama, false)
        .await
        .unwrap();
    assert!(outcome.tool_events.iter().any(|e| e.blocked));
    assert!(!*executed.lock().unwrap(), "write must not run without approval");

    // Writes allowed → tool executes.
    let executed = Arc::new(Mutex::new(false));
    let outcome = build_agent(executed.clone())
        .run(&session.id, &h.workspace_id, "file an issue", ProviderKind::Ollama, true)
        .await
        .unwrap();
    assert!(outcome.tool_events.iter().any(|e| !e.blocked));
    assert!(*executed.lock().unwrap(), "write must run once approved");
    assert_eq!(outcome.message.content, "Done.");
}

#[tokio::test]
async fn forking_a_chat_duplicates_its_messages() {
    let h = setup().await;
    let session = h.chats.create_session(&h.workspace_id, "original").unwrap();

    // Two messages in the source chat (append via the repo directly).
    let db = Db::open(&h._tmp.path().join("test.sqlite")).unwrap();
    let chat_repo = SqliteChatRepository::new(db);
    use codebase_notebook_lib::domain::entities::chat::{Message, Role};
    for (role, content) in [(Role::User, "hi"), (Role::Assistant, "hello [1]")] {
        chat_repo
            .append_message(&Message {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session.id.clone(),
                role,
                content: content.into(),
                citations: Vec::new(),
                provider: None,
                model: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            })
            .unwrap();
    }

    let forked = h.chats.fork_session(&session.id, None).unwrap();
    assert_ne!(forked.id, session.id);
    assert_eq!(forked.title, "original (fork)");
    assert_eq!(forked.workspace_id, h.workspace_id);

    let original_msgs = h.chats.list_messages(&session.id).unwrap();
    let forked_msgs = h.chats.list_messages(&forked.id).unwrap();
    assert_eq!(forked_msgs.len(), original_msgs.len());
    assert_eq!(forked_msgs[0].content, "hi");
    // Copied messages get fresh ids.
    assert_ne!(forked_msgs[0].id, original_msgs[0].id);

    // Forking up to the first message only copies that one (branch point).
    let branch = h
        .chats
        .fork_session(&session.id, Some(&original_msgs[0].id))
        .unwrap();
    let branch_msgs = h.chats.list_messages(&branch.id).unwrap();
    assert_eq!(branch_msgs.len(), 1);
    assert_eq!(branch_msgs[0].content, "hi");

    // The chats are independent now.
    let listed = h.chats.list_sessions(&h.workspace_id).unwrap();
    assert!(listed.iter().any(|s| s.id == session.id));
    assert!(listed.iter().any(|s| s.id == forked.id));
}

#[tokio::test]
async fn workspace_export_import_round_trips() {
    use codebase_notebook_lib::application::usecases::backup::BackupUseCases;
    use codebase_notebook_lib::application::usecases::notes::NotesUseCases;
    use codebase_notebook_lib::domain::repositories::WorkspaceRepository;

    let h = setup().await;
    let db = Db::open(&h._tmp.path().join("test.sqlite")).unwrap();
    let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
    let repository_repo = Arc::new(SqliteRepositoryRepository::new(db.clone()));
    let chat_repo = Arc::new(SqliteChatRepository::new(db.clone()));
    let notes = Arc::new(NotesUseCases::new(
        workspace_repo.clone(),
        repository_repo.clone(),
        h._tmp.path().join("clones"),
    ));
    let backup = BackupUseCases::new(
        workspace_repo.clone(),
        repository_repo.clone(),
        chat_repo.clone(),
        notes.clone(),
    );

    // Give the workspace instructions, a note and a chat.
    h.workspaces
        .set_instructions(&h.workspace_id, "Always answer in Japanese.")
        .unwrap();
    notes.save(&h.workspace_id, "Runbook", "# Runbook\n\nRestart the queue.").unwrap();
    let session = h.chats.create_session(&h.workspace_id, "onboarding").unwrap();
    use codebase_notebook_lib::domain::entities::chat::{Message, Role};
    chat_repo
        .append_message(&Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session.id.clone(),
            role: Role::User,
            content: "how do I start?".into(),
            citations: Vec::new(),
            provider: None,
            model: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();

    let export = backup.export(&h.workspace_id).unwrap();
    assert_eq!(export.instructions, "Always answer in Japanese.");
    assert_eq!(export.notes.len(), 1);
    assert_eq!(export.chats.len(), 1);
    // The derived index is deliberately not exported.
    assert!(!export.repositories.is_empty());

    // Survives a JSON round-trip (that's how it's written to disk).
    let json = serde_json::to_string(&export).unwrap();
    let parsed = serde_json::from_str(&json).unwrap();

    let new_id = backup.import(parsed).unwrap();
    assert_ne!(new_id, h.workspace_id);
    let imported = workspace_repo.find_by_id(&new_id).unwrap();
    assert!(imported.name.contains("(imported)"));
    assert_eq!(imported.instructions, "Always answer in Japanese.");
    // Imported workspaces never inherit external-send permission.
    assert!(!imported.allow_external);

    assert!(notes
        .read(&new_id, "Runbook.md")
        .unwrap()
        .contains("Restart the queue"));
    let chats = h.chats.list_sessions(&new_id).unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(h.chats.list_messages(&chats[0].id).unwrap()[0].content, "how do I start?");
}

#[tokio::test]
async fn at_mentions_pin_a_file_into_the_context() {
    let h = setup().await;
    h.index.execute(&h.workspace_id).await.unwrap();
    let repo_name = h.repositories.list(&h.workspace_id).unwrap()[0].name.clone();
    let mentioned = format!("{repo_name}/README.md");

    // A question whose words point at auth.rs, but that @-mentions README.md.
    let prepared = h
        .ask
        .prepare(
            &h.workspace_id,
            &format!("@{mentioned} validate_token session"),
            ProviderKind::Ollama,
            None,
        )
        .await
        .unwrap();

    assert!(
        prepared.sources.iter().any(|s| s.rel_path == mentioned),
        "the @mentioned file must be in the context, got {:?}",
        prepared.sources.iter().map(|s| &s.rel_path).collect::<Vec<_>>()
    );
    // Search results still come along.
    assert!(prepared.sources.len() > 1);
}

#[tokio::test]
async fn chats_are_searchable_across_sessions() {
    let h = setup().await;
    let db = Db::open(&h._tmp.path().join("test.sqlite")).unwrap();
    let chat_repo = SqliteChatRepository::new(db);
    use codebase_notebook_lib::domain::entities::chat::{Message, Role};

    let a = h.chats.create_session(&h.workspace_id, "deploys").unwrap();
    let b = h.chats.create_session(&h.workspace_id, "billing").unwrap();
    for (session, content) in [
        (&a, "How do we roll back a deploy?"),
        (&b, "The billing_cron job runs nightly at 3am"),
    ] {
        chat_repo
            .append_message(&Message {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session.id.clone(),
                role: Role::User,
                content: content.into(),
                citations: Vec::new(),
                provider: None,
                model: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            })
            .unwrap();
    }

    let hits = h.chats.search_messages(&h.workspace_id, "billing_cron", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, b.id);
    assert_eq!(hits[0].session_title, "billing");
    assert!(hits[0].excerpt.contains("billing_cron"));

    // Case-insensitive, and scoped to the workspace.
    assert_eq!(h.chats.search_messages(&h.workspace_id, "ROLL BACK", 10).unwrap().len(), 1);
    assert!(h.chats.search_messages("other-workspace", "billing_cron", 10).unwrap().is_empty());
    assert!(h.chats.search_messages(&h.workspace_id, "  ", 10).unwrap().is_empty());
}

#[tokio::test]
async fn source_files_are_readable_writable_and_contained() {
    let h = setup().await;

    // The seeded repo is registered under its folder name.
    let repos = h.repositories.list(&h.workspace_id).unwrap();
    let repo_name = repos[0].name.clone();

    // Read a real file through its citation-style path.
    let rel = format!("{repo_name}/src/auth.rs");
    let content = h.repositories.read_source_file(&h.workspace_id, &rel).unwrap();
    assert!(content.contains("validate_token"));

    // Write it back with an edit.
    let edited = content.replace("validate_token", "verify_token");
    h.repositories
        .write_source_file(&h.workspace_id, &rel, &edited)
        .unwrap();
    assert!(h
        .repositories
        .read_source_file(&h.workspace_id, &rel)
        .unwrap()
        .contains("verify_token"));

    // Path traversal out of the repository must be refused.
    let escape = format!("{repo_name}/../../../../../../etc/passwd");
    assert!(
        h.repositories
            .read_source_file(&h.workspace_id, &escape)
            .is_err(),
        "must not read outside the repository"
    );
    assert!(h
        .repositories
        .write_source_file(&h.workspace_id, &escape, "pwned")
        .is_err());

    // Unknown repositories are refused too.
    assert!(h
        .repositories
        .read_source_file(&h.workspace_id, "nope/file.rs")
        .is_err());
}

#[tokio::test]
async fn in_app_notes_are_saved_and_indexed() {
    use codebase_notebook_lib::application::usecases::notes::NotesUseCases;

    let h = setup().await;
    let db = Db::open(&h._tmp.path().join("test.sqlite")).unwrap();
    let notes = NotesUseCases::new(
        Arc::new(SqliteWorkspaceRepository::new(db.clone())),
        Arc::new(SqliteRepositoryRepository::new(db)),
        h._tmp.path().join("clones"),
    );

    // Save a note; it registers the "notes" source and writes a .md file.
    let file = notes
        .save(
            &h.workspace_id,
            "Deploy Runbook",
            "# Deploy\n\nRun `make deploy` to ship the release_pipeline.",
        )
        .unwrap();
    assert_eq!(file, "Deploy-Runbook.md");

    // Listed and readable.
    let listed = notes.list(&h.workspace_id).unwrap();
    assert!(listed.iter().any(|n| n.name == "Deploy-Runbook.md"));
    assert!(notes
        .read(&h.workspace_id, "Deploy-Runbook.md")
        .unwrap()
        .contains("make deploy"));

    // Indexed and searchable under the notes/ prefix.
    h.index.execute(&h.workspace_id).await.unwrap();
    let hits = h
        .search
        .execute(&h.workspace_id, "deploy release_pipeline", 10)
        .await
        .unwrap();
    assert!(hits.iter().any(|hit| hit.rel_path == "notes/Deploy-Runbook.md"));

    // Delete removes the file.
    notes.delete(&h.workspace_id, "Deploy-Runbook.md").unwrap();
    assert!(notes.list(&h.workspace_id).unwrap().is_empty());
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
