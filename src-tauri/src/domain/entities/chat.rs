use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// A grounded reference from an assistant answer back to an indexed chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// 1-based marker used in the answer text, e.g. [1].
    pub marker: i64,
    pub chunk_id: String,
    pub rel_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub citations: Vec<Citation>,
    /// Which provider/model produced this message (assistant messages only).
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
}
