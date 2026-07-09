use serde::{Deserialize, Serialize};

/// A single indexed source file inside a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub workspace_id: String,
    pub repository_id: String,
    /// Repository name + path relative to the repository root,
    /// e.g. "backend/src/main.rs". Unique within a workspace.
    pub rel_path: String,
    pub language: String,
    /// Content hash used for incremental re-indexing.
    pub content_hash: String,
    pub indexed_at: String,
}
