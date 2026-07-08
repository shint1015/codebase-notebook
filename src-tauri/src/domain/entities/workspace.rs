use serde::{Deserialize, Serialize};

/// A workspace isolates one project (repository / document set) from others.
/// Chunks, chats and search never cross workspace boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    /// Whether the user allowed sending this workspace's content to external providers.
    pub allow_external: bool,
    pub created_at: String,
}
