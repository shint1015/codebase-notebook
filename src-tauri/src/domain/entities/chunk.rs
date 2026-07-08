use serde::{Deserialize, Serialize};

/// A retrievable piece of a document. The unit of search, citation and
/// external transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub workspace_id: String,
    /// Order of the chunk within its document.
    pub seq: i64,
    pub content: String,
    pub start_line: i64,
    pub end_line: i64,
}

/// A search result: a chunk plus its source path and relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk: Chunk,
    pub rel_path: String,
    pub score: f64,
}
