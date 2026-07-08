pub mod chat_repo;
pub mod document_repo;
pub mod provider_repo;
pub mod workspace_repo;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::domain::error::{DomainError, DomainResult};

/// Shared SQLite handle. All repositories borrow the same connection guarded
/// by a mutex — plenty for a single-user desktop app.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> DomainResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DomainError::Storage(format!("create db dir: {e}")))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| DomainError::Storage(format!("open db: {e}")))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> DomainResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DomainError::Storage(format!("open db: {e}")))?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().expect("db mutex poisoned")
    }

    fn migrate(&self) -> DomainResult<()> {
        let conn = self.lock();
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS workspaces (
                id             TEXT PRIMARY KEY,
                name           TEXT NOT NULL,
                root_path      TEXT NOT NULL,
                allow_external INTEGER NOT NULL DEFAULT 0,
                created_at     TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS documents (
                id           TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                rel_path     TEXT NOT NULL,
                language     TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                indexed_at   TEXT NOT NULL,
                UNIQUE(workspace_id, rel_path)
            );

            CREATE TABLE IF NOT EXISTS chunks (
                id           TEXT PRIMARY KEY,
                document_id  TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                workspace_id TEXT NOT NULL,
                seq          INTEGER NOT NULL,
                content      TEXT NOT NULL,
                start_line   INTEGER NOT NULL,
                end_line     INTEGER NOT NULL,
                embedding    BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_workspace ON chunks(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);

            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content,
                chunk_id UNINDEXED
            );

            CREATE TABLE IF NOT EXISTS chat_sessions (
                id           TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                title        TEXT NOT NULL,
                created_at   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id             TEXT PRIMARY KEY,
                session_id     TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
                role           TEXT NOT NULL,
                content        TEXT NOT NULL,
                citations_json TEXT NOT NULL DEFAULT '[]',
                provider       TEXT,
                model          TEXT,
                created_at     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);

            CREATE TABLE IF NOT EXISTS provider_configs (
                kind            TEXT PRIMARY KEY,
                enabled         INTEGER NOT NULL,
                base_url        TEXT NOT NULL,
                default_model   TEXT NOT NULL,
                allow_send_code INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|e| DomainError::Storage(format!("migrate: {e}")))?;
        Ok(())
    }
}

pub(crate) fn storage_err<E: std::fmt::Display>(context: &str) -> impl Fn(E) -> DomainError + '_ {
    move |e| DomainError::Storage(format!("{context}: {e}"))
}

/// Encode an f32 vector as little-endian bytes for BLOB storage.
pub(crate) fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub(crate) fn decode_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_roundtrip() {
        let v = vec![1.5f32, -0.25, 0.0, 3.4e5];
        assert_eq!(decode_embedding(&encode_embedding(&v)), v);
    }

    #[test]
    fn migrations_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
    }
}
