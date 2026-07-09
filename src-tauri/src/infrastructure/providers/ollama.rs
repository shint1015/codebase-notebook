use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::services::{ChatTurn, EmbeddingProvider, LlmProvider, TokenSink};

use super::{for_each_line, http_client, probe_client};

/// Local inference via the Ollama HTTP API. This is the default provider —
/// nothing ever leaves the machine.
pub struct OllamaProvider {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: http_client(),
        }
    }
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
}

#[derive(Deserialize)]
struct OllamaChatMessage {
    content: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    async fn chat(&self, model: &str, system: &str, turns: &[ChatTurn]) -> DomainResult<String> {
        let mut messages = vec![json!({"role": "system", "content": system})];
        for turn in turns {
            messages.push(json!({"role": turn.role, "content": turn.content}));
        }
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&json!({
                "model": model,
                "messages": messages,
                "stream": false,
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama request: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "ollama returned {status}: {body}"
            )));
        }
        let parsed: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama response: {e}")))?;
        Ok(parsed.message.content)
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
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&json!({
                "model": model,
                "messages": messages,
                "stream": true,
            }))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama request: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::Provider(format!(
                "ollama returned {status}: {body}"
            )));
        }
        // JSONL: one {"message":{"content":"…"},"done":bool} object per line.
        let mut full = String::new();
        for_each_line(response, |line| {
            if let Ok(parsed) = serde_json::from_str::<OllamaChatResponse>(line) {
                if !parsed.message.content.is_empty() {
                    full.push_str(&parsed.message.content);
                    on_token(&parsed.message.content);
                }
            }
            Ok(())
        })
        .await?;
        Ok(full)
    }

    async fn test_connection(&self) -> DomainResult<String> {
        let response = probe_client()
            .get(format!("{}/api/version", self.base_url))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama unreachable: {e}")))?;
        if response.status().is_success() {
            Ok(format!("Ollama reachable at {}", self.base_url))
        } else {
            Err(DomainError::Provider(format!(
                "ollama returned {}",
                response.status()
            )))
        }
    }
}

/// Embeddings via Ollama's /api/embed. Availability is probed cheaply so the
/// app silently degrades to keyword-only search when Ollama (or the model)
/// is absent. The model is resolved from settings on every call so users can
/// switch (e.g. to bge-m3 for Japanese) without restarting.
pub struct OllamaEmbedding {
    base_url: String,
    default_model: String,
    settings: std::sync::Arc<dyn crate::domain::services::SettingsRepository>,
    client: reqwest::Client,
}

impl OllamaEmbedding {
    pub fn new(
        base_url: &str,
        default_model: &str,
        settings: std::sync::Arc<dyn crate::domain::services::SettingsRepository>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            default_model: default_model.to_string(),
            settings,
            client: http_client(),
        }
    }

    fn model(&self) -> String {
        self.settings
            .get("embedding_model")
            .ok()
            .flatten()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| self.default_model.clone())
    }
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelTag>,
}

#[derive(Deserialize)]
struct OllamaModelTag {
    name: String,
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn embed(&self, texts: &[String]) -> DomainResult<Vec<Vec<f32>>> {
        let response = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .json(&json!({"model": self.model(), "input": texts}))
            .send()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama embed: {e}")))?;
        if !response.status().is_success() {
            return Err(DomainError::Provider(format!(
                "ollama embed returned {}",
                response.status()
            )));
        }
        let parsed: OllamaEmbedResponse = response
            .json()
            .await
            .map_err(|e| DomainError::Provider(format!("ollama embed response: {e}")))?;
        Ok(parsed.embeddings)
    }

    async fn is_available(&self) -> bool {
        let Ok(response) = probe_client()
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
        else {
            return false;
        };
        let Ok(tags) = response.json::<OllamaTagsResponse>().await else {
            return false;
        };
        let wanted = self.model();
        let wanted = wanted.as_str();
        tags.models
            .iter()
            .any(|m| m.name == wanted || m.name.starts_with(&format!("{wanted}:")))
    }
}
