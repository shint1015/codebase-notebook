pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai_compat;
pub mod router;

use std::time::Duration;

use crate::domain::error::{DomainError, DomainResult};

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
