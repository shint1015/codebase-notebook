//! GitHub issue fetching: prefers the user's authenticated `gh` CLI (works
//! for private repos with their existing credentials), falls back to the
//! unauthenticated REST API for public repositories.

use async_trait::async_trait;
use serde::Deserialize;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{IssueDoc, IssueFetcher};

const PER_PAGE: usize = 100;
/// Safety cap: at most 10 pages (1000 issues) per fetch.
const MAX_PAGES: usize = 10;

pub struct GitHubIssueFetcher;

#[derive(Deserialize)]
struct ApiIssue {
    number: i64,
    title: String,
    state: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    created_at: String,
    user: ApiUser,
    #[serde(default)]
    labels: Vec<ApiLabel>,
    /// Present when the "issue" is actually a pull request.
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ApiUser {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize)]
struct ApiLabel {
    #[serde(default)]
    name: String,
}

fn to_doc(issue: ApiIssue) -> IssueDoc {
    IssueDoc {
        number: issue.number,
        title: issue.title,
        state: issue.state,
        author: issue.user.login,
        labels: issue.labels.into_iter().map(|l| l.name).collect(),
        body: issue.body.unwrap_or_default(),
        url: issue.html_url,
        created_at: issue.created_at,
    }
}

fn parse_issues(json: &str) -> DomainResult<Vec<ApiIssue>> {
    serde_json::from_str::<Vec<ApiIssue>>(json)
        .map_err(|e| DomainError::Indexing(format!("parse issues response: {e}")))
}

fn gh_available() -> bool {
    std::process::Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn fetch_via_gh(spec: &str) -> DomainResult<Vec<ApiIssue>> {
    let output = std::process::Command::new("gh")
        .args([
            "api",
            "--paginate",
            &format!("repos/{spec}/issues?state=all&per_page={PER_PAGE}"),
            "--slurp",
        ])
        .output()
        .map_err(|e| DomainError::Indexing(format!("run gh: {e}")))?;
    if !output.status.success() {
        return Err(DomainError::Indexing(format!(
            "gh api failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    // --slurp wraps the pages in an outer array: [[...], [...]]
    let pages: Vec<Vec<ApiIssue>> = serde_json::from_slice(&output.stdout)
        .map_err(|e| DomainError::Indexing(format!("parse gh response: {e}")))?;
    Ok(pages.into_iter().flatten().collect())
}

async fn fetch_via_rest(spec: &str) -> DomainResult<Vec<ApiIssue>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("codebase-notebook")
        .build()
        .map_err(|e| DomainError::Indexing(format!("http client: {e}")))?;
    let mut all = Vec::new();
    for page in 1..=MAX_PAGES {
        let url = format!(
            "https://api.github.com/repos/{spec}/issues?state=all&per_page={PER_PAGE}&page={page}"
        );
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| DomainError::Indexing(format!("github api: {e}")))?;
        if !response.status().is_success() {
            return Err(DomainError::Indexing(format!(
                "github api returned {} for {spec} — for private repos install and \
                 authenticate the gh CLI",
                response.status()
            )));
        }
        let body = response
            .text()
            .await
            .map_err(|e| DomainError::Indexing(format!("github api body: {e}")))?;
        let issues = parse_issues(&body)?;
        let last_page = issues.len() < PER_PAGE;
        all.extend(issues);
        if last_page {
            break;
        }
    }
    Ok(all)
}

#[async_trait]
impl IssueFetcher for GitHubIssueFetcher {
    async fn fetch_issues(&self, spec: &str) -> DomainResult<Vec<IssueDoc>> {
        let spec = spec.to_string();
        let issues = if gh_available() {
            let spec_owned = spec.clone();
            tauri::async_runtime::spawn_blocking(move || fetch_via_gh(&spec_owned))
                .await
                .map_err(|e| DomainError::Indexing(format!("gh task failed: {e}")))??
        } else {
            fetch_via_rest(&spec).await?
        };
        Ok(issues
            .into_iter()
            .filter(|issue| issue.pull_request.is_none())
            .map(to_doc)
            .collect())
    }
}
