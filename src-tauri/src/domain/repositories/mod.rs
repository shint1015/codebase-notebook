use crate::domain::entities::chat::{ChatSession, Message};
use crate::domain::entities::chunk::{Chunk, SearchHit};
use crate::domain::entities::document::Document;
use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::entities::repository::Repository;
use crate::domain::entities::workspace::Workspace;
use crate::domain::error::DomainResult;

pub trait WorkspaceRepository: Send + Sync {
    fn create(&self, workspace: &Workspace) -> DomainResult<()>;
    fn find_by_id(&self, id: &str) -> DomainResult<Workspace>;
    fn list(&self) -> DomainResult<Vec<Workspace>>;
    fn set_allow_external(&self, id: &str, allow: bool) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}

pub trait RepositoryRepository: Send + Sync {
    fn create(&self, repository: &Repository) -> DomainResult<()>;
    fn find_by_id(&self, id: &str) -> DomainResult<Repository>;
    fn list_by_workspace(&self, workspace_id: &str) -> DomainResult<Vec<Repository>>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}

pub trait DocumentRepository: Send + Sync {
    fn upsert_document(&self, document: &Document) -> DomainResult<()>;
    fn find_by_path(&self, workspace_id: &str, rel_path: &str) -> DomainResult<Option<Document>>;
    fn list_by_workspace(&self, workspace_id: &str) -> DomainResult<Vec<Document>>;
    fn count_by_workspace(&self, workspace_id: &str) -> DomainResult<i64>;
    /// Remove all documents (and their chunks) belonging to a repository.
    fn delete_by_repository(&self, repository_id: &str) -> DomainResult<()>;
    /// Replace all chunks of a document (used on re-index).
    fn replace_chunks(&self, document_id: &str, chunks: &[Chunk]) -> DomainResult<()>;
    fn store_embedding(&self, chunk_id: &str, embedding: &[f32]) -> DomainResult<()>;
    fn get_chunk(&self, chunk_id: &str) -> DomainResult<Chunk>;
    fn count_chunks(&self, workspace_id: &str) -> DomainResult<i64>;
    /// Full-text (keyword) search within a workspace.
    fn search_keyword(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<SearchHit>>;
    /// All chunk embeddings of a workspace: (chunk_id, embedding).
    fn embeddings_by_workspace(
        &self,
        workspace_id: &str,
    ) -> DomainResult<Vec<(String, Vec<f32>)>>;
    fn hits_by_chunk_ids(&self, chunk_ids: &[String]) -> DomainResult<Vec<SearchHit>>;
}

pub trait ChatRepository: Send + Sync {
    fn create_session(&self, session: &ChatSession) -> DomainResult<()>;
    fn list_sessions(&self, workspace_id: &str) -> DomainResult<Vec<ChatSession>>;
    fn append_message(&self, message: &Message) -> DomainResult<()>;
    fn list_messages(&self, session_id: &str) -> DomainResult<Vec<Message>>;
}

pub trait ProviderConfigRepository: Send + Sync {
    fn upsert(&self, config: &ProviderConfig) -> DomainResult<()>;
    fn find(&self, kind: ProviderKind) -> DomainResult<Option<ProviderConfig>>;
    fn list(&self) -> DomainResult<Vec<ProviderConfig>>;
}
