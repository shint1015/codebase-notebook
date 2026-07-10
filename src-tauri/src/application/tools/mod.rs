//! Agent tools: capabilities the model can invoke during a chat. Read tools
//! run freely; write tools set `requires_consent` and are gated by the agent
//! loop behind explicit user approval.

pub mod github;
pub mod search;

use serde_json::Value;

/// Extract a required string argument, with a clear error for the model.
pub(crate) fn require_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing required string argument \"{key}\""))
}

pub(crate) fn optional_str(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}
