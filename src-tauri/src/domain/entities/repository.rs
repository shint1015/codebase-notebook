use serde::{Deserialize, Serialize};

/// A source tree inside a workspace. A workspace can hold any number of
/// repositories — local folders or clones managed by the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub workspace_id: String,
    /// Unique within the workspace; used as the path prefix in citations
    /// (e.g. "backend/src/main.rs").
    pub name: String,
    pub root_path: String,
    /// Set when the repository was cloned from a remote URL; such clones live
    /// in the app-managed directory and are removed together with the entry.
    pub remote_url: Option<String>,
    pub created_at: String,
}
