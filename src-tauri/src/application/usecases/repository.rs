use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::entities::repository::{Repository, SourceKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::{
    DocumentRepository, RepositoryRepository, WorkspaceRepository,
};
use crate::domain::services::{IssueDoc, IssueFetcher, RepoCloner};

pub struct RepositoryUseCases {
    workspaces: Arc<dyn WorkspaceRepository>,
    repositories: Arc<dyn RepositoryRepository>,
    documents: Arc<dyn DocumentRepository>,
    cloner: Arc<dyn RepoCloner>,
    issue_fetcher: Arc<dyn IssueFetcher>,
    /// App-managed directory where git clones and fetched issues live
    /// (<app data>/repos/<workspace_id>/<name>).
    clones_dir: PathBuf,
}

impl RepositoryUseCases {
    pub fn new(
        workspaces: Arc<dyn WorkspaceRepository>,
        repositories: Arc<dyn RepositoryRepository>,
        documents: Arc<dyn DocumentRepository>,
        cloner: Arc<dyn RepoCloner>,
        issue_fetcher: Arc<dyn IssueFetcher>,
        clones_dir: PathBuf,
    ) -> Self {
        Self {
            workspaces,
            repositories,
            documents,
            cloner,
            issue_fetcher,
            clones_dir,
        }
    }

    pub fn list(&self, workspace_id: &str) -> DomainResult<Vec<Repository>> {
        self.repositories.list_by_workspace(workspace_id)
    }

    /// Register an existing local folder or single file as a repository of
    /// the workspace.
    pub fn add_local(&self, workspace_id: &str, root_path: &str) -> DomainResult<Repository> {
        self.workspaces.find_by_id(workspace_id)?;
        let path = std::path::Path::new(root_path);
        if !path.is_dir() && !path.is_file() {
            return Err(DomainError::Validation(format!(
                "no such file or directory: {root_path}"
            )));
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string();
        self.create_entry(workspace_id, &name, root_path, None, SourceKind::Local)
    }

    /// Fetch a GitHub repository's issues and materialize them as markdown
    /// files under the app-managed directory, registered as a repository.
    /// Accepts "owner/repo" or a full GitHub URL.
    pub async fn add_github_issues(
        &self,
        workspace_id: &str,
        spec: &str,
    ) -> DomainResult<Repository> {
        self.workspaces.find_by_id(workspace_id)?;
        let spec = normalize_github_spec(spec).ok_or_else(|| {
            DomainError::Validation(format!(
                "cannot parse GitHub repository from \"{spec}\" — use owner/repo"
            ))
        })?;
        let issues = self.issue_fetcher.fetch_issues(&spec).await?;
        if issues.is_empty() {
            return Err(DomainError::Validation(format!(
                "{spec} has no issues to import"
            )));
        }

        let name = format!("{}-issues", spec.rsplit('/').next().unwrap_or("repo"));
        let dir = self.clones_dir.join(workspace_id).join(&name);
        if dir.exists() {
            return Err(DomainError::Validation(format!(
                "issues folder already exists: {}",
                dir.display()
            )));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| DomainError::Indexing(format!("create issues dir: {e}")))?;
        for issue in &issues {
            let file = dir.join(format!("issue-{:05}.md", issue.number));
            std::fs::write(&file, issue_markdown(issue))
                .map_err(|e| DomainError::Indexing(format!("write issue file: {e}")))?;
        }

        let dir_str = dir.to_string_lossy().to_string();
        match self.create_entry(
            workspace_id,
            &name,
            &dir_str,
            Some(format!("https://github.com/{spec}/issues")),
            SourceKind::GithubIssues,
        ) {
            Ok(repository) => Ok(repository),
            Err(error) => {
                std::fs::remove_dir_all(&dir).ok();
                Err(error)
            }
        }
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

        match self.create_entry(workspace_id, &name, &dest_str, Some(url), SourceKind::Git) {
            Ok(repository) => Ok(repository),
            Err(error) => {
                // Roll back the clone if the DB entry could not be created
                // (e.g. duplicate name).
                self.cloner.remove_clone(&dest_str).ok();
                Err(error)
            }
        }
    }

    /// Remove a repository from its workspace: indexed data always, on-disk
    /// files only when the app owns them (clones and fetched issues).
    pub fn remove(&self, repository_id: &str) -> DomainResult<()> {
        let repository = self.repositories.find_by_id(repository_id)?;
        self.documents.delete_by_repository(repository_id)?;
        self.repositories.delete(repository_id)?;
        if repository.source_kind.is_managed() {
            self.cloner.remove_clone(&repository.root_path)?;
        }
        Ok(())
    }

    /// Resolve a citation path ("repo/rel/path", or a single-file source's
    /// bare name) to an absolute path on disk.
    pub fn resolve_source_path(
        &self,
        workspace_id: &str,
        rel_path: &str,
    ) -> DomainResult<String> {
        let repositories = self.repositories.list_by_workspace(workspace_id)?;
        for repository in &repositories {
            if rel_path == repository.name {
                return Ok(repository.root_path.clone());
            }
            if let Some(rest) = rel_path.strip_prefix(&format!("{}/", repository.name)) {
                return Ok(std::path::Path::new(&repository.root_path)
                    .join(rest)
                    .to_string_lossy()
                    .to_string());
            }
        }
        Err(DomainError::NotFound(format!(
            "no source matches {rel_path}"
        )))
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
        source_kind: SourceKind,
    ) -> DomainResult<Repository> {
        let repository = Repository {
            id: uuid::Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            name: name.to_string(),
            root_path: root_path.to_string(),
            remote_url,
            source_kind,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.repositories.create(&repository)?;
        Ok(repository)
    }
}

/// "owner/repo", "https://github.com/owner/repo(.git|/issues|/wiki)" -> "owner/repo"
pub(crate) fn normalize_github_spec(input: &str) -> Option<String> {
    let mut s = input.trim().trim_end_matches('/').to_string();
    for prefix in ["https://github.com/", "http://github.com/", "github.com/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    if let Some(rest) = s.strip_prefix("git@github.com:") {
        s = rest.to_string();
    }
    for suffix in ["/issues", "/wiki", ".git"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.to_string();
        }
    }
    let parts: Vec<&str> = s.split('/').collect();
    let valid = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if parts.len() == 2 && parts.iter().all(|p| valid(p)) {
        Some(s)
    } else {
        None
    }
}

/// Render one issue as a markdown document with searchable metadata.
fn issue_markdown(issue: &IssueDoc) -> String {
    let labels = if issue.labels.is_empty() {
        "-".to_string()
    } else {
        issue.labels.join(", ")
    };
    format!(
        "# #{number}: {title}\n\n\
         - state: {state}\n\
         - author: {author}\n\
         - labels: {labels}\n\
         - created: {created}\n\
         - url: {url}\n\n\
         {body}\n",
        number = issue.number,
        title = issue.title,
        state = issue.state,
        author = issue.author,
        labels = labels,
        created = issue.created_at,
        url = issue.url,
        body = issue.body,
    )
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
    use super::{issue_markdown, normalize_github_spec, repo_name_from_url};
    use crate::domain::services::IssueDoc;

    #[test]
    fn normalizes_github_specs() {
        for input in [
            "owner/repo",
            "https://github.com/owner/repo",
            "https://github.com/owner/repo.git",
            "https://github.com/owner/repo/issues",
            "git@github.com:owner/repo.git",
            "github.com/owner/repo/",
        ] {
            assert_eq!(
                normalize_github_spec(input).as_deref(),
                Some("owner/repo"),
                "failed for {input}"
            );
        }
        assert_eq!(normalize_github_spec("not a spec"), None);
        assert_eq!(normalize_github_spec("https://github.com/owner"), None);
    }

    #[test]
    fn renders_issue_markdown_with_metadata() {
        let issue = IssueDoc {
            number: 42,
            title: "Login fails on Safari".into(),
            state: "open".into(),
            author: "octocat".into(),
            labels: vec!["bug".into(), "p1".into()],
            body: "Steps to reproduce...".into(),
            url: "https://github.com/o/r/issues/42".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let md = issue_markdown(&issue);
        assert!(md.contains("# #42: Login fails on Safari"));
        assert!(md.contains("- labels: bug, p1"));
        assert!(md.contains("Steps to reproduce..."));
    }

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
