use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{ChatTurn, LlmProvider};

use super::{http_client, probe_client};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 4096;

/// Anthropic Messages API adapter (BYOK).
pub struct AnthropicProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(base_url: &str, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: http_client(),
        }
    }
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(default)]
    text: String,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String> {
        let messages: Vec<_> = turns
            .iter()
            .map(|t| json!({"role": t.role, "content": t.content}))
            .collect();
        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&json!({
                "model": model,
                "system": system,
                "max_tokens": MAX_TOKENS,
                "messages": messages,
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("anthropic request: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "anthropic returned {status}: {body}"
            )));
        }
        let parsed: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("anthropic response: {e}")))?;
        Ok(parsed
            .content
            .into_iter()
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join(""))
    }

    async fn test_connection(&self) -> DomainResult<String> {
        let response = probe_client()
            .get(format!("{}/v1/models", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("anthropic unreachable: {e}")))?;
        if response.status().is_success() {
            Ok("Anthropic API key is valid".to_string())
        } else {
            Err(DomainError::Provider(format!(
                "anthropic returned {} — check the API key",
                response.status()
            )))
        }
    }
}
