use serde::{Deserialize, Serialize};

/// A single indexed source file inside a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub workspace_id: String,
    /// Path relative to the workspace root.
    pub rel_path: String,
    pub language: String,
    /// Content hash used for incremental re-indexing.
    pub content_hash: String,
    pub indexed_at: String,
}
