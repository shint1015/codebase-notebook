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

/// Deterministic assessment of how well an assistant answer is backed by its
/// retrieved sources. Computed from the answer text alone — no extra LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grounding {
    /// Prose claim units (paragraphs/bullets outside code blocks).
    pub total_claims: i64,
    /// Claim units that carry at least one valid citation marker.
    pub cited_claims: i64,
    /// Markers like [9] that do not map to any retrieved source.
    pub invalid_markers: Vec<i64>,
}

impl Grounding {
    /// "grounded" | "partial" | "ungrounded"
    pub fn verdict(&self) -> &'static str {
        if self.total_claims == 0 {
            return "grounded";
        }
        if self.cited_claims == 0 {
            return "ungrounded";
        }
        if self.cited_claims == self.total_claims && self.invalid_markers.is_empty() {
            "grounded"
        } else {
            "partial"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub citations: Vec<Citation>,
    /// Present on assistant messages produced with retrieval (None otherwise).
    #[serde(default)]
    pub grounding: Option<Grounding>,
    /// Which provider/model produced this message (assistant messages only).
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
}
