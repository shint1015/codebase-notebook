use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{ChatTurn, LlmProvider};

use super::{http_client, probe_client};

/// Google Gemini adapter (Generative Language API, BYOK).
pub struct GeminiProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(base_url: &str, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: http_client(),
        }
    }
}

#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: String,
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String> {
        // Gemini uses "model" instead of "assistant" for the reply role.
        let contents: Vec<_> = turns
            .iter()
            .map(|t| {
                let role = if t.role == "assistant" { "model" } else { "user" };
                json!({"role": role, "parts": [{"text": t.content}]})
            })
            .collect();
        let response = self
            .client
            .post(format!(
                "{}/v1beta/models/{}:generateContent",
                self.base_url, model
            ))
            .header("x-goog-api-key", &self.api_key)
            .json(&json!({
                "systemInstruction": {"parts": [{"text": system}]},
                "contents": contents,
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("gemini request: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "gemini returned {status}: {body}"
            )));
        }
        let parsed: GeminiResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("gemini response: {e}")))?;
        let text = parsed
            .candidates
            .into_iter()
            .next()
            .map(|c| {
                c.content
                    .parts
                    .into_iter()
                    .map(|p| p.text)
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.is_empty() {
            return Err(DomainError::Provider("empty gemini completion".into()));
        }
        Ok(text)
    }

    async fn test_connection(&self) -> DomainResult<String> {
        let response = probe_client()
            .get(format!("{}/v1beta/models", self.base_url))
            .header("x-goog-api-key", &self.api_key)
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("gemini unreachable: {e}")))?;
        if response.status().is_success() {
            Ok("Gemini API key is valid".to_string())
        } else {
            Err(DomainError::Provider(format!(
                "gemini returned {} — check the API key",
                response.status()
            )))
        }
    }
}
