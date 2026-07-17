use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::application::usecases::ask::AskUseCase;
use crate::application::usecases::chat::ChatUseCases;
use crate::application::usecases::index::IndexWorkspaceUseCase;
use crate::application::usecases::provider::ProviderUseCases;
use crate::application::usecases::publish::PublishUseCases;
use crate::application::usecases::repository::RepositoryUseCases;
use crate::application::usecases::search::SearchUseCase;
use crate::application::usecases::workspace::WorkspaceUseCases;
use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::error::DomainResult;
use crate::domain::repositories::{
    DocumentRepository, ProviderConfigRepository, RepositoryRepository,
};
use crate::infrastructure::indexing::ast_chunker::SmartChunker;
use crate::infrastructure::indexing::git::GitCliCloner;
use crate::infrastructure::indexing::github_issues::GitHubIssueFetcher;
use crate::infrastructure::indexing::scanner::FsSourceScanner;
use crate::infrastructure::persistence::chat_repo::SqliteChatRepository;
use crate::infrastructure::persistence::document_repo::SqliteDocumentRepository;
use crate::infrastructure::persistence::provider_repo::SqliteProviderConfigRepository;
use crate::infrastructure::persistence::repository_repo::SqliteRepositoryRepository;
use crate::infrastructure::persistence::settings_repo::SqliteSettingsRepository;
use crate::infrastructure::persistence::usage_repo::SqliteUsageRepository;
use crate::infrastructure::persistence::workspace_repo::SqliteWorkspaceRepository;
use crate::infrastructure::persistence::Db;
use crate::infrastructure::providers::ollama::OllamaEmbedding;
use crate::infrastructure::providers::router::ConfiguredProviderRouter;
use crate::infrastructure::secrets::keyring_store::KeyringSecretStore;
use crate::infrastructure::secrets::scanner::RegexSecretScanner;

pub const EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Composition root: wires infrastructure implementations into use cases.
/// This is the only place in the app that knows concrete types.
pub struct AppState {
    pub workspaces: WorkspaceUseCases,
    pub repositories: RepositoryUseCases,
    pub notes: Arc<crate::application::usecases::notes::NotesUseCases>,
    pub backup: crate::application::usecases::backup::BackupUseCases,
    pub publish: Arc<PublishUseCases>,
    pub chats: ChatUseCases,
    pub providers: ProviderUseCases,
    pub index: IndexWorkspaceUseCase,
    pub search: Arc<SearchUseCase>,
    pub ask: AskUseCase,
    pub agent: crate::application::usecases::agent::AgentUseCase,
    pub documents: Arc<dyn DocumentRepository>,
    pub settings: Arc<dyn crate::domain::services::SettingsRepository>,
    pub usage: Arc<dyn crate::domain::repositories::UsageRepository>,
    pub secrets: Arc<dyn crate::domain::services::SecretStore>,
    repository_reader: Arc<dyn RepositoryRepository>,
}

/// Managed handle for the filesystem watcher (armed in the app setup hook).
pub struct WatcherHandle(pub Arc<crate::infrastructure::indexing::watch::SourceWatcher>);

impl AppState {
    pub fn new(db_path: &Path, clones_dir: PathBuf) -> DomainResult<Self> {
        let db = Db::open(db_path)?;

        let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
        let repository_repo: Arc<dyn RepositoryRepository> =
            Arc::new(SqliteRepositoryRepository::new(db.clone()));
        let document_repo: Arc<dyn DocumentRepository> =
            Arc::new(SqliteDocumentRepository::new(db.clone()));
        let chat_repo = Arc::new(SqliteChatRepository::new(db.clone()));
        let settings_repo: Arc<dyn crate::domain::services::SettingsRepository> =
            Arc::new(SqliteSettingsRepository::new(db.clone()));
        let usage_repo: Arc<dyn crate::domain::repositories::UsageRepository> =
            Arc::new(SqliteUsageRepository::new(db.clone()));
        let provider_repo: Arc<SqliteProviderConfigRepository> =
            Arc::new(SqliteProviderConfigRepository::new(db));

        let secret_store = Arc::new(KeyringSecretStore);
        let secret_scanner = Arc::new(RegexSecretScanner::new());
        let scanner = Arc::new(FsSourceScanner);
        let cloner = Arc::new(GitCliCloner);
        let issue_service = Arc::new(GitHubIssueFetcher);

        let ollama_base_url = provider_repo
            .find(ProviderKind::Ollama)?
            .unwrap_or_else(|| ProviderConfig::default_for(ProviderKind::Ollama))
            .base_url;
        let embedder = Arc::new(OllamaEmbedding::new(
            &ollama_base_url,
            EMBEDDING_MODEL,
            settings_repo.clone(),
        ));

        let router = Arc::new(ConfiguredProviderRouter::new(
            provider_repo.clone(),
            secret_store.clone(),
        ));

        let search = Arc::new(SearchUseCase::new(
            document_repo.clone(),
            embedder.clone(),
            router.clone(),
            provider_repo.clone(),
            settings_repo.clone(),
        ));

        let publish = Arc::new(PublishUseCases::new(
            repository_repo.clone(),
            issue_service.clone(),
            cloner.clone(),
        ));

        // Agent tools: search (read) + write tools (GitHub, connectors), all
        // gated behind explicit approval in the agent loop.
        use crate::domain::services::Tool;
        let secret_store_dyn: Arc<dyn crate::domain::services::SecretStore> = secret_store.clone();
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(crate::application::tools::search::SearchSourcesTool::new(
                search.clone(),
            )),
            Arc::new(crate::application::tools::github::CreateGithubIssueTool::new(
                publish.clone(),
            )),
            Arc::new(crate::application::tools::github::WriteWikiPageTool::new(
                publish.clone(),
                repository_repo.clone(),
            )),
            Arc::new(crate::infrastructure::connectors::slack::SlackPostMessageTool::new(
                secret_store_dyn.clone(),
            )),
            Arc::new(crate::infrastructure::connectors::notion::NotionCreatePageTool::new(
                secret_store_dyn.clone(),
            )),
            Arc::new(crate::infrastructure::connectors::asana::AsanaCreateTaskTool::new(
                secret_store_dyn.clone(),
            )),
            Arc::new(crate::infrastructure::connectors::backlog::BacklogCreateIssueTool::new(
                secret_store_dyn.clone(),
            )),
            Arc::new(crate::infrastructure::connectors::confluence::ConfluenceCreatePageTool::new(
                secret_store_dyn.clone(),
            )),
        ];

        let notes = Arc::new(crate::application::usecases::notes::NotesUseCases::new(
            workspace_repo.clone(),
            repository_repo.clone(),
            clones_dir.clone(),
        ));
        let backup = crate::application::usecases::backup::BackupUseCases::new(
            workspace_repo.clone(),
            repository_repo.clone(),
            chat_repo.clone(),
            notes.clone(),
        );

        Ok(Self {
            workspaces: WorkspaceUseCases::new(workspace_repo.clone()),
            repositories: RepositoryUseCases::new(
                workspace_repo.clone(),
                repository_repo.clone(),
                document_repo.clone(),
                cloner.clone(),
                cloner.clone(),
                issue_service.clone(),
                clones_dir,
            ),
            notes,
            backup,
            publish: publish.clone(),
            repository_reader: repository_repo.clone(),
            chats: ChatUseCases::new(chat_repo.clone()),
            providers: ProviderUseCases::new(
                provider_repo.clone(),
                secret_store.clone(),
                router.clone(),
            ),
            index: IndexWorkspaceUseCase::new(
                workspace_repo.clone(),
                repository_repo.clone(),
                document_repo.clone(),
                scanner,
                secret_scanner,
                Arc::new(SmartChunker),
                embedder,
            ),
            ask: AskUseCase::new(
                workspace_repo.clone(),
                repository_repo.clone(),
                document_repo.clone(),
                chat_repo.clone(),
                provider_repo.clone(),
                usage_repo.clone(),
                router.clone(),
                search.clone(),
            ),
            agent: crate::application::usecases::agent::AgentUseCase::new(
                workspace_repo,
                chat_repo,
                provider_repo,
                router,
                tools,
            ),
            search,
            documents: document_repo,
            settings: settings_repo,
            usage: usage_repo,
            secrets: secret_store_dyn,
        })
    }

    /// Local sources to watch: (root path, workspace id). Managed clones and
    /// issue sets only change through explicit app actions, which re-index
    /// themselves.
    pub fn watch_targets(&self) -> DomainResult<Vec<(String, String)>> {
        use crate::domain::entities::repository::SourceKind;
        let mut targets = Vec::new();
        for workspace in self.workspaces.list()? {
            for repository in self.repository_reader.list_by_workspace(&workspace.id)? {
                if repository.source_kind == SourceKind::Local {
                    targets.push((repository.root_path, workspace.id.clone()));
                }
            }
        }
        Ok(targets)
    }
}
