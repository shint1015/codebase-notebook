use std::sync::Arc;

use crate::domain::entities::provider::{ProviderConfig, ProviderKind};
use crate::domain::error::{DomainError, DomainResult};
use crate::domain::repositories::ProviderConfigRepository;
use crate::domain::services::{ProviderRouter, SecretStore};

pub struct ProviderUseCases {
    configs: Arc<dyn ProviderConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    router: Arc<dyn ProviderRouter>,
}

pub const ALL_PROVIDER_KINDS: [ProviderKind; 4] = [
    ProviderKind::Ollama,
    ProviderKind::OpenAi,
    ProviderKind::Anthropic,
    ProviderKind::OpenAiCompatible,
];

impl ProviderUseCases {
    pub fn new(
        configs: Arc<dyn ProviderConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        router: Arc<dyn ProviderRouter>,
    ) -> Self {
        Self {
            configs,
            secrets,
            router,
        }
    }

    /// All provider kinds with stored config or defaults, key presence included.
    pub fn list(&self) -> DomainResult<Vec<ProviderConfig>> {
        ALL_PROVIDER_KINDS
            .iter()
            .map(|&kind| {
                let mut config = self
                    .configs
                    .find(kind)?
                    .unwrap_or_else(|| ProviderConfig::default_for(kind));
                config.has_api_key = self.secrets.get_api_key(kind)?.is_some();
                Ok(config)
            })
            .collect()
    }

    /// Save configuration; the API key goes to the OS keychain, never the DB.
    pub fn configure(
        &self,
        mut config: ProviderConfig,
        api_key: Option<String>,
    ) -> DomainResult<ProviderConfig> {
        if let Some(key) = api_key {
            let key = key.trim();
            if key.is_empty() {
                self.secrets.delete_api_key(config.kind)?;
            } else {
                self.secrets.set_api_key(config.kind, key)?;
            }
        }
        if config.kind.is_external()
            && config.enabled
            && config.kind != ProviderKind::OpenAiCompatible
            && self.secrets.get_api_key(config.kind)?.is_none()
        {
            return Err(DomainError::Validation(format!(
                "{} requires an API key before it can be enabled",
                config.kind.as_str()
            )));
        }
        config.has_api_key = self.secrets.get_api_key(config.kind)?.is_some();
        self.configs.upsert(&config)?;
        Ok(config)
    }

    pub async fn test_connection(&self, kind: ProviderKind) -> DomainResult<String> {
        let provider = self.router.resolve(kind)?;
        provider.test_connection().await
    }
}
