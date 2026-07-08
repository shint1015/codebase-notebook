use std::sync::Arc;

use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::ProviderConfigRepository;
use crate::domain::services::{LlmProvider, ProviderRouter, SecretStore};

use super::anthropic::AnthropicProvider;
use super::gemini::GeminiProvider;
use super::ollama::OllamaProvider;
use super::openai_compat::OpenAiCompatProvider;

/// Model Router: resolves a provider kind to a concrete adapter using the
/// current configuration and (for external providers) the keychain-held key.
/// Adding a new provider means adding one adapter and one match arm here —
/// nothing else in the app changes.
pub struct ConfiguredProviderRouter {
    configs: Arc<dyn ProviderConfigRepository>,
    secrets: Arc<dyn SecretStore>,
}

impl ConfiguredProviderRouter {
    pub fn new(
        configs: Arc<dyn ProviderConfigRepository>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self { configs, secrets }
    }

    fn config_for(&self, kind: ProviderKind) -> DomainResult<ProviderConfig> {
        Ok(self
            .configs
            .find(kind)?
            .unwrap_or_else(|| ProviderConfig::default_for(kind)))
    }

    fn required_key(&self, kind: ProviderKind) -> DomainResult<String> {
        self.secrets.get_api_key(kind)?.ok_or_else(|| {
            DomainError::ProviderNotConfigured(format!("{} API key not set", kind.as_str()))
        })
    }
}

impl ProviderRouter for ConfiguredProviderRouter {
    fn resolve(&self, kind: ProviderKind) -> DomainResult<Arc<dyn LlmProvider>> {
        let config = self.config_for(kind)?;
        match kind {
            ProviderKind::Ollama => Ok(Arc::new(OllamaProvider::new(&config.base_url))),
            // Mistral and xAI expose fully OpenAI-compatible chat APIs, so
            // they share the adapter and differ only in base URL and key.
            ProviderKind::OpenAi | ProviderKind::Mistral | ProviderKind::XAi => {
                let key = self.required_key(kind)?;
                Ok(Arc::new(OpenAiCompatProvider::new(
                    kind,
                    &config.base_url,
                    Some(key),
                )))
            }
            ProviderKind::OpenAiCompatible => {
                let key = self.secrets.get_api_key(kind)?;
                Ok(Arc::new(OpenAiCompatProvider::new(
                    kind,
                    &config.base_url,
                    key,
                )))
            }
            ProviderKind::Anthropic => {
                let key = self.required_key(kind)?;
                Ok(Arc::new(AnthropicProvider::new(&config.base_url, key)))
            }
            ProviderKind::Gemini => {
                let key = self.required_key(kind)?;
                Ok(Arc::new(GeminiProvider::new(&config.base_url, key)))
            }
        }
    }
}
