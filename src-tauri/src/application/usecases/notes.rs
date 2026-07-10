use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::domain::entities::repository::{Repository, SourceKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::{RepositoryRepository, WorkspaceRepository};

/// In-app markdown documents ("notes"). Each workspace has one app-managed
/// notes folder, registered as a repository so notes are indexed and citable
/// like any other source.
pub struct NotesUseCases {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    /// Root of app-managed source dirs: notes live at <root>/<workspace>/notes.
    clones_dir: PathBuf,
}

const NOTES_REPO_NAME: &str = "notes";

#[derive(Debug, Serialize)]
pub struct NoteFile {
    /// File name including the .md extension (unique within the workspace).
    pub name: String,
    pub updated_at: String,
}

impl NotesUseCases {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        clones_dir: PathBuf,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            clones_dir,
        }
    }

    fn notes_dir(&self, workspace_id: &str) -> PathBuf {
        self.clones_dir.join(workspace_id).join(NOTES_REPO_NAME)
    }

    /// Ensure the notes folder exists and is registered as a repository.
    fn ensure_notes_repo(&self, workspace_id: &str) -> DomainResult<()> {
        self.workspaces.find_by_id(workspace_id)?;
        let dir = self.notes_dir(workspace_id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| DomainError::Indexing(format!("create notes dir: {e}")))?;
        let exists = self
            .repositories
            .list_by_workspace(workspace_id)?
            .into_iter()
            .any(|r| r.name == NOTES_REPO_NAME);
        if !exists {
            let repository = Repository {
                id: uuid::Uuid::new_v4().to_string(),
                workspace_id: workspace_id.to_string(),
                name: NOTES_REPO_NAME.to_string(),
                root_path: dir.to_string_lossy().to_string(),
                remote_url: None,
                source_kind: SourceKind::Local,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            self.repositories.create(&repository)?;
        }
        Ok(())
    }

    pub fn list(&self, workspace_id: &str) -> DomainResult<Vec<NoteFile>> {
        let dir = self.notes_dir(workspace_id);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut notes = Vec::new();
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| DomainError::Indexing(format!("read notes dir: {e}")))?
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let updated_at = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                })
                .map(|secs| {
                    chrono::DateTime::from_timestamp(secs as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            notes.push(NoteFile {
                name: name.to_string(),
                updated_at,
            });
        }
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(notes)
    }

    pub fn read(&self, workspace_id: &str, name: &str) -> DomainResult<String> {
        let path = self.notes_dir(workspace_id).join(safe_name(name)?);
        std::fs::read_to_string(&path)
            .map_err(|_| DomainError::NotFound(format!("note {name}")))
    }

    /// Create or overwrite a note. Returns the stored file name.
    pub fn save(&self, workspace_id: &str, name: &str, content: &str) -> DomainResult<String> {
        self.ensure_notes_repo(workspace_id)?;
        let file_name = safe_name(name)?;
        let path = self.notes_dir(workspace_id).join(&file_name);
        std::fs::write(&path, content)
            .map_err(|e| DomainError::Indexing(format!("write note: {e}")))?;
        Ok(file_name)
    }

    pub fn delete(&self, workspace_id: &str, name: &str) -> DomainResult<()> {
        let path = self.notes_dir(workspace_id).join(safe_name(name)?);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| DomainError::Indexing(format!("delete note: {e}")))?;
        }
        Ok(())
    }
}

/// Turn a user title/name into a safe `<slug>.md` file name — no path
/// traversal, no separators.
fn safe_name(name: &str) -> DomainResult<String> {
    let base = name.trim().trim_end_matches(".md");
    let slug: String = base
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let slug = slug.trim_matches(['.', '-']).to_string();
    if slug.is_empty() || !slug.chars().any(|c| c.is_alphanumeric()) {
        return Err(DomainError::Validation(
            "document name must contain letters or digits".into(),
        ));
    }
    Ok(format!("{slug}.md"))
}

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn sanitizes_names() {
        assert_eq!(safe_name("My Design Doc").unwrap(), "My-Design-Doc.md");
        assert_eq!(safe_name("notes.md").unwrap(), "notes.md");
        assert_eq!(safe_name("../../etc/passwd").unwrap(), "etcpasswd.md");
        assert!(safe_name("   ").is_err());
        assert!(safe_name("///").is_err());
    }
}
