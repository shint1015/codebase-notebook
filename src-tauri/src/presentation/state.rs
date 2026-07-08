use std::path::Path;
use std::sync::Arc;

use crate::application::usecases::ask::AskUseCase;
use crate::application::usecases::chat::ChatUseCases;
use crate::application::usecases::index::IndexWorkspaceUseCase;
use crate::application::usecases::provider::ProviderUseCases;
use crate::application::usecases::search::SearchUseCase;
use crate::application::usecases::workspace::WorkspaceUseCases;
use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::error::DomainResult;
use crate::domain::repositories::{DocumentRepository, ProviderConfigRepository};
use crate::infrastructure::indexing::scanner::FsSourceScanner;
use crate::infrastructure::persistence::chat_repo::SqliteChatRepository;
use crate::infrastructure::persistence::document_repo::SqliteDocumentRepository;
use crate::infrastructure::persistence::provider_repo::SqliteProviderConfigRepository;
use crate::infrastructure::persistence::workspace_repo::SqliteWorkspaceRepository;
use crate::infrastructure::persistence::Db;
use crate::infrastructure::providers::ollama::OllamaEmbedding;
use crate::infrastructure::providers::router::ConfiguredProviderRouter;
use crate::infrastructure::secrets::keyring_store::KeyringSecretStore;
use crate::infrastructure::secrets::scanner::RegexSecretScanner;

const EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Composition root: wires infrastructure implementations into use cases.
/// This is the only place in the app that knows concrete types.
pub struct AppState {
    pub workspaces: WorkspaceUseCases,
    pub chats: ChatUseCases,
    pub providers: ProviderUseCases,
    pub index: IndexWorkspaceUseCase,
    pub search: Arc<SearchUseCase>,
    pub ask: AskUseCase,
    pub documents: Arc<dyn DocumentRepository>,
}

impl AppState {
    pub fn new(db_path: &Path) -> DomainResult<Self> {
        let db = Db::open(db_path)?;

        let workspace_repo = Arc::new(SqliteWorkspaceRepository::new(db.clone()));
        let document_repo: Arc<dyn DocumentRepository> =
            Arc::new(SqliteDocumentRepository::new(db.clone()));
        let chat_repo = Arc::new(SqliteChatRepository::new(db.clone()));
        let provider_repo: Arc<SqliteProviderConfigRepository> =
            Arc::new(SqliteProviderConfigRepository::new(db));

        let secret_store = Arc::new(KeyringSecretStore);
        let secret_scanner = Arc::new(RegexSecretScanner::new());
        let scanner = Arc::new(FsSourceScanner);

        let ollama_base_url = provider_repo
            .find(ProviderKind::Ollama)?
            .unwrap_or_else(|| ProviderConfig::default_for(ProviderKind::Ollama))
            .base_url;
        let embedder = Arc::new(OllamaEmbedding::new(&ollama_base_url, EMBEDDING_MODEL));

        let router = Arc::new(ConfiguredProviderRouter::new(
            provider_repo.clone(),
            secret_store.clone(),
        ));

        let search = Arc::new(SearchUseCase::new(document_repo.clone(), embedder.clone()));

        Ok(Self {
            workspaces: WorkspaceUseCases::new(workspace_repo.clone()),
            chats: ChatUseCases::new(chat_repo.clone()),
            providers: ProviderUseCases::new(
                provider_repo.clone(),
                secret_store.clone(),
                router.clone(),
            ),
            index: IndexWorkspaceUseCase::new(
                workspace_repo.clone(),
                document_repo.clone(),
                scanner,
                secret_scanner,
                embedder,
            ),
            ask: AskUseCase::new(
                workspace_repo,
                chat_repo,
                provider_repo,
                router,
                search.clone(),
            ),
            search,
            documents: document_repo,
        })
    }
}
