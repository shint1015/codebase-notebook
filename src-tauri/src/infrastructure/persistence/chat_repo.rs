use rusqlite::params;

use super::{storage_err, Db};
use crate::domain::entities::chat::{ChatSession, Citation, Message, Role};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::ChatRepository;

pub struct SqliteChatRepository {
    db: Db,
}

impl SqliteChatRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl ChatRepository for SqliteChatRepository {
    fn create_session(&self, session: &ChatSession) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "INSERT INTO chat_sessions (id, workspace_id, title, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    session.id,
                    session.workspace_id,
                    session.title,
                    session.created_at
                ],
            )
            .map_err(storage_err("insert session"))?;
        Ok(())
    }

    fn list_sessions(&self, workspace_id: &str) -> DomainResult<Vec<ChatSession>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, workspace_id, title, created_at
                 FROM chat_sessions WHERE workspace_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(storage_err("prepare list sessions"))?;
        let rows = stmt
            .query_map(params![workspace_id], |row| {
                Ok(ChatSession {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    title: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(storage_err("list sessions"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(storage_err("read sessions"))
    }

    fn rename_session(&self, session_id: &str, title: &str) -> DomainResult<()> {
        let changed = self
            .db
            .lock()
            .execute(
                "UPDATE chat_sessions SET title = ?2 WHERE id = ?1",
                params![session_id, title],
            )
            .map_err(storage_err("rename session"))?;
        if changed == 0 {
            return Err(DomainError::NotFound(format!("session {session_id}")));
        }
        Ok(())
    }

    fn delete_session(&self, session_id: &str) -> DomainResult<()> {
        self.db
            .lock()
            .execute(
                "DELETE FROM chat_sessions WHERE id = ?1",
                params![session_id],
            )
            .map_err(storage_err("delete session"))?;
        Ok(())
    }

    fn append_message(&self, message: &Message) -> DomainResult<()> {
        let citations_json = serde_json::to_string(&message.citations)
            .map_err(|e| DomainError::Storage(format!("serialize citations: {e}")))?;
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        self.db
            .lock()
            .execute(
                "INSERT INTO messages
                 (id, session_id, role, content, citations_json, provider, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    message.id,
                    message.session_id,
                    role,
                    message.content,
                    citations_json,
                    message.provider,
                    message.model,
                    message.created_at,
                ],
            )
            .map_err(storage_err("insert message"))?;
        Ok(())
    }

    fn list_messages(&self, session_id: &str) -> DomainResult<Vec<Message>> {
        let conn = self.db.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, role, content, citations_json, provider, model, created_at
                 FROM messages WHERE session_id = ?1 ORDER BY created_at, rowid",
            )
            .map_err(storage_err("prepare list messages"))?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                let role_str: String = row.get(2)?;
                let citations_json: String = row.get(4)?;
                Ok((
                    Message {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        role: if role_str == "user" {
                            Role::User
                        } else {
                            Role::Assistant
                        },
                        content: row.get(3)?,
                        citations: Vec::new(),
                        provider: row.get(5)?,
                        model: row.get(6)?,
                        created_at: row.get(7)?,
                    },
                    citations_json,
                ))
            })
            .map_err(storage_err("list messages"))?;
        let mut messages = Vec::new();
        for row in rows {
            let (mut message, citations_json) = row.map_err(storage_err("read message"))?;
            message.citations = serde_json::from_str::<Vec<Citation>>(&citations_json)
                .unwrap_or_default();
            messages.push(message);
        }
        Ok(messages)
    }
}
