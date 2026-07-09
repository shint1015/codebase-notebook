use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{ChatTurn, LlmProvider, TokenSink};

use super::{for_each_line, http_client, probe_client, sse_data};

/// Adapter for OpenAI and any OpenAI-compatible endpoint (LM Studio, vLLM,
/// in-house gateways). BYOK: the key is injected at call time from the OS
/// keychain and never persisted here.
pub struct OpenAiCompatProvider {
    kind: ProviderKind,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(kind: ProviderKind, base_url: &str, api_key: Option<String>) -> Self {
        Self {
            kind,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: http_client(),
        }
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String> {
        let mut messages = vec![json!({"role": "system", "content": system})];
        for turn in turns {
            messages.push(json!({"role": turn.role, "content": turn.content}));
        }
        let response = self
            .authorized(
                self.client
                    .post(format!("{}/chat/completions", self.base_url)),
            )
            .json(&json!({"model": model, "messages": messages}))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("{} request: {e}", self.kind.as_str())))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "{} returned {status}: {body}",
                self.kind.as_str()
            )));
        }
        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("{} response: {e}", self.kind.as_str())))?;
        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| DomainError::Provider("empty completion".into()))
    }

    async fn chat_stream(
        &self,
        model: &str,
        system: &str,
        turns: &[ChatTurn],
        on_token: &TokenSink,
    ) -> DomainResult<String> {
        let mut messages = vec![json!({"role": "system", "content": system})];
        for turn in turns {
            messages.push(json!({"role": turn.role, "content": turn.content}));
        }
        let response = self
            .authorized(
                self.client
                    .post(format!("{}/chat/completions", self.base_url)),
            )
            .json(&json!({"model": model, "messages": messages, "stream": true}))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("{} request: {e}", self.kind.as_str())))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "{} returned {status}: {body}",
                self.kind.as_str()
            )));
        }
        // SSE: `data: {"choices":[{"delta":{"content":"…"}}]}` per event.
        let mut full = String::new();
        for_each_line(response, |line| {
            let Some(data) = sse_data(line) else {
                return Ok(());
            };
            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                if let Some(delta) = chunk
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|c| c.delta.content)
                {
                    if !delta.is_empty() {
                        full.push_str(&delta);
                        on_token(&delta);
                    }
                }
            }
            Ok(())
        })
        .await?;
        if full.is_empty() {
            return Err(DomainError::Provider("empty streamed completion".into()));
        }
        Ok(full)
    }

    async fn test_connection(&self) -> DomainResult<String> {
        let response = self
            .authorized(probe_client().get(format!("{}/models", self.base_url)))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("unreachable: {e}")))?;
        if response.status().is_success() {
            Ok(format!("{} reachable", self.base_url))
        } else {
            Err(DomainError::Provider(format!(
                "returned {} — check base URL and API key",
                response.status()
            )))
        }
    }
}
