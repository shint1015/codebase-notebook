use serde::{Deserialize, Serialize};

/// One model invocation: usage accounting and — for external providers —
/// the audit trail of which sources were sent off the machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub created_at: String,
    pub provider: String,
    pub model: String,
    pub workspace_id: Option<String>,
    pub prompt_chars: i64,
    pub completion_chars: i64,
    pub est_cost_usd: f64,
    /// rel_paths of the chunks included in the prompt.
    pub sources: Vec<String>,
}
