use std::sync::Arc;

use serde::Serialize;

use crate::domain::entities::chunk::Chunk;
use crate::domain::entities::document::Document;
use crate::domain::error::DomainResult;
use crate::domain::repositories::{
    DocumentRepository, RepositoryRepository, WorkspaceRepository,
};
use crate::domain::services::{Chunker, EmbeddingProvider, SecretScanner, SourceScanner};

/// Embedding batch size kept small so local embedders stay responsive.
const EMBED_BATCH: usize = 16;

#[derive(Debug, Default, Serialize)]
pub struct IndexReport {
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub chunks_created: usize,
    pub files_with_secrets_redacted: usize,
    pub embeddings_created: usize,
    pub embedding_available: bool,
}

pub struct IndexWorkspaceUseCase {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    documents: Arc<dyn DocumentRepository>,
    scanner: Arc<dyn SourceScanner>,
    secret_scanner: Arc<dyn SecretScanner>,
    chunker: Arc<dyn Chunker>,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl IndexWorkspaceUseCase {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        documents: Arc<dyn DocumentRepository>,
        scanner: Arc<dyn SourceScanner>,
        secret_scanner: Arc<dyn SecretScanner>,
        chunker: Arc<dyn Chunker>,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            documents,
            scanner,
            secret_scanner,
            chunker,
            embedder,
        }
    }

    /// Index every repository of the workspace. Document paths are stored as
    /// "<repository name>/<path in repo>" so citations show their origin and
    /// paths stay unique across repositories.
    pub async fn execute(&self, workspace_id: &str) -> DomainResult<IndexReport> {
        self.workspaces.find_by_id(workspace_id)?;
        let repositories = self.repositories.list_by_workspace(workspace_id)?;
        let embedding_available = self.embedder.is_available().await;

        let mut report = IndexReport {
            embedding_available,
            ..Default::default()
        };

        for repository in &repositories {
            let files = self.scanner.scan(&repository.root_path)?;
            self.index_files(workspace_id, &repository.id, &repository.name, files, &mut report)
                .await?;
        }

        Ok(report)
    }

    async fn index_files(
        &self,
        workspace_id: &str,
        repository_id: &str,
        repository_name: &str,
        files: Vec<crate::domain::services::SourceFile>,
        report: &mut IndexReport,
    ) -> DomainResult<()> {
        for file in files {
            // Single-file sources already carry the repository name as their
            // path — avoid "notes.md/notes.md".
            let rel_path = if file.rel_path == repository_name {
                file.rel_path.clone()
            } else {
                format!("{repository_name}/{}", file.rel_path)
            };
            let file = crate::domain::services::SourceFile { rel_path, ..file };
            // Incremental: skip files whose content hash is unchanged. When a
            // file did change, keep its document id stable across re-indexing.
            let existing = self
                .documents
                .find_by_path(workspace_id, &file.rel_path)?;
            if let Some(ref doc) = existing {
                if doc.content_hash == file.content_hash {
                    report.files_unchanged += 1;
                    continue;
                }
            }

            // Secrets must never enter the index: redact before chunking.
            let findings = self.secret_scanner.scan(&file.content);
            let content = if findings.is_empty() {
                file.content.clone()
            } else {
                report.files_with_secrets_redacted += 1;
                self.secret_scanner.redact(&file.content)
            };

            let document = Document {
                id: existing
                    .map(|d| d.id)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                workspace_id: workspace_id.to_string(),
                repository_id: repository_id.to_string(),
                rel_path: file.rel_path.clone(),
                language: file.language.clone(),
                content_hash: file.content_hash.clone(),
                indexed_at: chrono::Utc::now().to_rfc3339(),
            };
            self.documents.upsert_document(&document)?;

            let chunks: Vec<Chunk> = self
                .chunker
                .chunk(&content, &file.language)
                .into_iter()
                .enumerate()
                .map(|(seq, c)| Chunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    document_id: document.id.clone(),
                    workspace_id: workspace_id.to_string(),
                    seq: seq as i64,
                    content: c.content,
                    start_line: c.start_line as i64,
                    end_line: c.end_line as i64,
                })
                .collect();
            self.documents.replace_chunks(&document.id, &chunks)?;
            report.chunks_created += chunks.len();
            report.files_indexed += 1;

            if report.embedding_available {
                for batch in chunks.chunks(EMBED_BATCH) {
                    let texts: Vec<String> = batch
                        .iter()
                        .map(|c| format!("{}\n{}", file.rel_path, c.content))
                        .collect();
                    match self.embedder.embed(&texts).await {
                        Ok(vectors) => {
                            for (chunk, vector) in batch.iter().zip(vectors.iter()) {
                                self.documents.store_embedding(&chunk.id, vector)?;
                                report.embeddings_created += 1;
                            }
                        }
                        // Embeddings are an enhancement; keyword search still
                        // works, so indexing must not fail because of them.
                        Err(_) => break,
                    }
                }
            }
        }

        Ok(())
    }
}
