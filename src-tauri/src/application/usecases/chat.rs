use std::sync::Arc;

use crate::domain::entities::chat::{ChatSession, Message};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::ChatRepository;

pub struct ChatUseCases {
    chats: Arc<dyn ChatRepository>,
}

impl ChatUseCases {
    pub fn new(chats: Arc<dyn ChatRepository>) -> Self {
        Self { chats }
    }

    pub fn create_session(&self, workspace_id: &str, title: &str) -> DomainResult<ChatSession> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::Validation("session title is empty".into()));
        }
        let session = ChatSession {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            title: title.chars().take(80).collect(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.create_session(&session)?;
        Ok(session)
    }

    pub fn list_sessions(&self, workspace_id: &str) -> DomainResult<Vec<ChatSession>> {
        self.chats.list_sessions(workspace_id)
    }

    pub fn list_messages(&self, session_id: &str) -> DomainResult<Vec<Message>> {
        self.chats.list_messages(session_id)
    }
}
