use serde::{Deserialize, Serialize};

/// Supported AI provider kinds. `Ollama` is the local default; everything else
/// is an optional BYOK external provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Ollama,
    OpenAi,
    Anthropic,
    /// Any OpenAI-compatible endpoint (LM Studio, vLLM, in-house gateways...).
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Ollama => "ollama",
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAiCompatible => "openai_compatible",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ollama" => Some(ProviderKind::Ollama),
            "openai" => Some(ProviderKind::OpenAi),
            "anthropic" => Some(ProviderKind::Anthropic),
            "openai_compatible" => Some(ProviderKind::OpenAiCompatible),
            _ => None,
        }
    }

    /// Local providers never send content off the machine.
    pub fn is_external(&self) -> bool {
        match self {
            ProviderKind::Ollama => false,
            // OpenAI-compatible endpoints may be local (LM Studio) but we treat
            // them conservatively as external unless proven otherwise.
            ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::OpenAiCompatible => {
                true
            }
        }
    }
}

/// Per-provider configuration. The API key itself is NOT stored here — it
/// lives in the OS keychain, referenced by the provider kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub enabled: bool,
    pub base_url: String,
    pub default_model: String,
    /// Whether code snippets may be sent to this provider at all.
    pub allow_send_code: bool,
    /// True if an API key is registered in the secret store.
    pub has_api_key: bool,
}

impl ProviderConfig {
    pub fn default_for(kind: ProviderKind) -> Self {
        let (base_url, default_model) = match kind {
            ProviderKind::Ollama => ("http://localhost:11434", "qwen2.5-coder:14b"),
            ProviderKind::OpenAi => ("https://api.openai.com/v1", "gpt-4o-mini"),
            ProviderKind::Anthropic => ("https://api.anthropic.com", "claude-sonnet-5"),
            ProviderKind::OpenAiCompatible => ("http://localhost:1234/v1", ""),
        };
        Self {
            kind,
            enabled: kind == ProviderKind::Ollama,
            base_url: base_url.to_string(),
            default_model: default_model.to_string(),
            allow_send_code: !kind.is_external(),
            has_api_key: false,
        }
    }
}
