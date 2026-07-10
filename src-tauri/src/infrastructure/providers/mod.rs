pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai_compat;
pub mod router;

use std::time::Duration;

use serde_json::json;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{ChatTurn, ToolCall, ToolSpec};

/// Serialize turns to OpenAI/Ollama chat message shape, including tool calls
/// on assistant turns and tool results on tool turns.
pub(crate) fn openai_messages(system: &str, turns: &[ChatTurn]) -> Vec<serde_json::Value> {
    let mut messages = vec![json!({"role": "system", "content": system})];
    for turn in turns {
        if turn.role == "tool" {
            messages.push(json!({
                "role": "tool",
                "content": turn.content,
                "tool_call_id": turn.tool_call_id,
            }));
        } else if !turn.tool_calls.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": turn.content,
                "tool_calls": turn.tool_calls.iter().map(|call| json!({
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": call.arguments.to_string(),
                    },
                })).collect::<Vec<_>>(),
            }));
        } else {
            messages.push(json!({"role": turn.role, "content": turn.content}));
        }
    }
    messages
}

/// Tool specs in OpenAI/Ollama `tools` shape.
pub(crate) fn openai_tools(tools: &[ToolSpec]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            })
        })
        .collect()
}

/// Fallback for models (e.g. some Ollama templates for qwen2.5-coder) that
/// emit a tool call as JSON in the message content instead of `tool_calls`.
/// Recognizes `{"name": "...", "arguments": {...}}`, optionally fenced.
pub(crate) fn tool_calls_from_content(content: &str) -> Vec<ToolCall> {
    let trimmed = content.trim();
    // Strip a ```json … ``` fence if present.
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_end_matches("```").trim())
        .unwrap_or(trimmed);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(inner) else {
        return Vec::new();
    };
    // Accept a single object or an array of them.
    let candidates: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Array(items) => items.iter().collect(),
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return Vec::new(),
    };
    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(index, obj)| {
            let name = obj.get("name")?.as_str()?.to_string();
            let arguments = obj
                .get("arguments")
                .or_else(|| obj.get("parameters"))
                .cloned()
                .unwrap_or(json!({}));
            Some(ToolCall {
                id: format!("call_{index}"),
                name,
                arguments,
            })
        })
        .collect()
}

/// Parse an OpenAI/Ollama `tool_calls` array into domain ToolCalls.
/// `arguments` may arrive as a JSON string or an object.
pub(crate) fn parse_tool_calls(value: &serde_json::Value) -> Vec<ToolCall> {
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .filter_map(|(index, call)| {
            let function = call.get("function")?;
            let name = function.get("name")?.as_str()?.to_string();
            let raw = function.get("arguments").cloned().unwrap_or(json!({}));
            let arguments = match raw {
                serde_json::Value::String(s) => {
                    serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(Default::default()))
                }
                other => other,
            };
            let id = call
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("call_{index}"));
            Some(ToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

/// Drive a line-delimited HTTP response (JSONL or SSE), invoking `handle`
/// for every non-empty line as it arrives.
pub(crate) async fn for_each_line<F>(
    response: reqwest::Response,
    mut handle: F,
) -> DomainResult<()>
where
    F: FnMut(&str) -> DomainResult<()>,
{
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DomainError::Provider(format!("stream read: {e}")))?;
        buffer.extend_from_slice(&chunk);
        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();
            if !line.is_empty() {
                handle(line)?;
            }
        }
    }
    let tail = String::from_utf8_lossy(&buffer);
    let tail = tail.trim();
    if !tail.is_empty() {
        handle(tail)?;
    }
    Ok(())
}

/// Strip an SSE "data:" prefix; returns None for non-data lines and [DONE].
pub(crate) fn sse_data(line: &str) -> Option<&str> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        None
    } else {
        Some(data)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_tool_calls, tool_calls_from_content};
    use serde_json::json;

    #[test]
    fn parses_openai_tool_calls_with_string_arguments() {
        let value = json!([{
            "id": "call_9",
            "function": {"name": "search_sources", "arguments": "{\"query\":\"auth\"}"}
        }]);
        let calls = parse_tool_calls(&value);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_sources");
        assert_eq!(calls[0].arguments["query"], "auth");
    }

    #[test]
    fn recovers_tool_call_from_content_json() {
        let content = r#"{"name": "search_sources", "arguments": {"query": "how auth works"}}"#;
        let calls = tool_calls_from_content(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_sources");
        assert_eq!(calls[0].arguments["query"], "how auth works");
    }

    #[test]
    fn recovers_tool_call_from_fenced_content() {
        let content = "```json\n{\"name\": \"create_github_issue\", \"arguments\": {\"repo\": \"a/b\", \"title\": \"x\"}}\n```";
        let calls = tool_calls_from_content(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "create_github_issue");
    }

    #[test]
    fn plain_text_content_yields_no_calls() {
        assert!(tool_calls_from_content("Here is the answer, no tools.").is_empty());
    }
}

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client")
}

pub(crate) fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("reqwest client")
}
