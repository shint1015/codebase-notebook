use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::entities::repository::Repository;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::{
    DocumentRepository, RepositoryRepository, WorkspaceRepository,
};
use crate::domain::services::RepoCloner;

pub struct RepositoryUseCases {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    documents: Arc<dyn DocumentRepository>,
    cloner: Arc<dyn RepoCloner>,
    /// App-managed directory where git clones live
    /// (<app data>/repos/<workspace_id>/<name>).
    clones_dir: PathBuf,
}

impl RepositoryUseCases {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        documents: Arc<dyn DocumentRepository>,
        cloner: Arc<dyn RepoCloner>,
        clones_dir: PathBuf,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            documents,
            cloner,
            clones_dir,
        }
    }

    pub fn list(&self, workspace_id: &str) -> DomainResult<Vec<Repository>> {
        self.repositories.list_by_workspace(workspace_id)
    }

    /// Register an existing local folder as a repository of the workspace.
    pub fn add_local(&self, workspace_id: &str, root_path: &str) -> DomainResult<Repository> {
        self.workspaces.find_by_id(workspace_id)?;
        let path = std::path::Path::new(root_path);
        if !path.is_dir() {
            return Err(DomainError::Validation(format!(
                "not a directory: {root_path}"
            )));
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string();
        self.create_entry(workspace_id, &name, root_path, None)
    }

    /// Clone a remote git repository into the app-managed directory and
    /// register it. Runs the user's own `git`, so their SSH keys and
    /// credential helpers apply.
    pub async fn add_from_git(&self, workspace_id: &str, url: &str) -> DomainResult<Repository> {
        self.workspaces.find_by_id(workspace_id)?;
        let url = url.trim().to_string();
        if !(url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("git@")
            || url.starts_with("ssh://"))
        {
            return Err(DomainError::Validation(
                "repository URL must start with https://, git@ or ssh://".into(),
            ));
        }
        let name = repo_name_from_url(&url)
            .ok_or_else(|| DomainError::Validation(format!("cannot derive a name from {url}")))?;

        let dest = self.clones_dir.join(workspace_id).join(&name);
        if dest.exists() {
            return Err(DomainError::Validation(format!(
                "clone target already exists: {}",
                dest.display()
            )));
        }
        std::fs::create_dir_all(dest.parent().expect("clone dir has parent"))
            .map_err(|e| DomainError::Indexing(format!("create clones dir: {e}")))?;

        let dest_str = dest.to_string_lossy().to_string();
        // git clone is slow and blocking; keep the async runtime free.
        let cloner = self.cloner.clone();
        let clone_url = url.clone();
        let clone_dest = dest_str.clone();
        tauri::async_runtime::spawn_blocking(move || cloner.clone_repo(&clone_url, &clone_dest))
            .await
            .map_err(|e| DomainError::Indexing(format!("clone task failed: {e}")))??;

        match self.create_entry(workspace_id, &name, &dest_str, Some(url)) {
            Ok(repository) => Ok(repository),
            Err(error) => {
                // Roll back the clone if the DB entry could not be created
                // (e.g. duplicate name).
                self.cloner.remove_clone(&dest_str).ok();
                Err(error)
            }
        }
    }

    /// Remove a repository from its workspace: indexed data always, cloned
    /// files only when the app owns them (remote_url is set).
    pub fn remove(&self, repository_id: &str) -> DomainResult<()> {
        let repository = self.repositories.find_by_id(repository_id)?;
        self.documents.delete_by_repository(repository_id)?;
        self.repositories.delete(repository_id)?;
        if repository.remote_url.is_some() {
            self.cloner.remove_clone(&repository.root_path)?;
        }
        Ok(())
    }

    /// Remove all app-managed clones of a workspace (used on workspace delete).
    pub fn remove_workspace_clones(&self, workspace_id: &str) -> DomainResult<()> {
        let dir = self.clones_dir.join(workspace_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| DomainError::Indexing(format!("remove workspace clones: {e}")))?;
        }
        Ok(())
    }

    fn create_entry(
        &self,
        workspace_id: &str,
        name: &str,
        root_path: &str,
        remote_url: Option<String>,
    ) -> DomainResult<Repository> {
        let repository = Repository {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            name: name.to_string(),
            root_path: root_path.to_string(),
            remote_url,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.repositories.create(&repository)?;
        Ok(repository)
    }
}

/// "https://github.com/org/my-repo.git" -> "my-repo",
/// "git@github.com:org/my-repo.git" -> "my-repo"
fn repo_name_from_url(url: &str) -> Option<String> {
    let tail = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()?
        .trim_end_matches(".git");
    let name: String = tail
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::repo_name_from_url;

    #[test]
    fn derives_name_from_common_url_shapes() {
        assert_eq!(
            repo_name_from_url("https://github.com/org/my-repo.git").as_deref(),
            Some("my-repo")
        );
        assert_eq!(
            repo_name_from_url("git@github.com:org/my-repo.git").as_deref(),
            Some("my-repo")
        );
        assert_eq!(
            repo_name_from_url("https://github.com/org/my-repo/").as_deref(),
            Some("my-repo")
        );
        assert_eq!(repo_name_from_url("https://"), None);
    }
}
