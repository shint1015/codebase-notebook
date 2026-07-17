use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::entities::chat::{ChatSession, Message};
use crate::domain::entities::repository::{Repository, SourceKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::{ChatRepository, RepositoryRepository, WorkspaceRepository};

/// Version of the export format. Bump on a breaking change; import refuses
/// anything newer than it understands.
const FORMAT_VERSION: u32 = 1;

/// A portable snapshot of a workspace: its configuration, sources, notes and
/// chats. The *index* is deliberately excluded — it's derived data that is
/// rebuilt by re-indexing, and it would bloat the file enormously.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceExport {
    pub format_version: u32,
    pub exported_at: String,
    pub name: String,
    pub instructions: String,
    pub repositories: Vec<ExportedRepository>,
    pub notes: Vec<ExportedNote>,
    pub chats: Vec<ExportedChat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedRepository {
    pub name: String,
    pub root_path: String,
    pub remote_url: Option<String>,
    pub source_kind: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedNote {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedChat {
    pub title: String,
    pub created_at: String,
    pub messages: Vec<Message>,
}

pub struct BackupUseCases {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    chats: Arc<dyn ChatRepository>,
    notes: Arc<super::notes::NotesUseCases>,
}

impl BackupUseCases {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        chats: Arc<dyn ChatRepository>,
        notes: Arc<super::notes::NotesUseCases>,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            chats,
            notes,
        }
    }

    pub fn export(&self, workspace_id: &str) -> DomainResult<WorkspaceExport> {
        let workspace = self.workspaces.find_by_id(workspace_id)?;

        let repositories = self
            .repositories
            .list_by_workspace(workspace_id)?
            .into_iter()
            // Notes travel in their own section, rebuilt on import.
            .filter(|r| r.name != "notes")
            .map(|r| ExportedRepository {
                name: r.name,
                root_path: r.root_path,
                remote_url: r.remote_url,
                source_kind: r.source_kind.as_str().to_string(),
            })
            .collect();

        let notes = self
            .notes
            .list(workspace_id)?
            .into_iter()
            .filter_map(|n| {
                self.notes
                    .read(workspace_id, &n.name)
                    .ok()
                    .map(|content| ExportedNote {
                        name: n.name,
                        content,
                    })
            })
            .collect();

        let mut chats = Vec::new();
        for session in self.chats.list_sessions(workspace_id)? {
            chats.push(ExportedChat {
                title: session.title,
                created_at: session.created_at,
                messages: self.chats.list_messages(&session.id)?,
            });
        }

        Ok(WorkspaceExport {
            format_version: FORMAT_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            name: workspace.name,
            instructions: workspace.instructions,
            repositories,
            notes,
            chats,
        })
    }

    /// Recreate a workspace from an export. Sources are re-registered by
    /// path/URL — the caller should re-index afterwards to rebuild the index.
    /// Returns the new workspace id.
    pub fn import(&self, export: WorkspaceExport) -> DomainResult<String> {
        if export.format_version > FORMAT_VERSION {
            return Err(DomainError::Validation(format!(
                "this export is from a newer version (format {}); update the app first",
                export.format_version
            )));
        }
        let workspace = crate::domain::entities::workspace::Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{} (imported)", export.name),
            allow_external: false,
            instructions: export.instructions,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.workspaces.create(&workspace)?;

        for repo in export.repositories {
            // A source whose files are gone (different machine) is skipped
            // rather than failing the whole import.
            let kind = SourceKind::parse(&repo.source_kind);
            if kind == SourceKind::Local && !std::path::Path::new(&repo.root_path).exists() {
                continue;
            }
            let entry = Repository {
                id: uuid::Uuid::new_v4().to_string(),
                workspace_id: workspace.id.clone(),
                name: repo.name,
                root_path: repo.root_path,
                remote_url: repo.remote_url,
                source_kind: kind,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            self.repositories.create(&entry).ok();
        }

        for note in export.notes {
            self.notes.save(&workspace.id, &note.name, &note.content)?;
        }

        for chat in export.chats {
            let session = ChatSession {
                id: uuid::Uuid::new_v4().to_string(),
                workspace_id: workspace.id.clone(),
                title: chat.title,
                created_at: chat.created_at,
            };
            self.chats.create_session(&session)?;
            for message in chat.messages {
                self.chats.append_message(&Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session.id.clone(),
                    ..message
                })?;
            }
        }

        Ok(workspace.id)
    }
}
