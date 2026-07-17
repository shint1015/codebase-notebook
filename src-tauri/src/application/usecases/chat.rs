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

    /// Duplicate a session into a new one so the user can branch a
    /// conversation without disturbing the original. When `up_to_message_id`
    /// is given, only messages up to and including that one are copied — this
    /// branches the conversation from a chosen point.
    pub fn fork_session(
        &self,
        session_id: &str,
        up_to_message_id: Option<&str>,
    ) -> DomainResult<ChatSession> {
        let source = self.chats.find_session(session_id)?;
        let title: String = format!("{} (fork)", source.title)
            .chars()
            .take(80)
            .collect();
        let forked = ChatSession {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: source.workspace_id,
            title,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.chats.create_session(&forked)?;
        for message in self.chats.list_messages(session_id)? {
            let stop = up_to_message_id == Some(message.id.as_str());
            let copy = Message {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: forked.id.clone(),
                ..message
            };
            self.chats.append_message(&copy)?;
            if stop {
                break;
            }
        }
        Ok(forked)
    }

    pub fn rename_session(&self, session_id: &str, title: &str) -> DomainResult<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::Validation("session title is empty".into()));
        }
        self.chats
            .rename_session(session_id, &title.chars().take(80).collect::<String>())
    }

    pub fn delete_session(&self, session_id: &str) -> DomainResult<()> {
        self.chats.delete_session(session_id)
    }

    pub fn list_messages(&self, session_id: &str) -> DomainResult<Vec<Message>> {
        self.chats.list_messages(session_id)
    }

    pub fn search_messages(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<crate::domain::repositories::ChatSearchHit>> {
        self.chats.search_messages(workspace_id, query, limit)
    }

    /// Render a session as a markdown transcript (for export).
    pub fn export_markdown(&self, session_id: &str) -> DomainResult<String> {
        let messages = self.chats.list_messages(session_id)?;
        let mut output = String::new();
        for message in &messages {
            match message.role {
                crate::domain::entities::chat::Role::User => {
                    output.push_str(&format!("## 🧑 Question\n\n{}\n\n", message.content));
                }
                crate::domain::entities::chat::Role::Assistant => {
                    let model = match (&message.provider, &message.model) {
                        (Some(p), Some(m)) => format!(" ({p} · {m})"),
                        _ => String::new(),
                    };
                    output.push_str(&format!("## 🤖 Answer{model}\n\n{}\n\n", message.content));
                    if !message.citations.is_empty() {
                        output.push_str("Sources:\n");
                        for c in &message.citations {
                            output.push_str(&format!(
                                "- [{}] {} (lines {}-{})\n",
                                c.marker, c.rel_path, c.start_line, c.end_line
                            ));
                        }
                        output.push('\n');
                    }
                }
            }
        }
        Ok(output)
    }
}
