use rusqlite::params;

use super::{decode_embedding, encode_embedding, storage_err, Db};
use crate::domain::entities::chunk::{Chunk, SearchHit};
use crate::domain::entities::document::Document;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::DocumentRepository;

pub struct SqliteDocumentRepository {
    db: Db,
}

impl SqliteDocumentRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn row_to_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        repository_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        rel_path: row.get(3)?,
        language: row.get(4)?,
        content_hash: row.get(5)?,
        indexed_at: row.get(6)?,
    })
}

fn row_to_hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        chunk: Chunk {
            id: row.get(0)?,
            document_id: row.get(1)?,
            workspace_id: row.get(2)?,
            seq: row.get(3)?,
            content: row.get(4)?,
            start_line: row.get(5)?,
            end_line: row.get(6)?,
        },
        rel_path: row.get(7)?,
        score: row.get(8)?,
    })
}

/// Turn free text into a safe FTS5 query: each alphanumeric token is quoted
/// and OR-ed so user input can never be parsed as FTS syntax.
fn fts_query(input: &str) -> Option<String> {
    let tokens: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .take(24)
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

impl DocumentRepository for SqliteDocumentRepository {
    fn upsert_document(&self, document: &Document) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO documents (id, workspace_id, repository_id, rel_path, language, content_hash, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(workspace_id, rel_path) DO UPDATE SET
                   repository_id = excluded.repository_id,
                   language = excluded.language,
                   content_hash = excluded.content_hash,
                   indexed_at = excluded.indexed_at",
                params![
                    document.id,
                    document.workspace_id,
                    document.repository_id,
                    document.rel_path,
                    document.language,
                    document.content_hash,
                    document.indexed_at,
                ],
            )
            .map_err(storage_err("upsert document"))?;
        Ok(())
    }

    fn find_by_path(&self, workspace_id: &str, rel_path: &str) -> DomainResult<Option<Document>> {
        self.db
            .lock()
            .query_row(
                "SELECT id, workspace_id, repository_id, rel_path, language, content_hash, indexed_at
                 FROM documents WHERE workspace_id = ?1 AND rel_path = ?2",
                params![workspace_id, rel_path],
                row_to_document,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(DomainError::Storage(format!("find document: {other}"))),
            })
    }

    fn list_by_workspace(&self, workspace_id: &str) -> DomainResult<Vec<Document>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, repository_id, rel_path, language, content_hash, indexed_at
                 FROM documents WHERE workspace_id = ?1 ORDER BY rel_path",
            )
            .map_err(storage_err("prepare list documents"))?;
        let rows = stmt
            .query_map(params![workspace_id], row_to_document)
            .map_err(storage_err("list documents"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read documents"))
    }

    fn count_by_workspace(&self, workspace_id: &str) -> DomainResult<i64> {
        self.db
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE workspace_id = ?1",
                params![workspace_id],
                |row| row.get(0),
            )
            .map_err(storage_err("count documents"))
    }

    fn delete_by_repository(&self, repository_id: &str) -> DomainResult<()> {
        let mut conn = self.db.lock();
        let tx = conn
            .transaction()
            .map_err(storage_err("begin delete by repository"))?;
        tx.execute(
            "DELETE FROM chunks_fts WHERE chunk_id IN
             (SELECT c.id FROM chunks c
              JOIN documents d ON d.id = c.document_id
              WHERE d.repository_id = ?1)",
            params![repository_id],
        )
        .map_err(storage_err("delete repository fts"))?;
        tx.execute(
            "DELETE FROM documents WHERE repository_id = ?1",
            params![repository_id],
        )
        .map_err(storage_err("delete repository documents"))?;
        tx.commit().map_err(storage_err("commit delete by repository"))
    }

    fn replace_chunks(&self, document_id: &str, chunks: &[Chunk]) -> DomainResult<()> {
        let mut conn = self.db.lock();
        let tx = conn
            .transaction()
            .map_err(storage_err("begin replace chunks"))?;
        tx.execute(
            "DELETE FROM chunks_fts WHERE chunk_id IN
             (SELECT id FROM chunks WHERE document_id = ?1)",
            params![document_id],
        )
        .map_err(storage_err("delete fts"))?;
        tx.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            params![document_id],
        )
        .map_err(storage_err("delete chunks"))?;
        for chunk in chunks {
            tx.execute(
                "INSERT INTO chunks (id, document_id, workspace_id, seq, content, start_line, end_line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    chunk.id,
                    chunk.document_id,
                    chunk.workspace_id,
                    chunk.seq,
                    chunk.content,
                    chunk.start_line,
                    chunk.end_line,
                ],
            )
            .map_err(storage_err("insert chunk"))?;
            tx.execute(
                "INSERT INTO chunks_fts (content, chunk_id) VALUES (?1, ?2)",
                params![chunk.content, chunk.id],
            )
            .map_err(storage_err("insert fts"))?;
        }
        tx.commit().map_err(storage_err("commit replace chunks"))
    }

    fn store_embedding(&self, chunk_id: &str, embedding: &[f32]) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "UPDATE chunks SET embedding = ?2 WHERE id = ?1",
                params![chunk_id, encode_embedding(embedding)],
            )
            .map_err(storage_err("store embedding"))?;
        Ok(())
    }

    fn get_chunk(&self, chunk_id: &str) -> DomainResult<Chunk> {
        self.db
            .lock()
            .query_row(
                "SELECT id, document_id, workspace_id, seq, content, start_line, end_line
                 FROM chunks WHERE id = ?1",
                params![chunk_id],
                |row| {
                    Ok(Chunk {
                        id: row.get(0)?,
                        document_id: row.get(1)?,
                        workspace_id: row.get(2)?,
                        seq: row.get(3)?,
                        content: row.get(4)?,
                        start_line: row.get(5)?,
                        end_line: row.get(6)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DomainError::NotFound(format!("chunk {chunk_id}"))
                }
                other => DomainError::Storage(format!("get chunk: {other}")),
            })
    }

    fn count_chunks(&self, workspace_id: &str) -> DomainResult<i64> {
        self.db
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE workspace_id = ?1",
                params![workspace_id],
                |row| row.get(0),
            )
            .map_err(storage_err("count chunks"))
    }

    fn search_keyword(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<SearchHit>> {
        let Some(match_query) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.document_id, c.workspace_id, c.seq, c.content,
                        c.start_line, c.end_line, d.rel_path,
                        -bm25(chunks_fts) AS score
                 FROM chunks_fts f
                 JOIN chunks c ON c.id = f.chunk_id
                 JOIN documents d ON d.id = c.document_id
                 WHERE chunks_fts MATCH ?1 AND c.workspace_id = ?2
                 ORDER BY bm25(chunks_fts)
                 LIMIT ?3",
            )
            .map_err(storage_err("prepare fts search"))?;
        let rows = stmt
            .query_map(params![match_query, workspace_id, limit as i64], row_to_hit)
            .map_err(storage_err("fts search"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read search hits"))
    }

    fn embeddings_by_workspace(
        &self,
        workspace_id: &str,
    ) -> DomainResult<Vec<(String, Vec<f32>)>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, embedding FROM chunks
                 WHERE workspace_id = ?1 AND embedding IS NOT NULL",
            )
            .map_err(storage_err("prepare embeddings"))?;
        let rows = stmt
            .query_map(params![workspace_id], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((id, decode_embedding(&blob)))
            })
            .map_err(storage_err("load embeddings"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read embeddings"))
    }

    fn hits_by_chunk_ids(&self, chunk_ids: &[String]) -> DomainResult<Vec<SearchHit>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=chunk_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT c.id, c.document_id, c.workspace_id, c.seq, c.content,
                    c.start_line, c.end_line, d.rel_path, 0.0 AS score
             FROM chunks c JOIN documents d ON d.id = c.document_id
             WHERE c.id IN ({placeholders})"
        );
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&sql).map_err(storage_err("prepare hits"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(chunk_ids.iter()), row_to_hit)
            .map_err(storage_err("load hits"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read hits"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::workspace::Workspace;
    use crate::domain::repositories::{RepositoryRepository, WorkspaceRepository};
    use crate::infrastructure::persistence::repository_repo::SqliteRepositoryRepository;
    use crate::infrastructure::persistence::workspace_repo::SqliteWorkspaceRepository;

    fn setup() -> (SqliteWorkspaceRepository, SqliteDocumentRepository) {
        let db = Db::open_in_memory().unwrap();
        (
            SqliteWorkspaceRepository::new(db.clone()),
            SqliteDocumentRepository::new(db),
        )
    }

    fn seed(ws_repo: &SqliteWorkspaceRepository, doc_repo: &SqliteDocumentRepository) -> String {
        let ws = Workspace {
            id: "ws1".into(),
            name: "test".into(),
            allow_external: false,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        ws_repo.create(&ws).unwrap();
        let repo = crate::domain::entities::repository::Repository {
            id: "repo1".into(),
            workspace_id: "ws1".into(),
            name: "app".into(),
            root_path: "/tmp".into(),
            remote_url: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        SqliteRepositoryRepository::new(doc_repo.db.clone())
            .create(&repo)
            .unwrap();
        let doc = Document {
            id: "doc1".into(),
            workspace_id: "ws1".into(),
            repository_id: "repo1".into(),
            rel_path: "app/src/auth.rs".into(),
            language: "rust".into(),
            content_hash: "h1".into(),
            indexed_at: "2026-01-01T00:00:00Z".into(),
        };
        doc_repo.upsert_document(&doc).unwrap();
        let chunk = Chunk {
            id: "c1".into(),
            document_id: "doc1".into(),
            workspace_id: "ws1".into(),
            seq: 0,
            content: "fn authenticate(user: &User) -> Result<Session> { validate_token(user) }"
                .into(),
            start_line: 1,
            end_line: 1,
        };
        doc_repo.replace_chunks("doc1", &[chunk]).unwrap();
        ws.id
    }

    #[test]
    fn keyword_search_finds_indexed_chunk() {
        let (ws_repo, doc_repo) = setup();
        let ws_id = seed(&ws_repo, &doc_repo);
        let hits = doc_repo
            .search_keyword(&ws_id, "how does authenticate work?", 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rel_path, "app/src/auth.rs");
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn fts_special_characters_do_not_break_search() {
        let (ws_repo, doc_repo) = setup();
        let ws_id = seed(&ws_repo, &doc_repo);
        // FTS5 operators and quotes in user input must be neutralized.
        let hits = doc_repo
            .search_keyword(&ws_id, "authenticate\" OR NEAR( * AND", 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn replace_chunks_is_idempotent_for_fts() {
        let (ws_repo, doc_repo) = setup();
        let ws_id = seed(&ws_repo, &doc_repo);
        let chunk = Chunk {
            id: "c2".into(),
            document_id: "doc1".into(),
            workspace_id: "ws1".into(),
            seq: 0,
            content: "fn authenticate() {} // v2".into(),
            start_line: 1,
            end_line: 1,
        };
        doc_repo.replace_chunks("doc1", &[chunk]).unwrap();
        let hits = doc_repo.search_keyword(&ws_id, "authenticate", 10).unwrap();
        assert_eq!(hits.len(), 1, "old FTS rows must be gone");
        assert_eq!(hits[0].chunk.id, "c2");
    }

    #[test]
    fn embeddings_roundtrip_through_workspace_query() {
        let (ws_repo, doc_repo) = setup();
        let ws_id = seed(&ws_repo, &doc_repo);
        doc_repo.store_embedding("c1", &[0.1, 0.2, 0.3]).unwrap();
        let embeddings = doc_repo.embeddings_by_workspace(&ws_id).unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].0, "c1");
        assert_eq!(embeddings[0].1.len(), 3);
    }
}
