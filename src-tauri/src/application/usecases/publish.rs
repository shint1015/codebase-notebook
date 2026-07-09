use std::sync::Arc;

use crate::application::usecases::repository::normalize_github_spec;
use crate::domain::entities::repository::{Repository, SourceKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::RepositoryRepository;
use crate::domain::services::{GitSync, IssuePublisher};

/// Outbound publishing: create GitHub issues and write wiki pages. These are
/// the only operations that push user-authored content off the machine, and
/// each one happens solely on an explicit button press.
pub struct PublishUseCases {
    repositories: Arc<dyn RepositoryRepository>,
    issues: Arc<dyn IssuePublisher>,
    git: Arc<dyn GitSync>,
}

impl PublishUseCases {
    pub fn new(
        repositories: Arc<dyn RepositoryRepository>,
        issues: Arc<dyn IssuePublisher>,
        git: Arc<dyn GitSync>,
    ) -> Self {
        Self {
            repositories,
            issues,
            git,
        }
    }

    /// Create a GitHub issue; returns its URL.
    pub async fn create_issue(
        &self,
        spec: &str,
        title: &str,
        body: &str,
    ) -> DomainResult<String> {
        let spec = normalize_github_spec(spec).ok_or_else(|| {
            DomainError::Validation(format!(
                "cannot parse GitHub repository from \"{spec}\" — use owner/repo"
            ))
        })?;
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::Validation("issue title is empty".into()));
        }
        self.issues.create_issue(&spec, title, body.trim()).await
    }

    /// Write (create or update) a page in a cloned wiki repository and push
    /// it. Returns the page file name. Callers should re-index afterwards so
    /// the new page becomes searchable.
    pub fn write_wiki_page(
        &self,
        repository_id: &str,
        title: &str,
        content: &str,
    ) -> DomainResult<String> {
        let repository = self.repositories.find_by_id(repository_id)?;
        ensure_wiki_repository(&repository)?;
        let slug = wiki_page_slug(title).ok_or_else(|| {
            DomainError::Validation("page title must contain letters or digits".into())
        })?;
        if content.trim().is_empty() {
            return Err(DomainError::Validation("page content is empty".into()));
        }

        // Best effort: reduce push conflicts; a failing pull (e.g. offline)
        // surfaces at push time anyway.
        self.git.pull(&repository.root_path).ok();

        let file_name = format!("{slug}.md");
        let path = std::path::Path::new(&repository.root_path).join(&file_name);
        std::fs::write(&path, content)
            .map_err(|e| DomainError::Indexing(format!("write wiki page: {e}")))?;

        self.git.commit_and_push(
            &repository.root_path,
            &format!("Update wiki page: {}", title.trim()),
        )?;
        Ok(file_name)
    }

    /// Wiki repositories of a workspace (targets for page publishing).
    pub fn wiki_repositories(&self, workspace_id: &str) -> DomainResult<Vec<Repository>> {
        Ok(self
            .repositories
            .list_by_workspace(workspace_id)?
            .into_iter()
            .filter(is_wiki_repository)
            .collect())
    }
}

fn is_wiki_repository(repository: &Repository) -> bool {
    repository.source_kind == SourceKind::Git
        && repository
            .remote_url
            .as_deref()
            .is_some_and(|url| url.trim_end_matches('/').ends_with(".wiki.git")
                || url.trim_end_matches('/').ends_with(".wiki"))
}

fn ensure_wiki_repository(repository: &Repository) -> DomainResult<()> {
    if !is_wiki_repository(repository) {
        return Err(DomainError::Validation(format!(
            "\"{}\" is not a cloned wiki — clone the wiki first ({}.wiki.git)",
            repository.name,
            repository
                .remote_url
                .as_deref()
                .unwrap_or("https://github.com/owner/repo")
                .trim_end_matches(".git")
        )));
    }
    Ok(())
}

/// GitHub wiki file naming: "Deployment Guide" -> "Deployment-Guide".
fn wiki_page_slug(title: &str) -> Option<String> {
    let slug: String = title
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.chars().any(|c| c.is_alphanumeric()) {
        Some(slug)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FakePublisher;

    #[async_trait]
    impl IssuePublisher for FakePublisher {
        async fn create_issue(&self, spec: &str, title: &str, _body: &str) -> DomainResult<String> {
            Ok(format!("https://github.com/{spec}/issues/7?t={title}"))
        }
    }

    #[derive(Default)]
    struct FakeGit {
        pushed: Mutex<Vec<String>>,
    }

    impl GitSync for FakeGit {
        fn pull(&self, _repo_path: &str) -> DomainResult<()> {
            Ok(())
        }
        fn commit_and_push(&self, _repo_path: &str, message: &str) -> DomainResult<()> {
            self.pushed.lock().unwrap().push(message.to_string());
            Ok(())
        }
    }

    struct OneRepo(Repository);

    impl RepositoryRepository for OneRepo {
        fn create(&self, _r: &Repository) -> DomainResult<()> {
            unimplemented!()
        }
        fn find_by_id(&self, _id: &str) -> DomainResult<Repository> {
            Ok(self.0.clone())
        }
        fn list_by_workspace(&self, _w: &str) -> DomainResult<Vec<Repository>> {
            Ok(vec![self.0.clone()])
        }
        fn delete(&self, _id: &str) -> DomainResult<()> {
            unimplemented!()
        }
    }

    fn wiki_repo(dir: &std::path::Path) -> Repository {
        Repository {
            id: "r1".into(),
            workspace_id: "w1".into(),
            name: "app.wiki".into(),
            root_path: dir.to_string_lossy().to_string(),
            remote_url: Some("https://github.com/acme/app.wiki.git".into()),
            source_kind: SourceKind::Git,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn slugifies_wiki_titles() {
        assert_eq!(wiki_page_slug("Deployment Guide").as_deref(), Some("Deployment-Guide"));
        assert_eq!(wiki_page_slug("  API v2:設計  ").as_deref(), Some("API-v2設計"));
        assert_eq!(wiki_page_slug("!!!"), None);
    }

    #[tokio::test]
    async fn creates_issue_with_normalized_spec() {
        let dir = std::env::temp_dir();
        let publish = PublishUseCases::new(
            Arc::new(OneRepo(wiki_repo(&dir))),
            Arc::new(FakePublisher),
            Arc::new(FakeGit::default()),
        );
        let url = publish
            .create_issue("https://github.com/acme/app", "Bug: crash", "details")
            .await
            .unwrap();
        assert!(url.starts_with("https://github.com/acme/app/issues/"));
        assert!(publish.create_issue("acme/app", "  ", "x").await.is_err());
        assert!(publish.create_issue("not a spec", "t", "x").await.is_err());
    }

    #[test]
    fn writes_wiki_page_and_pushes() {
        let dir = std::env::temp_dir().join(format!("cbnb-wiki-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let git = Arc::new(FakeGit::default());
        let publish = PublishUseCases::new(
            Arc::new(OneRepo(wiki_repo(&dir))),
            Arc::new(FakePublisher),
            git.clone(),
        );
        let file = publish
            .write_wiki_page("r1", "Deployment Guide", "# How to deploy\n\nSteps…")
            .unwrap();
        assert_eq!(file, "Deployment-Guide.md");
        let written = std::fs::read_to_string(dir.join(&file)).unwrap();
        assert!(written.contains("How to deploy"));
        assert_eq!(git.pushed.lock().unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_non_wiki_repositories() {
        let dir = std::env::temp_dir();
        let mut repo = wiki_repo(&dir);
        repo.remote_url = Some("https://github.com/acme/app.git".into());
        let publish = PublishUseCases::new(
            Arc::new(OneRepo(repo)),
            Arc::new(FakePublisher),
            Arc::new(FakeGit::default()),
        );
        assert!(publish.write_wiki_page("r1", "Page", "content").is_err());
    }
}
