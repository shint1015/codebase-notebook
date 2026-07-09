use serde::{Deserialize, Serialize};

/// How a repository's content got here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A local folder or single file registered in place.
    Local,
    /// A git clone managed by the app (includes GitHub wikis — they are
    /// plain git repositories at `<repo>.wiki.git`).
    Git,
    /// GitHub issues materialized as markdown files by the app.
    GithubIssues,
}

impl SourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceKind::Local => "local",
            SourceKind::Git => "git",
            SourceKind::GithubIssues => "github_issues",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "git" => SourceKind::Git,
            "github_issues" => SourceKind::GithubIssues,
            _ => SourceKind::Local,
        }
    }

    /// App-managed sources live under the clones dir and are deleted with
    /// their repository entry.
    pub fn is_managed(&self) -> bool {
        matches!(self, SourceKind::Git | SourceKind::GithubIssues)
    }
}

/// A source tree inside a workspace. A workspace can hold any number of
/// repositories — local folders/files, clones, or fetched issue sets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub workspace_id: String,
    /// Unique within the workspace; used as the path prefix in citations
    /// (e.g. "backend/src/main.rs").
    pub name: String,
    pub root_path: String,
    /// Remote origin (git URL or GitHub repo) for managed sources.
    pub remote_url: Option<String>,
    pub source_kind: SourceKind,
    pub created_at: String,
}
