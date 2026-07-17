use serde::{Deserialize, Serialize};

/// A workspace isolates one project (a set of repositories / documents) from
/// others. Chunks, chats and search never cross workspace boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// Whether the user allowed sending this workspace's content to external providers.
    pub allow_external: bool,
    /// Custom instructions appended to the system prompt for this workspace
    /// (tone, conventions, what to prioritise). Empty = none.
    pub instructions: String,
    pub created_at: String,
}
